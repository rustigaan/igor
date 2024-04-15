use std::borrow::Cow;
use std::hash::Hash;
use std::ops::Add;
use anyhow::{anyhow, bail, Result};
use std::path::Path;
use ahash::{AHashMap, AHashSet};
use log::{debug, info, trace, warn};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use serde::Deserialize;
use crate::config_model::{InvarConfig, ThunderConfig};
use crate::config_model::WriteMode::Ignore;
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

trait BoltExt {
    fn bolt_core(&self) -> &BoltCore;
    fn kind_name(&self) -> &'static str;
    fn base_name(&self) -> String;
    fn extension(&self) -> String;
    fn target_name(&self) -> String;
    fn feature_name(&self) -> String;
    fn source(&self) -> &AbsolutePath;
    fn qualifier(&self) -> Option<String>;
}

impl BoltExt for Bolt {
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
        let dir_bolt_list = combine_and_filter_bolt_lists(dir_bolts.0, dir_bolts.1, thunder_config);
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
        let cumulus_bolts = filter_options(&bolt_lists.0, thunder_config);
        let invar_bolts = filter_options(&bolt_lists.1, thunder_config);
        let bolts = combine_and_filter_bolt_lists(cumulus_bolts, invar_bolts, thunder_config);
        generate_file(&target_file, bolts, use_config.as_ref()).await?;
    }
    Ok(())
}

async fn generate_file(target_file: &AbsolutePath, bolts: Vec<Bolt>, invar_config: &InvarConfig) -> Result<()> {
    if bolts.is_empty() {
        debug!("Skip: {:?}: {:?}", target_file, &bolts);
        return Ok(())
    }
    let use_config = update_invar_config(invar_config, &bolts).await?;
    if use_config.write_mode() == Ignore {
        debug!("Ignore: {:?}: {:?}: {:?}", target_file, &bolts, &use_config);
        return Ok(())
    }
    debug!("Generate: {:?}: {:?}: {:?}", target_file, &bolts, &use_config);
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

fn combine_and_filter_bolt_lists(cumulus_bolts_list: Vec<Bolt>, invar_bolts_list: Vec<Bolt>, thunder_config: &ThunderConfig) -> Vec<Bolt> {
    let combined = combine_bolt_lists(cumulus_bolts_list, invar_bolts_list);
    filter_options(&combined, thunder_config)
}

fn filter_options(bolt_list: &Vec<Bolt>, thunder_config: &ThunderConfig) -> Vec<Bolt> {
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
    let mut bolts = fragments;
    if !options.is_empty() {
        let first_option = options.remove(0);
        bolts.insert(0, first_option);
    }
    bolts
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

fn combine_bolt_lists(cumulus_bolts_list: Vec<Bolt>, invar_bolts_list: Vec<Bolt>) -> Vec<Bolt> {
    let mut result = invar_bolts_list;
    for cumulus_bolt in &cumulus_bolts_list {
        let mut add_bolt = Some(Cow::Borrowed(cumulus_bolt));
        for invar_bolt in &result {
            if cumulus_bolt.feature_name() != invar_bolt.feature_name() {
                continue;
            }
            if cumulus_bolt.qualifier() != invar_bolt.qualifier() {
                continue;
            }
            if let (Bolt::Fragment { .. }, Bolt::Fragment { .. }) = (cumulus_bolt, invar_bolt) {
                    add_bolt = None;
            }
            if add_bolt.is_none() {
                break;
            }
        }
        if let Some(add_bolt) = add_bolt {
            result.push(add_bolt.into_owned());
        }
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