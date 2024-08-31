use std::io::Read;
use serde::Deserialize;
use crate::file_system::FileSystem;
use super::{ThunderConfig, UseThundercloudConfig, NicheConfig};
use super::thunder_config_data::ThunderConfigData;
use super::use_thundercloud_config_data::UseThundercloudConfigData;
use crate::path::AbsolutePath;

#[derive(Deserialize,Debug)]
#[serde(rename_all = "kebab-case")]
pub struct NicheConfigData {
    use_thundercloud: UseThundercloudConfigData,
}

impl NicheConfig for NicheConfigData {
    fn from_reader<R: Read>(reader: R) -> anyhow::Result<Self> {
        let config: NicheConfigData = serde_yaml::from_reader(reader)?;
        Ok(config)
    }

    fn use_thundercloud(&self) -> &impl UseThundercloudConfig {
        &self.use_thundercloud
    }

    fn new_thunder_config<TFS: FileSystem, PFS: FileSystem>(&self, thundercloud_fs: TFS, thundercloud_directory: AbsolutePath, project_fs: PFS, invar: AbsolutePath, project_root: AbsolutePath) -> impl ThunderConfig {
        ThunderConfigData::new(
            self.use_thundercloud.clone(),
            thundercloud_directory,
            invar,
            project_root,
            thundercloud_fs,
            project_fs
        )
    }
}
