use std::hash::Hash;
use std::ops::Add;
use anyhow::{bail, Result};
use std::path::Path;
use ahash::AHashMap;
use log::{debug, info, trace};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use serde::Deserialize;

#[derive(Deserialize,Debug)]
struct ThundercloudConfig {
    niche: NicheConfig,
}

#[derive(Deserialize,Debug)]
struct NicheConfig {
    name: String,
    description: Option<String>,
}

pub async fn process_niche(thundercloud_directory: impl AsRef<Path>, niche_directory: impl AsRef<Path>) -> Result<()> {
    info!("Apply: {:?} -> {:?}", thundercloud_directory.as_ref(), niche_directory.as_ref());
    let config = get_config(thundercloud_directory.as_ref())?;
    info!("Thundercloud: {:?}: {:?}", config.niche.name, config.niche.description.unwrap_or("-".to_string()));
    let mut cumulus = thundercloud_directory.as_ref().to_owned();
    cumulus.push("cumulus");
    visit(cumulus).await?;
    Ok(())
}

fn get_config(thundercloud_directory: impl AsRef<Path>) -> Result<ThundercloudConfig> {
    let mut config_path = thundercloud_directory.as_ref().to_owned();
    config_path.push("thundercloud.yaml");
    info!("Config path: {config_path:?}");

    let file = std::fs::File::open(config_path)?;
    let config = serde_yaml::from_reader(file)?;
    debug!("Thundercloud configuration: {config:?}");
    Ok(config)
}

#[derive(Debug)]
struct BoltCore {
    base_name: String,
    extension: String,
    feature_name: String,
}

#[derive(Debug)]
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

async fn visit(directory: impl AsRef<Path>) -> Result<()> {
    trace!("Visit directory: {:?}", directory.as_ref());
    let mut bolts = AHashMap::new();
    let mut entries = tokio::fs::read_dir(directory.as_ref()).await?;
    while let Some(entry) = entries.next_entry().await? {
        trace!("Visit entry: {entry:?}");
        let mut entry_path = directory.as_ref().to_owned();
        entry_path.push(entry.file_name());
        if entry.file_type().await?.is_dir() {
            Box::pin(visit(entry.path())).await?;
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
                bolt = Bolt::Option(BoltCore { base_name: base_name.to_string(), extension: extension.to_string(), feature_name: "*".to_string() })
            } else {
                bolt = Bolt::Option(BoltCore { base_name: file_name.to_string(), extension: "".to_string(), feature_name: "*".to_string() })
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
        debug!("Found bolts: {:?}: {:?}: {:?}: {:?}", directory.as_ref(), target_name, bolts, qualifiers);
    }
    Ok(())
}

fn captures_to_bolt(captures: Captures) -> Result<Bolt> {
    let extension = captures.name("extension").map(|m|m.as_str().to_string()).unwrap_or("".to_string());
    let feature_name = captures.name("feature").map(|m|m.as_str().to_string()).unwrap_or("*".to_string());
    let qualifier = captures.name("qualifier").map(|m|m.as_str().to_string());
    if let (Some(base_name), Some(bolt_type)) = (captures.name("base"), captures.name("bolt_type")) {
        let bolt_type = bolt_type.as_str();
        let bolt =
            if bolt_type == "option" {
                Bolt::Option(BoltCore{base_name: base_name.as_str().to_string(),extension,feature_name})
            } else if bolt_type == "example" {
                Bolt::Example(BoltCore{base_name: base_name.as_str().to_string(),extension,feature_name})
            } else if bolt_type == "overwrite" {
                Bolt::Overwrite(BoltCore{base_name: base_name.as_str().to_string(),extension,feature_name})
            } else if bolt_type == "fragment" {
                Bolt::Fragment { bolt_core: BoltCore { base_name: base_name.as_str().to_string(), extension, feature_name }, qualifier }
            } else if bolt_type == "ignore" {
                Bolt::Ignore(BoltCore{base_name: base_name.as_str().to_string(),extension,feature_name})
            } else {
                Bolt::Unknown { bolt_core: BoltCore{base_name: base_name.as_str().to_string(), extension, feature_name }, qualifier }
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