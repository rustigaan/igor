use std::borrow::Cow;
use std::hash::Hash;
use std::io::ErrorKind;
use std::ops::Add;
use anyhow::{anyhow, bail, Result};
use std::path::Path;
use ahash::{AHashMap, AHashSet};
use log::{debug, info, trace, warn};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use serde::Deserialize;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::fs::{DirBuilder, File, OpenOptions};
use tokio::sync::mpsc::{channel, Sender, Receiver};
use crate::config_model::{InvarConfig, ThunderConfig, WriteMode};
use crate::path::{AbsolutePath, RelativePath, SingleComponent};
use crate::thundercloud::Thumbs::{FromBothCumulusAndInvar, FromCumulus, FromInvar};

#[derive(Deserialize,Debug)]
struct ThundercloudConfig {
    niche: NicheConfig,
    #[serde(rename = "invar-defaults")]
    invar_defaults: Option<InvarConfig>
}

#[derive(Deserialize,Debug)]
struct NicheConfig {
    name: String,
    description: Option<String>,
}

pub async fn process_niche(thunder_config: ThunderConfig) -> Result<()> {
    let thundercloud_directory = thunder_config.thundercloud_directory();
    let cumulus = thunder_config.cumulus();
    let invar = thunder_config.invar();
    let project_root = thunder_config.project_root();
    info!("Apply: {:?} ⊕ {:?} ⇒ {:?}", cumulus, invar, project_root);
    let config = get_config(thundercloud_directory)?;
    let niche = &config.niche;
    info!("Thundercloud: {:?}: {:?}", niche.name, niche.description.as_ref().unwrap_or(&"-".to_string()));
    debug!("Use thundercloud: {:?}", thunder_config.use_thundercloud());
    let current_directory = RelativePath::from(".");
    let invar_config = if let Some(invar_config) = &config.invar_defaults {
        Cow::Borrowed(invar_config)
    } else {
        Cow::Owned(InvarConfig::new())
    };
    visit_subtree(&current_directory, FromBothCumulusAndInvar, &thunder_config, invar_config.as_ref()).await?;
    Ok(())
}

fn get_config(thundercloud_directory: &AbsolutePath) -> Result<ThundercloudConfig> {
    let mut config_path = thundercloud_directory.clone();
    config_path.push("thundercloud.yaml");
    info!("Config path: {config_path:?}");

    let file = std::fs::File::open(&*config_path)?;
    let config = serde_yaml::from_reader(file)?;
    debug!("Thundercloud configuration: {config:?}");
    Ok(config)
}

#[derive(Debug, Clone)]
struct BoltCore {
    base_name: String,
    extension: String,
    feature_name: String,
    source: AbsolutePath,
}

#[derive(Debug, Clone)]
enum Bolt {
    Option(BoltCore),
    Fragment {
        bolt_core: BoltCore,
        qualifier: Option<String>
    },
    Config(BoltCore),
    Unknown {
        bolt_core: BoltCore,
        qualifier: Option<String>
    },
}

impl Bolt {
    fn bolt_core(&self) -> &BoltCore {
        match self {
            Bolt::Option(bolt_core) => bolt_core,
            Bolt::Config(bolt_core) => bolt_core,
            Bolt::Fragment { bolt_core, .. } => bolt_core,
            Bolt::Unknown { bolt_core, .. } => bolt_core,
        }
    }
    fn kind_name(&self) -> &'static str {
        match self {
            Bolt::Option(_) => "option",
            Bolt::Config(_) => " config",
            Bolt::Fragment { .. } => "fragment",
            Bolt::Unknown { .. } => "unknown",
        }
    }
    fn base_name(&self) -> String {
        self.bolt_core().base_name.clone()
    }
    fn extension(&self) -> String {
        self.bolt_core().extension.clone()
    }
    fn target_name(&self) -> String {
        let result = self.base_name().clone();
        result.add(self.extension().as_str())
    }
    fn feature_name(&self) -> String {
        self.bolt_core().feature_name.clone()
    }
    fn source(&self) -> &AbsolutePath {
        &self.bolt_core().source
    }
    fn qualifier(&self) -> Option<String> {
        match self {
            Bolt::Fragment { qualifier, .. } => qualifier.clone(),
            Bolt::Unknown { qualifier, .. } => qualifier.clone(),
            _ => None
        }
    }
}

static BOLT_REGEX_WITH_DOT: Lazy<Regex> = Lazy::new(|| {
    Regex::new("^(?<base>.*)[+](?<bolt_type>[a-z0-9_]+)(-(?<feature>[a-z0-9_]+|@)(-(?<qualifier>[a-z0-9_]+))?)?(?<extension>[.][^.]*)$").unwrap()
});
static BOLT_REGEX_WITHOUT_DOT: Lazy<Regex> = Lazy::new(|| {
    Regex::new("^(?<base>[^.]+)[+](?<bolt_type>[a-z0-9_]+)(-(?<feature>[a-z0-9_]+|@)(-(?<qualifier>[a-z0-9_]+))?)?$").unwrap()
});
static PLAIN_FILE_REGEX_WITH_DOT: Lazy<Regex> = Lazy::new(|| {
    Regex::new("^(?<base>.*)(?<extension>[.][^.]*)").unwrap()
});
static ILLEGAL_FILE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new("^([.][.]?)?$").unwrap()
});

static FRAGMENT_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new("==== (?<bracket>(BEGIN|END) )?FRAGMENT (?<feature>[a-z0-9_]+|@)(-(?<qualifier>[a-z0-9_]+))? ====").unwrap()
});

#[derive(Clone, Copy)]
enum Thumbs {
    FromCumulus,
    FromInvar,
    FromBothCumulusAndInvar,
}

impl Thumbs {
    fn visit_cumulus(&self) -> bool {
        match self {
            FromCumulus => true,
            FromBothCumulusAndInvar => true,
            _ => false,
        }
    }
    fn visit_invar(&self) -> bool {
        match self {
            FromInvar => true,
            FromBothCumulusAndInvar => true,
            _ => false,
        }
    }
}

async fn visit_subtree(directory: &RelativePath, thumbs: Thumbs, thunder_config: &ThunderConfig, invar_config: &InvarConfig) -> Result<()> {
    let (cumulus_bolts, cumulus_subdirectories) =
        try_visit_directory(thumbs.visit_cumulus(), ThunderConfig::cumulus, thunder_config, directory).await?;
    let (invar_bolts, invar_subdirectories) =
        try_visit_directory(thumbs.visit_invar(), ThunderConfig::invar, thunder_config, directory).await?;

    let bolts = combine(cumulus_bolts, invar_bolts);
    for (key, bolt_lists) in &bolts {
        debug!("Bolts entry: {:?}: {:?}", key, bolt_lists);
    }

    generate_files(&directory, bolts, thunder_config, invar_config).await?;

    visit_subdirectories(directory, cumulus_subdirectories, invar_subdirectories, thunder_config, invar_config).await?;

    Ok(())
}

async fn generate_files(directory: &RelativePath, bolts: AHashMap<String, (Vec<Bolt>,Vec<Bolt>)>, thunder_config: &ThunderConfig, invar_config: &InvarConfig) -> Result<()> {
    let mut bolts = bolts;
    let mut use_config = Cow::Borrowed(invar_config);
    if let Some(dir_bolts) = bolts.remove(".") {
        let (_, dir_bolt_list) = combine_and_filter_bolt_lists(&dir_bolts.0, &dir_bolts.1, thunder_config);
        use_config = update_invar_config(invar_config, &dir_bolt_list).await?;
    }
    let bolts = bolts;

    let target_directory = directory.relative_to(thunder_config.project_root());
    debug!("Generate files in {:?} with config {:?}", &target_directory, &use_config);
    for (name, bolt_lists) in &bolts {
        if ILLEGAL_FILE_REGEX.is_match(name) {
            warn!("Target filename is not legal: {name:?}");
            continue;
        }
        let target_file = RelativePath::from(name as &str).relative_to(&target_directory);
        let half_config = update_invar_config(invar_config, &bolt_lists.0).await?;
        let whole_config = update_invar_config(half_config.as_ref(), &bolt_lists.1).await?;
        let (option, bolts) = combine_and_filter_bolt_lists(&bolt_lists.0, &bolt_lists.1, thunder_config);
        generate_file(&target_file, option, bolts, whole_config.as_ref()).await?;
    }
    Ok(())
}

async fn generate_file(target_file: &AbsolutePath, option: Option<Bolt>, bolts: Vec<Bolt>, invar_config: &InvarConfig) -> Result<()> {
    if bolts.is_empty() {
        debug!("Skip: {:?}: {:?}", target_file, &bolts);
        return Ok(())
    }
    let option =
        if let Some(option) = option {
            option
        } else {
            debug!("Skip (only fragments): {:?}: {:?}", target_file, &bolts);
            return Ok(())
        }
    ;
    if invar_config.write_mode() == WriteMode::Ignore {
        debug!("Ignore: {:?}: {:?}: {:?}", target_file, &bolts, &invar_config);
        return Ok(())
    }
    if let Some(target) = open_target(target_file.to_owned(), invar_config).await? {
        debug!("Generate: {:?}: {:?}: {:?}", target_file, &bolts, &invar_config);
        let (tx, rx) = channel(10);
        let join_handle = tokio::task::spawn(file_writer(rx, target));
        generate_option(option, bolts, invar_config, &tx).await?;
        drop(tx);
        join_handle.await??;
    } else {
        debug!("Skip (target exists): {:?}: {:?}: {:?}", target_file, &bolts, &invar_config);
    }
    Ok(())
}

async fn generate_option(option: Bolt, fragments: Vec<Bolt>, invar_config: &InvarConfig, tx: &Sender<String>) -> Result<()> {
    debug!("Generating option: {:?}: {:?}: {:?}", &option, &fragments, invar_config);
    let source = option.source();
    let file = File::open(source.as_path()).await?;
    let buffered_reader = BufReader::new(file);
    let mut lines = buffered_reader.lines();
    while let Some(mut line) = lines.next_line().await? {
        if let Some(captures) = FRAGMENT_REGEX.captures(&line) {
            let feature = captures.name("feature").map(|m|m.as_str().to_string()).unwrap_or("@".to_string());
            let qualifier = captures.name("qualifier").map(|m|m.as_str().to_string()).unwrap_or("".to_string());
            debug!("Found fragment: {:?}: {:?}", &feature, &qualifier);
            if let Some(bracket) = captures.name("bracket") {
                if bracket.as_str() == "BEGIN " {
                    skip_to_end_of_fragment(&mut lines, &feature, &qualifier).await?;
                }
            }
            find_and_include_fragment(&feature, &qualifier, tx, &fragments, invar_config).await?;
            continue;
        }
        line.push('\n');
        tx.send(line).await?;
    }
    Ok(())
}

async fn skip_to_end_of_fragment<T>(lines: &mut Lines<T>, feature: &str, qualifier: &str) -> Result<()>
where T: AsyncBufRead + Unpin {
    while let Some(fragment_line) = lines.next_line().await? {
        if let Some(captures) = FRAGMENT_REGEX.captures(&fragment_line) {
            debug!("Found inner fragment: {:?}", &captures);
            if is_matching_end(captures, feature, qualifier) {
                break;
            }
        }
    }
    Ok(())
}

async fn find_and_include_fragment(feature: &str, qualifier: &str, tx: &Sender<String>, fragments: &Vec<Bolt>, invar_config: &InvarConfig) -> Result<()> {
    for bolt in fragments {
        if let Bolt::Fragment { bolt_core, qualifier: fragment_qualifier, .. } = bolt {
            let fragment_qualifier = fragment_qualifier.as_ref().map(ToOwned::to_owned).unwrap_or("".to_string());
            if bolt_core.feature_name == feature && fragment_qualifier == qualifier {
                debug!("Found fragment to include: {:?}", bolt);
                include_fragment(bolt, feature, qualifier, tx, invar_config).await?;
                break;
            }
        }
    }
    Ok(())
}

async fn include_fragment(fragment: &Bolt, feature: &str, qualifier: &str, tx: &Sender<String>, _invar_config: &InvarConfig) -> Result<()> {
    let source = fragment.source();
    let file = File::open(source.as_path()).await?;
    let buffered_reader = BufReader::new(file);
    let mut lines = buffered_reader.lines();
    while let Some(mut line) = lines.next_line().await? {
        if let Some(captures) = FRAGMENT_REGEX.captures(&line) {
            let placeholder_feature = captures.name("feature").map(|m|m.as_str().to_string()).unwrap_or("@".to_string());
            let placeholder_qualifier = captures.name("qualifier").map(|m|m.as_str().to_string()).unwrap_or("".to_string());
            debug!("Found placeholder: {:?}: {:?}", &feature, &qualifier);
            if let Some(bracket) = captures.name("bracket") {
                if bracket.as_str() == "BEGIN " && placeholder_feature == feature && placeholder_qualifier == qualifier {
                    line.push('\n');
                    tx.send(line).await?;
                    copy_to_end_of_fragment(&mut lines, &feature, &qualifier, tx).await?;
                }
            }
            return Ok(());
        }
    }
    debug!("Include fragment: placeholder not found");
    Ok(())
}

async fn copy_to_end_of_fragment<T>(lines: &mut Lines<T>, feature: &str, qualifier: &str, tx: &Sender<String>) -> Result<()>
    where T: AsyncBufRead + Unpin {
    while let Some(mut fragment_line) = lines.next_line().await? {
        fragment_line.push('\n');
        tx.send(fragment_line.clone()).await?;
        if let Some(captures) = FRAGMENT_REGEX.captures(&fragment_line) {
            debug!("Found inner fragment: {:?}", &captures);
            if is_matching_end(captures, feature, qualifier) {
                break;
            }
        }
    }
    Ok(())
}

fn is_matching_end(captures: Captures, feature: &str, qualifier: &str) -> bool {
    if let Some(inner_bracket) = captures.name("bracket") {
        if inner_bracket.as_str() == "END " {
            let inner_feature = captures.name("feature").map(|m|m.as_str().to_string()).unwrap_or("@".to_string());
            if inner_feature != feature {
                return false;
            }
            let inner_qualifier = captures.name("qualifier").map(|m|m.as_str().to_string()).unwrap_or("".to_string());
            if inner_qualifier != qualifier {
                return false;
            }
            debug!("Found end of fragment: {:?}", &captures);
            return true;
        }
    }
    return false;
}

async fn open_target(target_file: AbsolutePath, invar_config: &InvarConfig) -> Result<Option<File>> {
    let mut open_options = OpenOptions::new().read(false).write(true).to_owned();
    let open_options = match invar_config.write_mode() {
        WriteMode::Ignore => {
            return Ok(None)
        },
        WriteMode::WriteNew => open_options.create_new(true),
        WriteMode::Overwrite => open_options.create(true).truncate(true),
    };

    let mut target_dir = target_file.to_path_buf();
    target_dir.pop();
    let mut dir_builder = DirBuilder::new();
    dir_builder.recursive(true);
    dir_builder.create(target_dir.as_path()).await?;

    let result = open_options.open(target_file.as_path()).await;
    match result {
        Ok(file) => Ok(Some(file)),
        Err(error) => {
            if let ErrorKind::AlreadyExists = error.kind() {
                Ok(None)
            } else {
                Err(error.into())
            }
        }
    }
}

async fn file_writer(rx: Receiver<String>, mut target: File) -> Result<()> {
    let mut rx = rx;
    while let Some(line) = rx.recv().await {
        target.write_all(line.as_bytes()).await?;
    }
    Ok(())
}

async fn update_invar_config<'a>(invar_config: &'a InvarConfig, bolts: &Vec<Bolt>) -> Result<Cow<'a,InvarConfig>> {
    let mut use_config = Cow::Borrowed(invar_config);
    for bolt in bolts {
        if let Bolt::Config(_) = bolt {
            let bolt_invar_config = get_invar_config(bolt.source()).await?;
            debug!("Apply bolt configuration: {:?}: {:?} += {:?}", bolt.target_name(), invar_config, &bolt_invar_config);
            let new_use_config = use_config.to_owned().with_invar_config(&bolt_invar_config).into_owned();
            use_config = Cow::Owned(new_use_config);
        }
    }
    Ok(use_config)
}

async fn get_invar_config(source: &AbsolutePath) -> Result<InvarConfig> {
    info!("Config path: {source:?}");

    let file = std::fs::File::open(source.as_path())?;
    let config = serde_yaml::from_reader(file)?;
    debug!("Invar configuration: {config:?}");
    Ok(config)
}

fn combine_and_filter_bolt_lists(cumulus_bolts_list: &Vec<Bolt>, invar_bolts_list: &Vec<Bolt>, thunder_config: &ThunderConfig) -> (Option<Bolt>, Vec<Bolt>) {
    let combined = combine_bolt_lists(cumulus_bolts_list, invar_bolts_list);
    filter_options(&combined, thunder_config)
}

fn filter_options(bolt_list: &Vec<Bolt>, thunder_config: &ThunderConfig) -> (Option<Bolt>, Vec<Bolt>) {
    let mut features = AHashSet::new();
    features.insert("@");
    for feature in thunder_config.use_thundercloud().features() {
        features.insert(feature);
    }
    let mut options = Vec::new();
    let mut fragments = Vec::new();
    for bolt in bolt_list {
        if features.contains(&bolt.feature_name() as &str) {
            if let Bolt::Option(_) = bolt {
                options.push(bolt.clone());
            } else if let Bolt::Fragment { .. } = bolt {
                fragments.push(bolt.clone())
            }
        }
    }
    let first_option = if options.is_empty() {
        None
    } else {
        Some(options.remove(0))
    };
    (first_option, fragments)
}

async fn visit_subdirectories(directory: &RelativePath, cumulus_subdirectories: AHashSet<SingleComponent>, invar_subdirectories: AHashSet<SingleComponent>, thunder_config: &ThunderConfig, invar_config: &InvarConfig) -> Result<()> {
    let mut invar_subdirectories = invar_subdirectories;
    for path in cumulus_subdirectories {
        let subdirectory_thumbs = if let Some(_) = invar_subdirectories.get(&path) {
            invar_subdirectories.remove(&path);
            FromBothCumulusAndInvar
        } else {
            FromCumulus
        };
        let mut subdirectory = directory.clone();
        let path: RelativePath = path.try_into()?;
        subdirectory.push(path);
        Box::pin(visit_subtree(&subdirectory, subdirectory_thumbs, thunder_config, invar_config)).await?;
    }
    for path in invar_subdirectories {
        let mut subdirectory = directory.clone();
        let path: RelativePath = path.try_into()?;
        subdirectory.push(path);
        Box::pin(visit_subtree(&subdirectory, FromInvar, thunder_config, invar_config)).await?;
    }
    Ok(())
}

fn combine(cumulus_bolts: AHashMap<String,Vec<Bolt>>, invar_bolts: AHashMap<String,Vec<Bolt>>) -> AHashMap<String,(Vec<Bolt>,Vec<Bolt>)> {
    let cumulus_keys: AHashSet<String> = cumulus_bolts.iter().map(|(k,_)| k).map(ToOwned::to_owned).collect();
    let invar_keys: AHashSet<String> = invar_bolts.iter().map(|(k,_)| k).map(ToOwned::to_owned).collect();
    let keys = cumulus_keys.union(&invar_keys);
    keys.map(
        |k: &String|
            (k.to_owned(),
                (
                    cumulus_bolts.get(k).map(ToOwned::to_owned).unwrap_or_else(Vec::new),
                    invar_bolts.get(k).map(ToOwned::to_owned).unwrap_or_else(Vec::new)
                )
            )
    ).collect()
}

fn combine_bolt_lists(cumulus_bolts_list: &Vec<Bolt>, invar_bolts_list: &Vec<Bolt>) -> Vec<Bolt> {
    let mut result = invar_bolts_list.clone();
    let mut invar_fragments = AHashSet::new();
    for invar_bolt in invar_bolts_list {
        if let Bolt::Fragment { .. } = invar_bolt {
            invar_fragments.insert((invar_bolt.feature_name(), invar_bolt.qualifier()));
        }
    }
    for cumulus_bolt in cumulus_bolts_list {
        if let Bolt::Fragment { .. } = cumulus_bolt {
            if invar_fragments.contains(&(cumulus_bolt.feature_name(), cumulus_bolt.qualifier())) {
                continue;
            }
        }
        result.push(cumulus_bolt.clone());
    }
    result
}

async fn try_visit_directory(exists: bool, get_root: impl FnOnce(&ThunderConfig) -> &AbsolutePath, thunder_config: &ThunderConfig, directory: &RelativePath) -> Result<(AHashMap<String,Vec<Bolt>>, AHashSet<SingleComponent>)> {
    if exists {
        let source_root = get_root(thunder_config);
        let in_cumulus = directory.clone().relative_to(source_root);
        visit_directory(&in_cumulus, thunder_config).await
    } else {
        Ok(void_subtree())
    }
}

fn void_subtree() -> (AHashMap<String, Vec<Bolt>>, AHashSet<SingleComponent>) {
    (AHashMap::new(), AHashSet::new())
}

async fn visit_directory(directory: &AbsolutePath, thunder_config: &ThunderConfig) -> Result<(AHashMap<String,Vec<Bolt>>, AHashSet<SingleComponent>)> {
    trace!("Visit directory: {:?} ⇒ {:?} [{:?}]", &directory, thunder_config.project_root(), thunder_config.invar());
    let mut bolts = AHashMap::new();
    let mut subdirectories = AHashSet::new();
    let mut entries = tokio::fs::read_dir(directory as &Path).await
        .map_err(|e| anyhow!(format!("error reading {:?}: {:?}", &directory, e)))?;
    while let Some(entry) = entries.next_entry().await? {
        trace!("Visit entry: {entry:?}");
        if entry.file_type().await?.is_dir() {
            if let Some(component) = entry.path().components().last() {
                let component = SingleComponent::try_new(Path::new(component.as_os_str()))?;
                subdirectories.insert(component);
            }
        } else {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let source = RelativePath::from(file_name.as_str()).relative_to(directory);
            let bolt;
            if let Some(captures) = BOLT_REGEX_WITH_DOT.captures(&file_name) {
                debug!("Bolt regex with dot: {:?}", &file_name);
                bolt = captures_to_bolt(captures, source)?;
            } else if let Some(captures) = BOLT_REGEX_WITHOUT_DOT.captures(&file_name) {
                debug!("Bolt regex without dot: {:?}", &file_name);
                bolt = captures_to_bolt(captures, source)?;
            } else if let Some(captures) = PLAIN_FILE_REGEX_WITH_DOT.captures(&file_name) {
                debug!("Plain file regex with dot: {:?}", &file_name);
                let (base_name, extension) =
                    if let (Some(b), Some(e)) = (captures.name("base"), captures.name("extension")) {
                        (b.as_str(), e.as_str())
                    } else {
                        (&*file_name, "")
                    };
                bolt = Bolt::Option(BoltCore {
                    base_name: base_name.to_string(),
                    extension: extension.to_string(),
                    feature_name: "@".to_string(),
                    source,
                })
            } else {
                debug!("Unrecognized file name: {:?}", &file_name);
                bolt = Bolt::Option(BoltCore {
                    base_name: file_name.to_string(),
                    extension: "".to_string(),
                    feature_name: "@".to_string(),
                    source,
                })
            }
            debug!("Bolt: {bolt:?}");
            add(&mut bolts, &bolt.target_name(), bolt);
        }
    }
    for (target_name, bolts) in &bolts {
        let mut qualifiers = Vec::new();
        for bolt in bolts {
            let qualifier = match bolt {
                Bolt::Fragment { qualifier, .. } => qualifier,
                Bolt::Unknown { qualifier, .. } => qualifier,
                _ => &None
            };
            if let Some(qualifier) = qualifier {
                qualifiers.push(qualifier.to_owned());
            }
        }
        debug!("Found bolts: {:?}: {:?}: {:?}: {:?}", &directory, target_name, bolts, qualifiers);
    }
    Ok((bolts, subdirectories))
}

fn captures_to_bolt(captures: Captures, source: AbsolutePath) -> Result<Bolt> {
    let extension = captures.name("extension").map(|m|m.as_str().to_string()).unwrap_or("".to_string());
    let feature_name = captures.name("feature").map(|m|m.as_str().to_string()).unwrap_or("@".to_string());
    let qualifier = captures.name("qualifier").map(|m|m.as_str().to_string());
    if let (Some(base_name_orig), Some(bolt_type)) = (captures.name("base"), captures.name("bolt_type")) {
        let base_name = base_name_orig.as_str().to_string();
        let base_name = base_name.strip_prefix("dot_")
            .map(|stripped| ".".to_string() + stripped)
            .unwrap_or(base_name);
        let base_name = base_name.strip_prefix("x_").unwrap_or(&base_name).to_string();
        let bolt_type = bolt_type.as_str();
        let bolt =
            if bolt_type == "option" {
                Bolt::Option(BoltCore{base_name,extension,feature_name,source})
            } else if bolt_type == "config" {
                create_config(base_name_orig.as_str(), &base_name, &extension, &feature_name, source)
            } else if bolt_type == "fragment" {
                Bolt::Fragment { bolt_core: BoltCore { base_name, extension, feature_name, source }, qualifier }
            } else {
                Bolt::Unknown { bolt_core: BoltCore{base_name, extension, feature_name, source }, qualifier }
            };
        Ok(bolt)
    } else {
        bail!("Internal error")
    }
}

fn create_config(base_name_orig: &str, base_name: &str, extension: &str, feature_name: &str, source: AbsolutePath) -> Bolt {
    if base_name_orig == "dot" && extension == ".yaml" {
        Bolt::Config(BoltCore{
            base_name: ".".to_string(),
            extension: "".to_string(),
            feature_name: feature_name.to_string(),
            source,
        })
    } else {
        Bolt::Config(BoltCore{
            base_name: base_name.to_string(),
            extension: extension.to_string(),
            feature_name: feature_name.to_string(),
            source,
        })
    }
}

fn add<K,I>(map: &mut AHashMap<K,Vec<I>>, key: &K, item: I)
where
    K: PartialEq + Eq + Hash + Clone
{
    if let Some(existing_list) = map.get_mut(key) {
        existing_list.push(item);
    } else {
        let mut new_list = Vec::new();
        new_list.push(item);
        map.insert(key.clone(), new_list);
    }
}