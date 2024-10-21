use std::borrow::Cow;
use anyhow::Result;
use std::fmt::Debug;
use crate::config_model::InvarConfig;
use crate::config_model::project_config_data::ProjectConfigData;
use crate::config_model::psychotropic::PsychotropicConfig;
use crate::file_system::ConfigFormat;
use crate::path::RelativePath;

pub trait ProjectConfig: Debug + Sized {
    type InvarConfigImpl : InvarConfig;
    fn from_str(toml_data: &str, config_format: ConfigFormat) -> anyhow::Result<Self>;
    fn niches_directory(&self) -> RelativePath;
    fn psychotropic(&self) -> Result<impl PsychotropicConfig>;
    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl>;
}

pub fn from_str(data: &str, config_format: ConfigFormat) -> Result<impl ProjectConfig> {
    ProjectConfigData::from_str(data, config_format)
}
