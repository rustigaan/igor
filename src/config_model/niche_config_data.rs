use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::file_system::ConfigFormat;
use super::{UseThundercloudConfig, NicheConfig};
use super::use_thundercloud_config_data::UseThundercloudConfigData;

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
}
