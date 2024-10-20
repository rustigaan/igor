use super::{UseThundercloudConfig, OnIncoming, InvarConfig, ThunderConfig};
use super::git_remote_config_data::GitRemoteConfigData;
use super::invar_config_data::InvarConfigData;
use std::borrow::Cow;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use crate::config_model::invar_config::invar_config_or_default;
use crate::config_model::thunder_config_data::ThunderConfigData;
use crate::file_system::FileSystem;
use crate::path::AbsolutePath;

#[derive(Deserialize,Serialize,Debug,Clone)]
#[serde(rename_all = "kebab-case")]
pub struct UseThundercloudConfigData {
    directory: Option<String>,
    git_remote: Option<GitRemoteConfigData>,
    on_incoming: Option<OnIncoming>,
    features: Option<Vec<String>>,
    invar_defaults: Option<InvarConfigData>,
}

static UPDATE: Lazy<OnIncoming> = Lazy::new(|| OnIncoming::Update);
static EMPTY_VEC: Lazy<Vec<String>> = Lazy::new(Vec::new);

impl UseThundercloudConfig for UseThundercloudConfigData {
    type InvarConfigImpl = InvarConfigData;
    type GitRemoteConfigImpl = GitRemoteConfigData;
    fn directory(&self) -> Option<&str> {
        self.directory.as_ref().map(String::as_ref)
    }
    fn on_incoming(&self) -> &OnIncoming {
        &self.on_incoming.as_ref().unwrap_or(&UPDATE)
    }
    fn features(&self) -> &[String] {
        &self.features.as_deref().unwrap_or(&EMPTY_VEC)
    }
    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl> {
        invar_config_or_default(&self.invar_defaults)
    }
    fn git_remote(&self) -> Option<&Self::GitRemoteConfigImpl> {
        self.git_remote.as_ref()
    }
    fn new_thunder_config<IC: InvarConfig, TFS: FileSystem, PFS: FileSystem>(&self, default_invar_config: IC, thundercloud_fs: TFS, thundercloud_directory: AbsolutePath, project_fs: PFS, invar: AbsolutePath, project_root: AbsolutePath) -> impl ThunderConfig {
        ThunderConfigData::new(
            self.clone(),
            default_invar_config,
            thundercloud_directory,
            invar,
            project_root,
            thundercloud_fs,
            project_fs
        )
    }
}
