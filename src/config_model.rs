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

pub trait ThundercloudConfig : Debug + Sized {
    type InvarConfigImpl : InvarConfig;
    fn from_reader<R: Read>(reader: R) -> Result<Self>;
    fn niche(&self) -> &impl NicheDescription;
    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl>;
}

#[derive(Deserialize,Debug)]
struct ThundercloudConfigData {
    niche: NicheDescriptionData,
    #[serde(rename = "invar-defaults")]
    invar_defaults: Option<InvarConfigData>
}

pub mod thundercloud_config {
    use super::*;

    pub fn from_reader<R: Read>(reader: R) -> Result<impl ThundercloudConfig> {
        let config: ThundercloudConfigData = ThundercloudConfig::from_reader(reader)?;
        Ok(config)
    }

    impl ThundercloudConfig for ThundercloudConfigData {
        type InvarConfigImpl = InvarConfigData;

        fn from_reader<R: Read>(reader: R) -> Result<Self> {
            let config: ThundercloudConfigData = serde_yaml::from_reader(reader)?;
            Ok(config)
        }

        fn niche(&self) -> &impl NicheDescription {
            &self.niche
        }

        fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl> {
            let result: Cow<Self::InvarConfigImpl>;
            if let Some(invar_config) = &self.invar_defaults {
                result = Cow::Borrowed(invar_config)
            } else {
                result = Cow::Owned(InvarConfigData::new())
            }
            result
        }
    }
}

pub trait NicheDescription {
    fn name(&self) -> &str;
    fn description(&self) -> &Option<String>;
}

#[derive(Deserialize,Debug)]
struct NicheDescriptionData {
    name: String,
    description: Option<String>,
}

impl NicheDescription for NicheDescriptionData {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &Option<String> {
        &self.description
    }
}

pub trait NicheConfig : Sized + Debug {
    fn from_reader<R: Read>(reader: R) -> Result<Self>;
    fn use_thundercloud(&self) -> &impl UseThundercloudConfig;
    fn new_thunder_config(&self, thundercloud_directory: AbsolutePath, invar: AbsolutePath, project_root: AbsolutePath) -> impl ThunderConfig;
}

#[derive(Deserialize,Debug)]
struct NicheConfigData {
    #[serde(rename = "use-thundercloud")]
    use_thundercloud: UseThundercloudConfigData,
}

pub mod niche_config {
    use super::*;

    pub fn from_reader<R: Read>(reader: R) -> Result<impl NicheConfig> {
        let config: NicheConfigData = NicheConfig::from_reader(reader)?;
        Ok(config)
    }

    impl NicheConfig for NicheConfigData {
        fn from_reader<R: Read>(reader: R) -> Result<Self> {
            let config: NicheConfigData = serde_yaml::from_reader(reader)?;
            Ok(config)
        }

        fn use_thundercloud(&self) -> &impl UseThundercloudConfig {
            &self.use_thundercloud
        }

        fn new_thunder_config(&self, thundercloud_directory: AbsolutePath, invar: AbsolutePath, project_root: AbsolutePath) -> impl ThunderConfig {
            ThunderConfigData::new(
                self.use_thundercloud.clone(),
                thundercloud_directory,
                invar,
                project_root
            )
        }
    }
}

pub trait ThunderConfig : Debug + Send + Sync {
    fn use_thundercloud(&self) -> &impl UseThundercloudConfig;
    fn thundercloud_directory(&self) -> &AbsolutePath;
    fn cumulus(&self) -> &AbsolutePath;
    fn invar(&self) -> &AbsolutePath;
    fn project_root(&self) -> &AbsolutePath;
}

#[derive(Debug)]
struct ThunderConfigData {
    use_thundercloud: UseThundercloudConfigData,
    thundercloud_directory: AbsolutePath,
    cumulus: AbsolutePath,
    invar: AbsolutePath,
    project: AbsolutePath,
}

impl ThunderConfigData {
    fn new(use_thundercloud: UseThundercloudConfigData, thundercloud_directory: AbsolutePath, invar: AbsolutePath, project: AbsolutePath) -> Self {
        let mut cumulus = thundercloud_directory.clone();
        cumulus.push("cumulus");
        ThunderConfigData {
            use_thundercloud,
            thundercloud_directory,
            cumulus,
            invar,
            project,
        }
    }
}

impl ThunderConfig for ThunderConfigData {

    fn use_thundercloud(&self) -> &impl UseThundercloudConfig {
        &self.use_thundercloud
    }

    fn thundercloud_directory(&self) -> &AbsolutePath {
        &self.thundercloud_directory
    }

    fn cumulus(&self) -> &AbsolutePath {
        &self.cumulus
    }

    fn invar(&self) -> &AbsolutePath {
        &self.invar
    }

    fn project_root(&self) -> &AbsolutePath {
        &self.project
    }
}

pub trait UseThundercloudConfig : Debug + Clone {
    type InvarConfigImpl : InvarConfig;
    fn directory(&self) -> Option<&String>;
    fn on_incoming(&self) -> &OnIncoming;
    fn features(&self) -> &[String];
    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl>;
}

#[allow(dead_code)]
#[derive(Deserialize,Debug,Clone)]
struct UseThundercloudConfigData {
    directory: Option<String>,
    #[serde(rename = "git-remote")]
    git_remote: Option<GitRemoteConfig>,
    #[serde(rename = "on-incoming")]
    on_incoming: Option<OnIncoming>,
    features: Option<Vec<String>>,
    #[serde(rename = "invar-defaults")]
    invar_defaults: Option<InvarConfigData>,
}

static UPDATE: Lazy<OnIncoming> = Lazy::new(|| OnIncoming::Update);
static EMPTY_VEC: Lazy<Vec<String>> = Lazy::new(Vec::new);

impl UseThundercloudConfig for UseThundercloudConfigData {
    type InvarConfigImpl = InvarConfigData;
    fn directory(&self) -> Option<&String> {
        self.directory.as_ref()
    }
    fn on_incoming(&self) -> &OnIncoming {
        &self.on_incoming.as_ref().unwrap_or(&UPDATE)
    }
    fn features(&self) -> &[String] {
        &self.features.as_deref().unwrap_or(&EMPTY_VEC)
    }
    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl> {
        if let Some(invar_defaults) = &self.invar_defaults {
            Cow::Borrowed(invar_defaults)
        } else {
            Cow::Owned(Self::InvarConfigImpl::new())
        }
    }
}

#[allow(dead_code)]
#[derive(Deserialize,Debug,Clone)]
struct GitRemoteConfig {
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

#[allow(dead_code)]
pub trait InvarConfig : Clone + Debug + Send + Sync + Sized {
    fn from_reader<R: Read>(reader: R) -> Result<Self>;
    fn with_invar_config<I: InvarConfig>(&self, invar_config: I) -> Cow<Self>;
    fn with_write_mode_option(&self, write_mode: Option<WriteMode>) -> Cow<Self>;
    fn with_write_mode(&self, write_mode: WriteMode) -> Cow<Self>;
    fn write_mode(&self) -> WriteMode;
    fn write_mode_option(&self) -> Option<WriteMode>;
    fn with_interpolate_option(&self, interpolate: Option<bool>) -> Cow<Self>;
    fn with_interpolate(&self, interpolate: bool) -> Cow<Self>;
    fn interpolate(&self) -> bool;
    fn interpolate_option(&self) -> Option<bool>;
    fn with_props_option(&self, props: Option<Mapping>) -> Cow<Self>;
    fn with_props(&self, props: Mapping) -> Cow<Self>;
    fn props(&self) -> Cow<Mapping>;
    fn props_option(&self) -> &Option<Mapping>;
    fn string_props(&self) -> AHashMap<String,String>;
}

#[derive(Deserialize,Debug,Clone)]
struct InvarConfigData {
    #[serde(rename = "write-mode")]
    write_mode: Option<WriteMode>,
    interpolate: Option<bool>,
    props: Option<Mapping>,
}

impl InvarConfigData {
    fn new() -> InvarConfigData {
        InvarConfigData { write_mode: None, interpolate: None, props: None }
    }
}

pub mod invar_config {
    use super::*;

    pub fn from_reader<R: Read>(reader: R) -> Result<impl InvarConfig> {
        let config: InvarConfigData = InvarConfigData::from_reader(reader)?;
        Ok(config)
    }
}

#[allow(dead_code)]
impl InvarConfig for InvarConfigData {
    fn from_reader<R: Read>(reader: R) -> Result<Self> {
        let config: InvarConfigData = serde_yaml::from_reader(reader)?;
        Ok(config)
    }

    fn with_invar_config<I: InvarConfig>(&self, invar_config: I) -> Cow<Self> {
        let dirty = false;
        let (write_mode, dirty) = merge_property(self.write_mode, invar_config.write_mode_option(), dirty);
        debug!("Write mode: {:?} -> {:?} ({:?})", self.write_mode, &write_mode, dirty);
        let (interpolate, dirty) = merge_property(self.interpolate, invar_config.interpolate_option(), dirty);
        debug!("Interpolate: {:?} -> {:?} ({:?})", self.interpolate, &interpolate, dirty);
        let (props, dirty) = merge_props(&self.props, &invar_config.props_option(), dirty);
        if dirty {
            Cow::Owned(InvarConfigData { write_mode, interpolate, props: Some(props.into_owned()) })
        } else {
            Cow::Borrowed(self)
        }
    }

    fn with_write_mode_option(&self, write_mode: Option<WriteMode>) -> Cow<Self> {
        let invar_config = InvarConfigData { write_mode, interpolate: None, props: None };
        self.with_invar_config(invar_config)
    }

    fn with_write_mode(&self, write_mode: WriteMode) -> Cow<Self> {
        self.with_write_mode_option(Some(write_mode))
    }

    fn write_mode(&self) -> WriteMode {
        self.write_mode.unwrap_or(WriteMode::Overwrite)
    }

    fn write_mode_option(&self) -> Option<WriteMode> {
        self.write_mode
    }

    fn with_interpolate_option(&self, interpolate: Option<bool>) -> Cow<Self> {
        let invar_config = InvarConfigData { write_mode: None, interpolate, props: None };
        self.with_invar_config(invar_config)
    }

    fn with_interpolate(&self, interpolate: bool) -> Cow<Self> {
        self.with_interpolate_option(Some(interpolate))
    }

    fn interpolate(&self) -> bool {
        self.interpolate.unwrap_or(true)
    }

    fn interpolate_option(&self) -> Option<bool> {
        self.interpolate
    }

    fn with_props_option(&self, props: Option<Mapping>) -> Cow<Self> {
        let invar_config = InvarConfigData { write_mode: None, interpolate: None, props };
        self.with_invar_config(invar_config)
    }

    fn with_props(&self, props: Mapping) -> Cow<Self> {
        self.with_props_option(Some(props))
    }

    fn props(&self) -> Cow<Mapping> {
        self.props.as_ref().map(Cow::Borrowed).unwrap_or(Cow::Owned(Mapping::new()))
    }

    fn props_option(&self) -> &Option<Mapping> {
        &self.props
    }

    fn string_props(&self) -> AHashMap<String,String> {
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
