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
use crate::config_model::ThunderConfig;
use crate::path::{AbsolutePath, RelativePath, SingleComponent};
use crate::thundercloud::Thumbs::{FromBothCumulusAndInvar, FromCumulus, FromInvar};

#[derive(Deserialize,Debug)]
struct ThundercloudConfig {
    niche: NicheConfig,
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
    visit_subtree(&current_directory, FromBothCumulusAndInvar, &thunder_config).await?;
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
}

#[derive(Debug, Clone)]
enum Bolt {
    Option(BoltCore),
    Example(BoltCore),
    Overwrite(BoltCore),
    Fragment {
        bolt_core: BoltCore,
        qualifier: Option<String>
    },
    Ignore(BoltCore),
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
    fn qualifier(&self) -> Option<String>;
}

impl BoltExt for Bolt {
    fn bolt_core(&self) -> &BoltCore {
        match self {
            Bolt::Option(bolt_core) => bolt_core,
            Bolt::Example(bolt_core) => bolt_core,
            Bolt::Overwrite(bolt_core) => bolt_core,
            Bolt::Fragment { bolt_core, .. } => bolt_core,
            Bolt::Ignore(bolt_core) => bolt_core,
            Bolt::Unknown { bolt_core, .. } => bolt_core,
        }
    }
    fn kind_name(&self) -> &'static str {
        match self {
            Bolt::Option(_) => "option",
            Bolt::Example(_) => "example",
            Bolt::Overwrite(_) => "overwrite",
            Bolt::Fragment { .. } => "fragment",
            Bolt::Ignore(_) => "ignore",
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
    fn qualifier(&self) -> Option<String> {
        match self {
            Bolt::Fragment { qualifier, .. } => qualifier.clone(),
            Bolt::Unknown { qualifier, .. } => qualifier.clone(),
            _ => None
        }
    }
}

static BOLT_REGEX_WITH_DOT: Lazy<Regex> = Lazy::new(|| {
    Regex::new("^(?<base>.*)[+](?<bolt_type>[a-z0-9_]+)(-(?<feature>[a-z0-9_]+)(-(?<qualifier>[a-z0-9_]+))?)?(?<extension>[.][^.]*)$").unwrap()
});
static BOLT_REGEX_WITHOUT_DOT: Lazy<Regex> = Lazy::new(|| {
    Regex::new("^(?<base>[^.]+)[+](?<bolt_type>[a-z0-9_]+)(-(?<feature>[a-z0-9_]+)(-(?<qualifier>[a-z0-9_]+))?)?$").unwrap()
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

async fn visit_subtree(directory: &RelativePath, thumbs: Thumbs, thunder_config: &ThunderConfig) -> Result<()> {
    let (cumulus_bolts, cumulus_subdirectories) =
        try_visit_directory(thumbs.visit_cumulus(), ThunderConfig::cumulus, thunder_config, directory).await?;
    let (invar_bolts, invar_subdirectories) =
        try_visit_directory(thumbs.visit_invar(), ThunderConfig::invar, thunder_config, directory).await?;

    let bolts = combine(cumulus_bolts, invar_bolts);
    for (key, bolt_list) in &bolts {
        debug!("Bolts entry: {:?}: {:?}", key, bolt_list);
    }

    generate_files(&directory, bolts, thunder_config).await?;

    visit_subdirectories(directory, cumulus_subdirectories, invar_subdirectories, thunder_config).await?;

    Ok(())
}

async fn generate_files(directory: &RelativePath, bolts: AHashMap<String, Vec<Bolt>>, thunder_config: &ThunderConfig) -> Result<()> {
    let target_directory = directory.relative_to(thunder_config.project_root());
    debug!("Generate files in {:?}", &target_directory);
    for (name, bolt_list) in &bolts {
        if ILLEGAL_FILE_REGEX.is_match(name) {
            warn!("Target filename is not legal: {name:?}");
            continue;
        }
        if let Some(first_bolt) = bolt_list.iter().next() {
            if let Bolt::Ignore(_) = first_bolt {
                continue;
            }
        } else {
            continue;
        }
        let target_file = RelativePath::from(name as &str).relative_to(&target_directory);
        let bolt_list = filter_options(bolt_list, thunder_config);
        if bolt_list.is_empty() {
            debug!("Skipped: {name:?}: {:?}", &target_file);
            continue;
        }
        debug!("Generate: {name:?}: {bolt_list:?}: {:?}", &target_file);
    }
    Ok(())
}

fn filter_options(bolt_list: &Vec<Bolt>, thunder_config: &ThunderConfig) -> Vec<Bolt> {
    let mut features = AHashSet::new();
    features.insert("@");
    for feature in thunder_config.use_thundercloud().features() {
        features.insert(feature);
    }
    let mut result = Vec::new();
    for bolt in bolt_list {
        if features.contains(&bolt.feature_name() as &str) {
            result.push(bolt.clone());
        }
    }
    result
}

async fn visit_subdirectories(directory: &RelativePath, cumulus_subdirectories: AHashSet<SingleComponent>, invar_subdirectories: AHashSet<SingleComponent>, thunder_config: &ThunderConfig) -> Result<()> {
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
        Box::pin(visit_subtree(&subdirectory, subdirectory_thumbs, thunder_config)).await?;
    }
    for path in invar_subdirectories {
        let mut subdirectory = directory.clone();
        let path: RelativePath = path.try_into()?;
        subdirectory.push(path);
        Box::pin(visit_subtree(&subdirectory, FromInvar, thunder_config)).await?;
    }
    Ok(())
}

fn combine(cumulus_bolts: AHashMap<String,Vec<Bolt>>, invar_bolts: AHashMap<String,Vec<Bolt>>) -> AHashMap<String,Vec<Bolt>> {
    let mut result = AHashMap::new();
    let mut cumulus_bolts = cumulus_bolts;
    let cumulus_bolts_ref = &mut cumulus_bolts;
    for (key, invar_bolt_list) in invar_bolts {
        let bolt_list = if let Some(cumulus_bolt_list) = cumulus_bolts_ref.remove(&key) {
            combine_bolt_lists(cumulus_bolt_list, invar_bolt_list)
        } else {
            invar_bolt_list
        };
        result.insert(key, bolt_list);
    }
    for (key, invar_bolt_list) in cumulus_bolts_ref {
        result.insert(key.clone(), invar_bolt_list.clone());
    }
    result
}

fn combine_bolt_lists(cumulus_bolts_list: Vec<Bolt>, invar_bolts_list: Vec<Bolt>) -> Vec<Bolt> {
    let mut result = invar_bolts_list;
    for bolt in &result {
        if let Bolt::Ignore(_) = bolt {
            let mut ignore_result = Vec::new();
            ignore_result.push(bolt.to_owned());
            return ignore_result;
        }
    }
    for cumulus_bolt in &cumulus_bolts_list {
        let mut add_bolt = Some(Cow::Borrowed(cumulus_bolt));
        for invar_bolt in &result {
            if cumulus_bolt.feature_name() != invar_bolt.feature_name() {
                continue;
            }
            if cumulus_bolt.qualifier() != invar_bolt.qualifier() {
                continue;
            }
            match (cumulus_bolt, invar_bolt) {
                (Bolt::Example(bolt_core), Bolt::Overwrite(_)) => {
                    add_bolt = Some(Cow::Owned(Bolt::Option(bolt_core.clone())))
                },
                (Bolt::Fragment { .. }, Bolt::Fragment { .. }) => {
                    add_bolt = None;
                }
                (_, Bolt::Ignore(_)) => {
                    let mut ignore_result = Vec::new();
                    ignore_result.push(invar_bolt.to_owned());
                    return ignore_result;
                }
                (_, _) => ()
            }
            if add_bolt.is_none() {
                break;
            }
        }
        if let Some(add_bolt) = add_bolt {
            match add_bolt {
                Cow::Owned(Bolt::Option(_)) => {
                    result = result.iter().filter(|item| if let Bolt::Overwrite(_) = item { false } else { true }).map(ToOwned::to_owned).collect();
                    result.insert(0, add_bolt.into_owned())
                },
                add_bolt => result.push(add_bolt.into_owned()),
            }
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
            let bolt;
            if let Some(captures) = BOLT_REGEX_WITH_DOT.captures(&file_name) {
                bolt = captures_to_bolt(captures)?;
            } else if let Some(captures) = BOLT_REGEX_WITHOUT_DOT.captures(&file_name) {
                bolt = captures_to_bolt(captures)?;
            } else if let Some(captures) = PLAIN_FILE_REGEX_WITH_DOT.captures(&file_name) {
                let (base_name, extension) =
                    if let (Some(b), Some(e)) = (captures.name("base"), captures.name("extension")) {
                        (b.as_str(), e.as_str())
                    } else {
                        (&*file_name, "")
                    };
                bolt = Bolt::Option(BoltCore { base_name: base_name.to_string(), extension: extension.to_string(), feature_name: "@".to_string() })
            } else {
                bolt = Bolt::Option(BoltCore { base_name: file_name.to_string(), extension: "".to_string(), feature_name: "@".to_string() })
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

fn captures_to_bolt(captures: Captures) -> Result<Bolt> {
    let extension = captures.name("extension").map(|m|m.as_str().to_string()).unwrap_or("".to_string());
    let feature_name = captures.name("feature").map(|m|m.as_str().to_string()).unwrap_or("@".to_string());
    let qualifier = captures.name("qualifier").map(|m|m.as_str().to_string());
    if let (Some(base_name), Some(bolt_type)) = (captures.name("base"), captures.name("bolt_type")) {
        let base_name = base_name.as_str().to_string();
        let base_name = base_name.strip_prefix("dot_")
            .map(|stripped| ".".to_string() + stripped)
            .unwrap_or(base_name);
        let base_name = base_name.strip_prefix("x_").unwrap_or(&base_name).to_string();
        let bolt_type = bolt_type.as_str();
        let bolt =
            if bolt_type == "option" {
                Bolt::Option(BoltCore{base_name,extension,feature_name})
            } else if bolt_type == "example" {
                Bolt::Example(BoltCore{base_name,extension,feature_name})
            } else if bolt_type == "overwrite" {
                Bolt::Overwrite(BoltCore{base_name,extension,feature_name})
            } else if bolt_type == "fragment" {
                Bolt::Fragment { bolt_core: BoltCore { base_name, extension, feature_name }, qualifier }
            } else if bolt_type == "ignore" {
                Bolt::Ignore(BoltCore{base_name,extension,feature_name})
            } else {
                Bolt::Unknown { bolt_core: BoltCore{base_name, extension, feature_name }, qualifier }
            };
        Ok(bolt)
    } else {
        bail!("Internal error")
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