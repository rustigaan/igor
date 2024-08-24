use ahash::{AHashMap, AHashSet};
use anyhow::{anyhow, bail, Result};
use std::borrow::Cow;
use std::hash::Hash;
use std::ops::Add;
use std::path::Path;
use std::pin::pin;
use log::{debug, info, trace, warn};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use serde_yaml::Value;
use tokio_stream::StreamExt;
use crate::config_model::{invar_config, InvarConfig, NicheDescription, thundercloud_config, ThundercloudConfig, ThunderConfig, WriteMode};
use crate::path::{AbsolutePath, RelativePath, SingleComponent};
use crate::thundercloud::Thumbs::{FromBothCumulusAndInvar, FromCumulus, FromInvar};
use crate::config_model::UseThundercloudConfig;
use crate::file_system::{DirEntry, FileSystem, SourceFile, TargetFile};

pub async fn process_niche<T: ThunderConfig>(thunder_config: T) -> Result<()> {
    let thundercloud_directory = thunder_config.thundercloud_directory();
    let cumulus = thunder_config.cumulus();
    let invar = thunder_config.invar();
    let project_root = thunder_config.project_root();
    info!("Apply: {:?} ⊕ {:?} ⇒ {:?}", cumulus, invar, project_root);
    let config = get_config(thundercloud_directory)?;
    let niche = config.niche();
    info!("Thundercloud: {:?}: {:?}", niche.name(), niche.description().unwrap_or(&"-".to_string()));
    debug!("Use thundercloud: {:?}", thunder_config.use_thundercloud());
    let current_directory = RelativePath::from(".");
    let invar_config = config.invar_defaults();
    let invar_defaults = thunder_config.use_thundercloud().invar_defaults().into_owned();
    let invar_config = invar_config.with_invar_config(invar_defaults);
    debug!("String properties: {:?}", invar_config.string_props());
    visit_subtree(&current_directory, FromBothCumulusAndInvar, &thunder_config, invar_config.as_ref()).await?;
    Ok(())
}

fn get_config(thundercloud_directory: &AbsolutePath) -> Result<impl ThundercloudConfig> {
    let mut config_path = thundercloud_directory.clone();
    config_path.push("thundercloud.yaml");
    info!("Config path: {config_path:?}");

    let file = std::fs::File::open(&*config_path)?;
    let config = thundercloud_config::from_reader(file)?;
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
            Bolt::Config(_) => "config",
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

static PLACE_HOLDER_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new("\\$\\{([^{}]*)}").unwrap()
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

trait DirectoryLocation {
    fn file_system(&self) -> &impl FileSystem<DirEntryItem=impl DirEntry>;
    fn directory<'a, T: ThunderConfig>(&self, thunder_config: &'a T) -> &'a AbsolutePath;
}

struct CumulusDirectoryLocation<FS: FileSystem>(FS);
struct InvarDirectoryLocation<FS: FileSystem>(FS);

impl<FS: FileSystem> DirectoryLocation for InvarDirectoryLocation<FS> {
    fn file_system(&self) -> &impl FileSystem<DirEntryItem=impl DirEntry> {
        &self.0
    }

    fn directory<'a, T: ThunderConfig>(&self, thunder_config: &'a T) -> &'a AbsolutePath {
        thunder_config.invar()
    }
}

impl<FS: FileSystem> DirectoryLocation for CumulusDirectoryLocation<FS> {
    fn file_system(&self) -> &impl FileSystem<DirEntryItem=impl DirEntry> {
        &self.0
    }

    fn directory<'a, T: ThunderConfig>(&self, thunder_config: &'a T) -> &'a AbsolutePath {
        thunder_config.cumulus()
    }
}

async fn visit_subtree<TC, IC>(directory: &RelativePath, thumbs: Thumbs, thunder_config: &TC, invar_config: &IC) -> Result<()>
where
    TC: ThunderConfig,
    IC: InvarConfig
{
    let cumulus_directory_location = CumulusDirectoryLocation(thunder_config.thundercloud_file_system().clone());
    let (cumulus_bolts, cumulus_subdirectories) =
        try_visit_directory(thumbs.visit_cumulus(), &cumulus_directory_location, thunder_config, directory).await?;
    let invar_directory_location = InvarDirectoryLocation(thunder_config.project_file_system().clone());
    let (invar_bolts, invar_subdirectories) =
        try_visit_directory(thumbs.visit_invar(), &invar_directory_location, thunder_config, directory).await?;

    let bolts = combine(cumulus_bolts, invar_bolts);
    for (key, bolt_lists) in &bolts {
        debug!("Bolts entry: {:?}: {:?}", key, bolt_lists);
    }

    generate_files(&directory, bolts, thunder_config, invar_config).await?;

    visit_subdirectories(directory, cumulus_subdirectories, invar_subdirectories, thunder_config, invar_config).await?;

    Ok(())
}

async fn generate_files<TC, IC>(directory: &RelativePath, bolts: AHashMap<String, (Vec<Bolt>, Vec<Bolt>)>, thunder_config: &TC, invar_config: &IC) -> Result<()>
where
    TC: ThunderConfig,
    IC: InvarConfig
{
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
        let file_system = thunder_config.project_file_system();
        generate_file(file_system, &target_file, option, bolts, whole_config.as_ref()).await?;
    }
    Ok(())
}

async fn generate_file<FS, IC>(file_system: &FS, target_path: &AbsolutePath, option: Option<Bolt>, bolts: Vec<Bolt>, invar_config: &IC) -> Result<()>
where
    FS: FileSystem,
    IC: InvarConfig
{
    if bolts.is_empty() {
        debug!("Skip: {:?}: {:?}", target_path, &bolts);
        return Ok(())
    }
    let option =
        if let Some(option) = option {
            option
        } else {
            debug!("Skip (only fragments): {:?}: {:?}", target_path, &bolts);
            return Ok(())
        }
    ;
    if invar_config.write_mode() == WriteMode::Ignore {
        debug!("Ignore: {:?}: {:?}: {:?}", target_path, &bolts, &invar_config);
        return Ok(())
    }
    if let Some(target_file) = file_system.open_target(target_path.clone(), invar_config.write_mode()).await? {
        generate_option(file_system, option, bolts, invar_config, &target_file).await?;
        let mut target_file_mut = target_file;
        target_file_mut.close().await?;
    } else {
        debug!("Skip (target exists): {:?}: {:?}: {:?}", target_path, &bolts, &invar_config);
    }
    Ok(())
}

async fn generate_option<FS, IC, TF>(file_system: &FS, option: Bolt, fragments: Vec<Bolt>, invar_config: &IC, target_file: &TF) -> Result<()>
where
    FS: FileSystem,
    IC: InvarConfig,
    TF: TargetFile
{
    debug!("Generating option: {:?}: {:?}: {:?}", &option, &fragments, invar_config);
    let source = option.source();
    let mut source_file = file_system.open_source(source.clone()).await?;
    while let Some(line) = source_file.next_line().await? {
        if let Some(captures) = FRAGMENT_REGEX.captures(&line) {
            let feature = captures.name("feature").map(|m|m.as_str().to_string()).unwrap_or("@".to_string());
            let qualifier = captures.name("qualifier").map(|m|m.as_str().to_string()).unwrap_or("".to_string());
            debug!("Found fragment: {:?}: {:?}", &feature, &qualifier);
            if let Some(bracket) = captures.name("bracket") {
                if bracket.as_str() == "BEGIN " {
                    skip_to_end_of_fragment(&mut source_file, &feature, &qualifier).await?;
                }
            }
            find_and_include_fragment(file_system, &feature, &qualifier, target_file, &fragments, invar_config).await?;
            continue;
        }
        send_to_writer(&line, invar_config, target_file).await?;
    }
    Ok(())
}

async fn skip_to_end_of_fragment<SF>(lines: &mut SF, feature: &str, qualifier: &str) -> Result<()>
where
    SF: SourceFile
{
    while let Some(fragment_line) = lines.next_line().await? {
        if let Some(captures) = FRAGMENT_REGEX.captures(&fragment_line) {
            debug!("Found inner fragment: {:?}", &captures);
            if is_matching_end(&captures, feature, qualifier) {
                break;
            }
        }
    }
    Ok(())
}

async fn find_and_include_fragment<FS, IC, TF>(file_system: &FS, feature: &str, qualifier: &str, target_file: &TF, fragments: &Vec<Bolt>, invar_config: &IC) -> Result<()>
where
    FS: FileSystem,
    IC: InvarConfig,
    TF: TargetFile
{
    for bolt in fragments {
        if let Bolt::Fragment { bolt_core, qualifier: fragment_qualifier, .. } = bolt {
            let fragment_qualifier = fragment_qualifier.as_ref().map(ToOwned::to_owned).unwrap_or("".to_string());
            if bolt_core.feature_name == feature && fragment_qualifier == qualifier {
                debug!("Found fragment to include: {:?}", bolt);
                include_fragment(file_system, bolt, feature, qualifier, target_file, fragments, invar_config).await?;
                break;
            }
        }
    }
    Ok(())
}

async fn include_fragment<FS, TF, IC>(file_system: &FS, fragment: &Bolt, feature: &str, qualifier: &str, target_file: &TF, fragments: &Vec<Bolt>, invar_config: &IC) -> Result<()>
where
    FS: FileSystem,
    TF: TargetFile,
    IC: InvarConfig
{
    let source = fragment.source().clone();
    let mut source_file = file_system.open_source(source).await?;
    while let Some(line) = source_file.next_line().await? {
        if let Some(captures) = FRAGMENT_REGEX.captures(&line) {
            let placeholder_feature = captures.name("feature").map(|m|m.as_str().to_string()).unwrap_or("@".to_string());
            let placeholder_qualifier = captures.name("qualifier").map(|m|m.as_str().to_string()).unwrap_or("".to_string());
            debug!("Found placeholder: {:?}: {:?}", &feature, &qualifier);
            if let Some(bracket) = captures.name("bracket") {
                if bracket.as_str() == "BEGIN " && placeholder_feature == feature && placeholder_qualifier == qualifier {
                    send_to_writer(&line, invar_config, target_file).await?;
                    copy_to_end_of_fragment(file_system, &mut source_file, &feature, &qualifier, target_file, fragments, invar_config).await?;
                }
            }
            return Ok(());
        }
    }
    debug!("Include fragment: placeholder not found");
    Ok(())
}

async fn copy_to_end_of_fragment<FS, SF, TF, IC>(file_system: &FS, lines: &mut SF, feature: &str, qualifier: &str, target_file: &TF, fragments: &Vec<Bolt>, invar_config: &IC) -> Result<()>
where
    FS: FileSystem,
    SF: SourceFile,
    TF: TargetFile,
    IC: InvarConfig
{
    while let Some(fragment_line) = lines.next_line().await? {
        if let Some(captures) = FRAGMENT_REGEX.captures(&fragment_line) {
            debug!("Found inner fragment: {:?}", &captures);
            if is_matching_end(&captures, feature, qualifier) {
                send_to_writer(&fragment_line, invar_config, target_file).await?;
                break;
            } else {
                if let Some(bracket) = captures.name("bracket") {
                    if bracket.as_str() == "BEGIN " {
                        skip_to_end_of_fragment(lines, &feature, &qualifier).await?;
                    }
                }
                Box::pin(find_and_include_fragment(file_system, &feature, &qualifier, target_file, fragments, invar_config)).await?;
                continue;
            }
        }
        send_to_writer(&fragment_line, invar_config, target_file).await?;
    }
    Ok(())
}

fn is_matching_end(captures: &Captures, feature: &str, qualifier: &str) -> bool {
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

    false
}

async fn send_to_writer<IC, TF>(line: &str, invar_config: &IC, target_file: &TF) -> Result<()>
where
    IC: InvarConfig,
    TF: TargetFile
{
    let mut line = interpolate(&line, invar_config);
    debug!("Send to writer: {:?}", line);
    line.push('\n');
    target_file.write_line(line).await?;
    Ok(())
}

fn interpolate<IC: InvarConfig>(line: &str, invar_config: &IC) -> String {
    let mut result = String::new();
    let properties = invar_config.props();
    let replacements = properties.as_ref();
    let mut pos = 0;
    for placeholder in PLACE_HOLDER_REGEX.captures_iter(line) {
        if let (Some(extent), Some(expression)) = (placeholder.get(0),placeholder.get(1)) {
            let expression = expression.as_str().to_string();
            if let Some(Value::String(replacement)) = replacements.get(&expression) {
                debug!("Found: [{:?}] -> [{:?}]", &expression, &replacement);
                result.push_str(&line[pos..extent.start()]);
                result.push_str(replacement);
            } else {
                debug!("Not found: [{:?}]", &expression);
                result.push_str(&line[pos..extent.end()])
            }
            pos = extent.end();
        } else {
            debug!("No expression: [{:?}]", &line[pos..]);
            result.push_str(&line[pos..]);
            continue;
        }
    }
    result.push_str(&line[pos..line.len()]);
    result
}

async fn update_invar_config<'a, IC>(invar_config: &'a IC, bolts: &Vec<Bolt>) -> Result<Cow<'a, IC>>
where
    IC: InvarConfig
{
    let mut use_config = Cow::Borrowed(invar_config);
    for bolt in bolts {
        debug!("Bolt kind: {:?}", bolt.kind_name());
        if let Bolt::Config(_) = bolt {
            let bolt_invar_config = get_invar_config(bolt.source()).await?;
            debug!("Apply bolt configuration: {:?}: {:?} += {:?}", bolt.target_name(), invar_config, &bolt_invar_config);
            let new_use_config = use_config.to_owned().with_invar_config(bolt_invar_config).into_owned();
            use_config = Cow::Owned(new_use_config);
        }
    }
    debug!("Updated invar config: {:?}", use_config);
    Ok(use_config)
}

async fn get_invar_config(source: &AbsolutePath) -> Result<impl InvarConfig> {
    info!("Config path: {source:?}");

    let file = std::fs::File::open(source.as_path())?;
    let config = invar_config::from_reader(file)?;
    debug!("Invar configuration: {config:?}");
    Ok(config)
}

fn combine_and_filter_bolt_lists<TC>(cumulus_bolts_list: &Vec<Bolt>, invar_bolts_list: &Vec<Bolt>, thunder_config: &TC) -> (Option<Bolt>, Vec<Bolt>)
where
    TC: ThunderConfig
{
    let combined = combine_bolt_lists(cumulus_bolts_list, invar_bolts_list);
    filter_options(&combined, thunder_config)
}

fn filter_options<TC>(bolt_list: &Vec<Bolt>, thunder_config: &TC) -> (Option<Bolt>, Vec<Bolt>)
where
    TC: ThunderConfig
{
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

async fn visit_subdirectories<TC, IC>(directory: &RelativePath, cumulus_subdirectories: AHashSet<SingleComponent>, invar_subdirectories: AHashSet<SingleComponent>, thunder_config: &TC, invar_config: &IC) -> Result<()>
where
    TC: ThunderConfig,
    IC: InvarConfig
{
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

async fn try_visit_directory<DL, TC>(exists: bool, directory_location: &DL, thunder_config: &TC, directory: &RelativePath) -> Result<(AHashMap<String,Vec<Bolt>>, AHashSet<SingleComponent>)>
where
    DL: DirectoryLocation,
    TC: ThunderConfig
{
    if exists {
        let source_root = directory_location.directory(thunder_config);
        let in_cumulus = directory.clone().relative_to(source_root);
        visit_directory(directory_location, &in_cumulus, thunder_config).await
    } else {
        Ok(void_subtree())
    }
}

fn void_subtree() -> (AHashMap<String, Vec<Bolt>>, AHashSet<SingleComponent>) {
    (AHashMap::new(), AHashSet::new())
}

async fn visit_directory<DL, TC>(directory_location: &DL, directory: &AbsolutePath, thunder_config: &TC) -> Result<(AHashMap<String,Vec<Bolt>>, AHashSet<SingleComponent>)>
where
    DL: DirectoryLocation,
    TC: ThunderConfig
{
    trace!("Visit directory: {:?} ⇒ {:?} [{:?}]", &directory, thunder_config.project_root(), thunder_config.invar());
    let mut bolts = AHashMap::new();
    let mut subdirectories = AHashSet::new();
    let file_system = directory_location.file_system();
    let entries = file_system.read_dir(directory).await
        .map_err(|e| anyhow!(format!("error reading {:?}: {:?}", &directory, e)))?;
    let mut entries = pin!(entries);
    while let Some(entry) = entries.next().await {
        let entry = entry?;
        trace!("Visit entry: {entry:?}");
        if entry.is_dir().await? {
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