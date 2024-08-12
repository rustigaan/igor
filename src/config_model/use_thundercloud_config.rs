use std::borrow::Cow;
use std::fmt::Debug;
use serde::Deserialize;
use crate::config_model::{GitRemoteConfig, InvarConfig};

#[derive(Deserialize,Debug,Clone,Eq, PartialEq)]
pub enum OnIncoming {
    Update,
    Ignore,
    Warn,
    Fail
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
