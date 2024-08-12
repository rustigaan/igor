use std::io::Read;
use serde::Deserialize;
use crate::config_model::niche_config::NicheConfig;
use crate::config_model::{ThunderConfig, ThunderConfigData, UseThundercloudConfig, UseThundercloudConfigData};
use crate::path::AbsolutePath;

#[derive(Deserialize,Debug)]
pub struct NicheConfigData {
    #[serde(rename = "use-thundercloud")]
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

    fn new_thunder_config(&self, thundercloud_directory: AbsolutePath, invar: AbsolutePath, project_root: AbsolutePath) -> impl ThunderConfig {
        ThunderConfigData::new(
            self.use_thundercloud.clone(),
            thundercloud_directory,
            invar,
            project_root
        )
    }
}
