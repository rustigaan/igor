use std::borrow::Cow;
use std::fmt::Debug;
use serde::{Deserialize, Serialize};
use crate::config_model::{GitRemoteConfig, InvarConfig, ThunderConfig};
use crate::file_system::FileSystem;
use crate::path::AbsolutePath;

#[derive(Deserialize,Serialize,Debug,Clone,Eq, PartialEq)]
pub enum OnIncoming {
    Update,
    Ignore,
    Warn,
    Fail
}

pub trait UseThundercloudConfig : Debug + Clone + Send + Sync {
    type InvarConfigImpl : InvarConfig;
    type GitRemoteConfigImpl : GitRemoteConfig + Send + Sync;
    fn directory(&self) -> Option<&str>;
    fn on_incoming(&self) -> &OnIncoming;
    fn features(&self) -> &[String];
    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl>;
    fn git_remote(&self) -> Option<&Self::GitRemoteConfigImpl>;
    fn new_thunder_config<IC: InvarConfig, TFS: FileSystem, PFS: FileSystem>(&self, default_invar_config: IC, thundercloud_fs: TFS, thundercloud_directory: AbsolutePath, project_fs: PFS, invar: AbsolutePath, project_root: AbsolutePath) -> impl ThunderConfig;
}

#[cfg(test)]
pub mod test {
    use super::*;
    use anyhow::Result;
    use indoc::indoc;
    use crate::config_model::invar_config;
    use crate::config_model::use_thundercloud_config_data::UseThundercloudConfigData;
    use crate::file_system::fixture;
    use crate::file_system::ConfigFormat::TOML;
    use crate::path::AbsolutePath;

    #[test]
    fn use_thundercloud_config() -> Result<()> {
        super::super::niche_config::test::test_from_reader()
    }

    #[test]
    pub fn test_new_thunder_config() -> Result<()> {
        // Given
        let toml_source = indoc! {r#"
            directory = "{{PROJECT}}/example-thundercloud"
        "#};
        let use_thundercloud_config: UseThundercloudConfigData = toml::from_str(toml_source)?;
        let root = AbsolutePath::root();
        let thunder_cloud_dir = AbsolutePath::new("/tmp", &root);
        let project_root = root;
        let invar_dir = AbsolutePath::new("/var/tmp", &project_root);
        let cumulus = AbsolutePath::new("cumulus", &thunder_cloud_dir);
        let fs = fixture::from_toml("")?;
        let default_invar_config = invar_config::from_str("", TOML)?;

        // When
        let thunder_config = use_thundercloud_config.new_thunder_config(default_invar_config, fs.clone(), thunder_cloud_dir.clone(), fs.clone(), invar_dir.clone(), project_root.clone());

        // Then
        assert_eq!(thunder_config.use_thundercloud().directory(), use_thundercloud_config.directory());
        assert_eq!(thunder_config.project_root().as_path(), project_root.as_path());
        assert_eq!(thunder_config.thundercloud_directory().as_path(), thunder_cloud_dir.as_path());
        assert_eq!(thunder_config.invar().as_path(), invar_dir.as_path());
        assert_eq!(thunder_config.cumulus().as_path(), cumulus.as_path());
        Ok(())
    }
}
