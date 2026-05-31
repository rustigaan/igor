use crate::config_model::project_config_data::ProjectConfigData;
use crate::config_model::psychotropic::PsychotropicConfig;
use crate::config_model::InvarConfig;
use crate::file_system::ConfigFormat;
use crate::path::RelativePath;
use anyhow::Result;
use std::borrow::Cow;
use std::fmt::Debug;

pub trait ProjectConfig: Debug + Sized {
    type InvarConfigImpl: InvarConfig;
    fn from_str(toml_data: &str, config_format: ConfigFormat) -> anyhow::Result<Self>;
    fn niches_directory(&self) -> RelativePath;
    fn psychotropic(&self) -> Result<impl PsychotropicConfig>;
    fn invar_defaults<'a>(&'a self) -> Cow<'a, Self::InvarConfigImpl>;
}

pub fn from_str(data: &str, config_format: ConfigFormat) -> Result<impl ProjectConfig> {
    ProjectConfigData::from_str(data, config_format)
}
