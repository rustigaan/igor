use anyhow::Result;
use std::borrow::Cow;
use std::fmt::Debug;
use std::io::Read;
use ahash::AHashMap;
use log::debug;
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_yaml::{Mapping, Value};
use crate::path::AbsolutePath;

#[derive(Deserialize,Debug,Clone)]
pub enum OnIncoming {
    Update,
    Ignore,
    Warn,
    Fail
}

pub trait NicheConfig : Sized + Debug {
    fn from_reader<R>(reader: R) -> Result<Self> where R: Read;
    fn use_thundercloud(&self) -> &UseThundercloudConfig;
}

pub mod niche_config {
    use super::*;

    pub fn from_reader<R>(reader: R) -> Result<impl NicheConfig> where R: Read {
        let config: NicheConfigData = NicheConfig::from_reader(reader)?;
        Ok(config)
    }

    #[derive(Deserialize,Debug)]
    struct NicheConfigData {
        #[serde(rename = "use-thundercloud")]
        use_thundercloud: UseThundercloudConfig,
    }

    impl NicheConfig for NicheConfigData {
        fn from_reader<R>(reader: R) -> Result<Self> where R: Read {
            let config: NicheConfigData = serde_yaml::from_reader(reader)?;
            Ok(config)
        }

        fn use_thundercloud(&self) -> &UseThundercloudConfig {
            &self.use_thundercloud
        }
    }
}

#[derive(Debug)]
pub struct ThunderConfig {
    use_thundercloud: UseThundercloudConfig,
    thundercloud_directory: AbsolutePath,
    cumulus: AbsolutePath,
    invar: AbsolutePath,
    project: AbsolutePath,
}

impl ThunderConfig {
    pub fn new(use_thundercloud: UseThundercloudConfig, thundercloud_directory: AbsolutePath, invar: AbsolutePath, project: AbsolutePath) -> Self {
        let mut cumulus = thundercloud_directory.clone();
        cumulus.push("cumulus");
        ThunderConfig {
            use_thundercloud,
            thundercloud_directory,
            cumulus,
            invar,
            project,
        }
    }

    pub fn use_thundercloud(&self) -> &UseThundercloudConfig {
        &self.use_thundercloud
    }

    pub fn thundercloud_directory(&self) -> &AbsolutePath {
        &self.thundercloud_directory
    }

    pub fn cumulus(&self) -> &AbsolutePath {
        &self.cumulus
    }

    pub fn invar(&self) -> &AbsolutePath {
        &self.invar
    }

    pub fn project_root(&self) -> &AbsolutePath {
        &self.project
    }
}

#[allow(dead_code)]
#[derive(Deserialize,Debug,Clone)]
pub struct UseThundercloudConfig {
    directory: Option<String>,
    #[serde(rename = "git-remote")]
    git_remote: Option<GitRemoteConfig>,
    #[serde(rename = "on-incoming")]
    on_incoming: Option<OnIncoming>,
    features: Option<Vec<String>>,
    #[serde(rename = "invar-defaults")]
    invar_defaults: Option<InvarConfig>,
}

static UPDATE: Lazy<OnIncoming> = Lazy::new(|| OnIncoming::Update);
static EMPTY_VEC: Lazy<Vec<String>> = Lazy::new(Vec::new);

impl UseThundercloudConfig {
    pub fn directory(&self) -> Option<&String> {
        self.directory.as_ref()
    }
    pub fn on_incoming(&self) -> &OnIncoming {
        &self.on_incoming.as_ref().unwrap_or(&UPDATE)
    }
    pub fn features(&self) -> &[String] {
        &self.features.as_deref().unwrap_or(&EMPTY_VEC)
    }
    pub fn invar_defaults(&self) -> Cow<InvarConfig> {
        if let Some(invar_defaults) = &self.invar_defaults {
            Cow::Borrowed(invar_defaults)
        } else {
            Cow::Owned(InvarConfig::new())
        }
    }
}

#[allow(dead_code)]
#[derive(Deserialize,Debug,Clone)]
pub struct GitRemoteConfig {
    #[serde(rename = "fetch-url")]
    fetch_url: String,
    revision: String,
    #[serde(rename = "on-incoming")]
    on_incoming: Option<OnIncoming>,
}

#[derive(Deserialize,Debug,Clone,Copy,Eq, PartialEq)]
pub enum WriteMode {
    Overwrite,
    WriteNew,
    Ignore
}

#[derive(Deserialize,Debug,Clone)]
pub struct InvarConfig {
    #[serde(rename = "write-mode")]
    write_mode: Option<WriteMode>,
    interpolate: Option<bool>,
    props: Option<Mapping>,
}

#[allow(dead_code)]
impl InvarConfig {
    pub fn new() -> InvarConfig {
        InvarConfig { write_mode: None, interpolate: None, props: None }
    }

    pub fn with_invar_config(&self, invar_config: &InvarConfig) -> Cow<InvarConfig> {
        let dirty = false;
        let (write_mode, dirty) = merge_property(self.write_mode, invar_config.write_mode, dirty);
        debug!("Write mode: {:?} -> {:?} ({:?})", self.write_mode, &write_mode, dirty);
        let (interpolate, dirty) = merge_property(self.interpolate, invar_config.interpolate, dirty);
        debug!("Interpolate: {:?} -> {:?} ({:?})", self.interpolate, &interpolate, dirty);
        let (props, dirty) = merge_props(&self.props, &invar_config.props, dirty);
        if dirty {
            Cow::Owned(InvarConfig { write_mode, interpolate, props: Some(props.into_owned()) })
        } else {
            Cow::Borrowed(self)
        }
    }

    pub fn with_write_mode_option(&self, write_mode: Option<WriteMode>) -> Cow<InvarConfig> {
        let invar_config = InvarConfig { write_mode, interpolate: None, props: None };
        self.with_invar_config(&invar_config)
    }

    pub fn with_write_mode(&self, write_mode: WriteMode) -> Cow<InvarConfig> {
        self.with_write_mode_option(Some(write_mode))
    }

    pub fn write_mode(&self) -> WriteMode {
        self.write_mode.unwrap_or(WriteMode::Overwrite)
    }

    pub fn with_interpolate_option(&self, interpolate: Option<bool>) -> Cow<InvarConfig> {
        let invar_config = InvarConfig { write_mode: None, interpolate, props: None };
        self.with_invar_config(&invar_config)
    }

    pub fn with_interpolate(&self, interpolate: bool) -> Cow<InvarConfig> {
        self.with_interpolate_option(Some(interpolate))
    }

    pub fn interpolate(&self) -> bool {
        self.interpolate.unwrap_or(true)
    }

    pub fn with_props_option(&self, props: Option<Mapping>) -> Cow<InvarConfig> {
        let invar_config = InvarConfig { write_mode: None, interpolate: None, props };
        self.with_invar_config(&invar_config)
    }

    pub fn with_props(&self, props: Mapping) -> Cow<InvarConfig> {
        self.with_props_option(Some(props))
    }

    pub fn props(&self) -> Cow<Mapping> {
        self.props.as_ref().map(Cow::Borrowed).unwrap_or(Cow::Owned(Mapping::new()))
    }

    pub fn string_props(&self) -> AHashMap<String,String> {
        to_string_map(self.props().as_ref())
    }
}

fn merge_property<T: Copy + Eq>(current_value_option: Option<T>, new_value_option: Option<T>, dirty: bool) -> (Option<T>, bool) {
    match (current_value_option, new_value_option) {
        (Some(current_value), Some(new_value)) =>
            if new_value == current_value {
                (current_value_option, dirty)
            } else {
                (new_value_option, true)
            },
        (None, Some(_)) => (new_value_option, true),
        (_, _) => (current_value_option, dirty)
    }
}

fn merge_props<'a>(current_props_option: &'a Option<Mapping>, new_props_option: &'a Option<Mapping>, dirty: bool) -> (Cow<'a, Mapping>, bool) {
    if let Some(current_props) = current_props_option {
        if let Some(new_props) = new_props_option {
            for (k, v) in new_props {
                if current_props.get(k) != Some(v) {
                    let mut result = current_props.clone();
                    let new_props = new_props.clone();
                    result.extend(new_props);
                    return (Cow::Owned(result), true)
                }
            }
            (Cow::Borrowed(current_props), dirty)
        } else {
            (Cow::Borrowed(current_props), dirty)
        }
    } else if let Some(new_props) = new_props_option {
        (Cow::Borrowed(new_props), true)
    } else {
        (Cow::Owned(Mapping::new()), true)
    }
}

fn to_string_map(props: &Mapping) -> AHashMap<String,String> {
    props.iter().map(to_strings).filter(Option::is_some).map(Option::unwrap).collect()
}

fn to_strings(entry: (&Value, &Value)) -> Option<(String, String)> {
    if let (Value::String(key), Value::String(val)) = entry {
        Some((key.to_owned(), val.to_owned()))
    } else {
        None
    }
}
