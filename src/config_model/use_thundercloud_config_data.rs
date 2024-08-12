use super::{UseThundercloudConfig,OnIncoming};
use super::git_remote_config_data::GitRemoteConfigData;
use super::invar_config_data::InvarConfigData;
use std::borrow::Cow;
use once_cell::sync::Lazy;
use serde::Deserialize;

#[derive(Deserialize,Debug,Clone)]
pub struct UseThundercloudConfigData {
    directory: Option<String>,
    #[serde(rename = "git-remote")]
    git_remote: Option<GitRemoteConfigData>,
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
