#![allow(dead_code)]

pub mod invar_config;
pub use invar_config::{InvarConfig, WriteMode};
mod invar_config_data;
use invar_config_data::InvarConfigData;

pub mod niche_description;
pub use niche_description::NicheDescription;
mod niche_description_data;

pub mod thundercloud_config;
pub use thundercloud_config::ThundercloudConfig;
mod thundercloud_config_data;

pub mod niche_config;
pub use niche_config::NicheConfig;
mod niche_config_data;

use anyhow::Result;
use std::borrow::Cow;
use std::fmt::Debug;
use std::io::Read;
use once_cell::sync::Lazy;
use serde::Deserialize;
use crate::path::AbsolutePath;

#[derive(Deserialize,Debug,Clone,Eq, PartialEq)]
pub enum OnIncoming {
    Update,
    Ignore,
    Warn,
    Fail
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
    type GitRemoteConfigImpl : GitRemoteConfig;
    fn directory(&self) -> Option<&String>;
    fn on_incoming(&self) -> &OnIncoming;
    fn features(&self) -> &[String];
    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl>;
    fn git_remote(&self) -> Option<&Self::GitRemoteConfigImpl>;
}

#[derive(Deserialize,Debug,Clone)]
struct UseThundercloudConfigData {
    directory: Option<String>,
    #[serde(rename = "git-remote")]
    git_remote: Option<GitRemoteConfigData>,
    #[serde(rename = "on-incoming")]
    on_incoming: Option<OnIncoming>,
    features: Option<Vec<String>>,
    #[serde(rename = "invar-defaults")]
    invar_defaults: Option<InvarConfigData>,
}

pub trait GitRemoteConfig {
    fn fetch_url(&self) -> &str;
    fn revision(&self) -> &str;
}

#[derive(Deserialize,Debug,Clone)]
struct GitRemoteConfigData {
    #[serde(rename = "fetch-url")]
    fetch_url: String,
    revision: String,
}

impl GitRemoteConfig for GitRemoteConfigData {
    fn fetch_url(&self) -> &str {
        &self.fetch_url
    }
    fn revision(&self) -> &str {
        &self.revision
    }
}

static UPDATE: Lazy<OnIncoming> = Lazy::new(|| OnIncoming::Update);
static EMPTY_VEC: Lazy<Vec<String>> = Lazy::new(Vec::new);

impl UseThundercloudConfig for UseThundercloudConfigData {
    type InvarConfigImpl = InvarConfigData;
    type GitRemoteConfigImpl = GitRemoteConfigData;
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
    fn git_remote(&self) -> Option<&Self::GitRemoteConfigImpl> {
        self.git_remote.as_ref()
    }
}

#[cfg(test)]
mod test_utils {
    use serde_yaml::{Mapping, Value};

    pub fn insert_entry<K: Into<String>, V: Into<String>>(props: &mut Mapping, key: K, value: V) {
        let wrapped_key = Value::String(key.into());
        let wrapped_value = Value::String(value.into());
        props.insert(wrapped_key, wrapped_value);
    }
}
