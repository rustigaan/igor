use std::borrow::Cow;
use std::fmt::Debug;
use serde::{Deserialize, Serialize};
use crate::config_model::{GitRemoteConfig, InvarConfig};

#[derive(Deserialize,Serialize,Debug,Clone,Eq, PartialEq)]
pub enum OnIncoming {
    Update,
    Ignore,
    Warn,
    Fail
}

pub trait UseThundercloudConfig : Debug + Clone {
    type InvarConfigImpl : InvarConfig;
    type GitRemoteConfigImpl : GitRemoteConfig;
    fn directory(&self) -> Option<&str>;
    fn on_incoming(&self) -> &OnIncoming;
    fn features(&self) -> &[String];
    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl>;
    fn git_remote(&self) -> Option<&Self::GitRemoteConfigImpl>;
}

#[cfg(test)]
mod test {
    use anyhow::Result;

    #[test]
    fn use_thundercloud_config() -> Result<()> {
        super::super::niche_config::test::test_from_reader()
    }
}
