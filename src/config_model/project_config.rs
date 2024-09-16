use anyhow::Result;
use std::fmt::Debug;
use crate::config_model::project_config_data::ProjectConfigData;
use crate::config_model::psychotropic::PsychotropicConfig;
use crate::file_system::ConfigFormat;
use crate::path::RelativePath;

pub trait ProjectConfig: Debug + Sized {
    fn from_str(toml_data: &str, config_format: ConfigFormat) -> anyhow::Result<Self>;
    fn niches_directory(&self) -> RelativePath;
    fn igor_settings(&self) -> String;
    fn psychotropic(&self) -> Result<impl PsychotropicConfig>;
}

pub fn from_str(data: &str, config_format: ConfigFormat) -> Result<impl ProjectConfig> {
    ProjectConfigData::from_str(data, config_format)
}
