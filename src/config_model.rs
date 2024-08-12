#![allow(dead_code)]

pub mod invar_config;
pub use invar_config::{InvarConfig, WriteMode};
mod invar_config_data;

pub mod niche_description;
pub use niche_description::NicheDescription;
mod niche_description_data;

pub mod thundercloud_config;
pub use thundercloud_config::ThundercloudConfig;
mod thundercloud_config_data;

pub mod niche_config;
pub use niche_config::NicheConfig;
mod niche_config_data;

mod thunder_config;
pub use thunder_config::ThunderConfig;
mod thunder_config_data;

mod use_thundercloud_config;
pub use use_thundercloud_config::{UseThundercloudConfig,OnIncoming};
mod use_thundercloud_config_data;

mod git_remote_config;
pub use git_remote_config::GitRemoteConfig;
mod git_remote_config_data;

use anyhow::Result;
use std::borrow::Cow;
use std::fmt::Debug;
use std::io::Read;

#[cfg(test)]
mod serde_test_utils {
    use serde_yaml::{Mapping, Value};

    pub fn insert_entry<K: Into<String>, V: Into<String>>(props: &mut Mapping, key: K, value: V) {
        let wrapped_key = Value::String(key.into());
        let wrapped_value = Value::String(value.into());
        props.insert(wrapped_key, wrapped_value);
    }
}
