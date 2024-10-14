use std::borrow::Cow;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::config_model::invar_config::invar_config_or_default;
use crate::config_model::invar_config_data::InvarConfigData;
use crate::config_model::project_config::ProjectConfig;
use crate::config_model::psychotropic::PsychotropicConfig;
use crate::config_model::psychotropic_data;
use crate::config_model::psychotropic_data::{data_to_index, PsychotropicConfigData};
use crate::file_system::ConfigFormat;
use crate::path::RelativePath;

#[derive(Deserialize, Serialize, Debug,Default)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectConfigData {
    niches_directory: Option<String>,
    igor_settings: Option<String>,
    psychotropic: Option<PsychotropicConfigData>,
    invar_defaults: Option<InvarConfigData>,
}

impl ProjectConfig for ProjectConfigData {
    type InvarConfigImpl = InvarConfigData;

    fn from_str(data: &str, config_format: ConfigFormat) -> Result<Self> {
        let project_config: ProjectConfigData = match config_format {
            ConfigFormat::TOML => toml::from_str(data)?,
            ConfigFormat::YAML => {
                let result = serde_yaml::from_str(data)?;
                result
            }
        };
        Ok(project_config)
    }

    fn niches_directory(&self) -> RelativePath {
        if let Some(dir) = &self.niches_directory {
            RelativePath::from((*dir).clone())
        } else {
            RelativePath::from("yeth-marthter")
        }
    }

    fn igor_settings(&self) -> String {
        if let Some(base) = &self.igor_settings {
            base.clone()
        } else {
            "igor-thettingth".to_string()
        }
    }

    fn psychotropic(&self) -> Result<impl PsychotropicConfig> {
        if let Some(psychotropic) = &self.psychotropic {
            data_to_index(psychotropic)
        } else {
            Ok(psychotropic_data::empty())
        }
    }

    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl> {
        invar_config_or_default(&self.invar_defaults)
    }
}
