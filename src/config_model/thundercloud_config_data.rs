use super::invar_config_data::InvarConfigData;
use crate::config_model::niche_description::NicheDescriptionData;
use crate::config_model::{NicheDescription, ThundercloudConfig};
use crate::file_system::ConfigFormat;
use crate::NicheName;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct ThundercloudConfigData {
    niche: NicheDescriptionData,
    invar_defaults: Option<InvarConfigData>,
}

impl ThundercloudConfig for ThundercloudConfigData {
    type InvarConfigImpl = InvarConfigData;

    fn from_str(data: &str, config_format: ConfigFormat) -> Result<Self> {
        let config: ThundercloudConfigData = match config_format {
            ConfigFormat::TOML => toml::from_str(data)?,
            ConfigFormat::YAML => {
                let result = serde_yaml::from_str(data)?;

                #[cfg(test)]
                crate::test_utils::log_toml("Thundercloud Config", &result)?;

                result
            }
        };
        Ok(config)
    }

    fn niche(&self) -> &impl NicheDescription {
        &self.niche
    }

    fn invar_defaults<'a>(&'a self) -> Cow<'a, Self::InvarConfigImpl> {
        let result: Cow<Self::InvarConfigImpl>;
        if let Some(invar_config) = &self.invar_defaults {
            result = Cow::Borrowed(invar_config)
        } else {
            result = Cow::Owned(InvarConfigData::default())
        }
        result
    }
}

impl From<NicheDescriptionData> for ThundercloudConfigData {
    fn from(niche: NicheDescriptionData) -> Self {
        ThundercloudConfigData {
            niche,
            invar_defaults: None,
        }
    }
}

impl From<&NicheName> for ThundercloudConfigData {
    fn from(niche: &NicheName) -> Self {
        let niche_description = NicheDescriptionData::new(niche.0.to_string(), None::<String>);
        ThundercloudConfigData::from(niche_description)
    }
}
