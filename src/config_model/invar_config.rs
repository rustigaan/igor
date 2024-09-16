use crate::config_model::invar_config_data::InvarConfigData;

use anyhow::Result;
use std::borrow::Cow;
use std::fmt::Debug;
use ahash::AHashMap;
use serde::{Deserialize, Serialize};
use toml::Table;
use crate::file_system::ConfigFormat;

#[derive(Deserialize,Serialize,Debug,Clone,Copy,Eq, PartialEq)]
pub enum WriteMode {
    Overwrite,
    WriteNew,
    Ignore
}

pub trait InvarConfig : Clone + Debug + Send + Sync + Sized {
    fn from_toml(body: &str) -> Result<Self>;
    fn from_yaml(body: &str) -> Result<Self>;
    fn with_invar_config<I: InvarConfig>(&self, invar_config: I) -> Cow<Self>;
    fn with_write_mode_option(&self, write_mode: Option<WriteMode>) -> Cow<Self>;
    fn with_write_mode(&self, write_mode: WriteMode) -> Cow<Self>;
    fn write_mode(&self) -> WriteMode;
    fn write_mode_option(&self) -> Option<WriteMode>;
    fn with_interpolate_option(&self, interpolate: Option<bool>) -> Cow<Self>;
    fn with_interpolate(&self, interpolate: bool) -> Cow<Self>;
    fn interpolate(&self) -> bool;
    fn interpolate_option(&self) -> Option<bool>;
    fn with_props_option(&self, props: Option<Table>) -> Cow<Self>;
    fn with_props(&self, props: Table) -> Cow<Self>;
    fn props(&self) -> Cow<Table>;
    fn props_option(&self) -> &Option<Table>;
    fn string_props(&self) -> AHashMap<String,String>;
}

pub fn from_str(body: &str, config_format: ConfigFormat) -> Result<impl InvarConfig> {
    match config_format {
        ConfigFormat::TOML => InvarConfigData::from_toml(body),
        ConfigFormat::YAML => InvarConfigData::from_yaml(body),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn invar_config_from_reader() -> Result<()> {
        let yaml_source = r#"{ "write-mode": "WriteNew" }"#;
        let invar_config = from_str(yaml_source, ConfigFormat::YAML)?;
        assert_eq!(invar_config.write_mode(), WriteMode::WriteNew); // From YAML
        assert_eq!(invar_config.interpolate(), true); // Default value
        assert_eq!(invar_config.props(), Cow::Owned(Table::new())); // Default value
        Ok(())
    }
}
