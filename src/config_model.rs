use std::borrow::Cow;
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

#[derive(Deserialize,Debug)]
pub struct NicheConfig {
    #[serde(rename = "use-thundercloud")]
    use_thundercloud: UseThundercloudConfig,
}

impl NicheConfig {
    pub fn use_thundercloud(&self) -> &UseThundercloudConfig {
        &self.use_thundercloud
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
    params: Option<Mapping>,
}

impl UseThundercloudConfig {
    pub fn directory(&self) -> Option<&String> {
        self.directory.as_ref()
    }
}

static UPDATE: Lazy<OnIncoming> = Lazy::new(|| OnIncoming::Update);
static EMPTY_VEC: Lazy<Vec<String>> = Lazy::new(Vec::new);

impl UseThundercloudConfig {
    pub fn on_incoming(&self) -> &OnIncoming {
        &self.on_incoming.as_ref().unwrap_or(&UPDATE)
    }
    pub fn features(&self) -> &[String] {
        &self.features.as_deref().unwrap_or(&EMPTY_VEC)
    }

    pub fn params(&self) -> AHashMap<String,String> {
        if let Some(params) = &self.params {
            let map: AHashMap<String, String> = params.iter().map(to_strings).filter(Option::is_some).map(Option::unwrap).collect();
            map
        } else {
            AHashMap::new()
        }
    }
}

fn to_strings(entry: (&Value, &Value)) -> Option<(String, String)> {
    if let (Value::String(key), Value::String(val)) = entry {
        Some((key.to_owned(), val.to_owned()))
    } else {
        None
    }
}

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
}

impl InvarConfig {
    pub fn new() -> InvarConfig {
        InvarConfig { write_mode: None, interpolate: None }
    }

    pub fn with_invar_config(&self, invar_config: &InvarConfig) -> Cow<InvarConfig> {
        let dirty = false;
        let (write_mode, dirty) = merge_property(self.write_mode, invar_config.write_mode, dirty);
        debug!("Write mode: {:?} -> {:?} ({:?})", self.write_mode, &write_mode, dirty);
        let (interpolate, dirty) = merge_property(self.interpolate, invar_config.interpolate, dirty);
        debug!("Interpolate: {:?} -> {:?} ({:?})", self.interpolate, &interpolate, dirty);
        if dirty {
            Cow::Owned(InvarConfig { write_mode, interpolate })
        } else {
            Cow::Borrowed(self)
        }
    }

    pub fn with_write_mode_option(&self, write_mode: Option<WriteMode>) -> Cow<InvarConfig> {
        let invar_config = InvarConfig { write_mode, interpolate: None };
        self.with_invar_config(&invar_config)
    }

    pub fn with_write_mode(&self, write_mode: WriteMode) -> Cow<InvarConfig> {
        self.with_write_mode_option(Some(write_mode))
    }

    pub fn write_mode(&self) -> WriteMode {
        self.write_mode.unwrap_or(WriteMode::Overwrite)
    }

    pub fn with_interpolate_option(&self, interpolate: Option<bool>) -> Cow<InvarConfig> {
        let invar_config = InvarConfig { write_mode: None, interpolate };
        self.with_invar_config(&invar_config)
    }

    pub fn with_interpolate(&self, interpolate: bool) -> Cow<InvarConfig> {
        self.with_interpolate_option(Some(interpolate))
    }

    pub fn interpolate(&self) -> bool {
        self.interpolate.unwrap_or(true)
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