use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::file_system::{ConfigFormat, FileSystem};
use super::{ThunderConfig, UseThundercloudConfig, NicheConfig, InvarConfig};
use super::thunder_config_data::ThunderConfigData;
use super::use_thundercloud_config_data::UseThundercloudConfigData;
use crate::path::AbsolutePath;

#[derive(Deserialize,Serialize,Debug)]
#[serde(rename_all = "kebab-case")]
pub struct NicheConfigData {
    use_thundercloud: UseThundercloudConfigData,
}

impl NicheConfig for NicheConfigData {
    fn from_str(body: &str, config_format: ConfigFormat) -> Result<Self> {
        let niche_config: NicheConfigData = match config_format {
            ConfigFormat::TOML => toml::from_str(body)?,
            ConfigFormat::YAML => {
                let result = serde_yaml::from_str(body)?;

                #[cfg(test)]
                crate::test_utils::log_toml("Niche Config", &result)?;

                result
            }
        };
        Ok(niche_config)
    }

    fn use_thundercloud(&self) -> &impl UseThundercloudConfig {
        &self.use_thundercloud
    }

    fn new_thunder_config<IC: InvarConfig, TFS: FileSystem, PFS: FileSystem>(&self, default_invar_config: IC, thundercloud_fs: TFS, thundercloud_directory: AbsolutePath, project_fs: PFS, invar: AbsolutePath, project_root: AbsolutePath) -> impl ThunderConfig {
        ThunderConfigData::new(
            self.use_thundercloud.clone(),
            default_invar_config,
            thundercloud_directory,
            invar,
            project_root,
            thundercloud_fs,
            project_fs
        )
    }
}
