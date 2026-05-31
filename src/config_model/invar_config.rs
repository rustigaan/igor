use crate::config_model::invar_config_data::InvarConfigData;

use crate::file_system::ConfigFormat;
use ahash::AHashMap;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::Debug;
use toml::Table;

#[derive(Deserialize, Serialize, Debug, Clone, Copy, Eq, PartialEq)]
pub enum WriteMode {
    Overwrite,
    WriteNew,
    Ignore,
}

pub trait InvarConfig: Clone + Debug + Send + Sync + Sized {
    fn from_str(body: &str, config_format: ConfigFormat) -> Result<Self>;
    fn clone_state(&self) -> impl InvarState;
    fn target(&self) -> Option<&String>;
}

pub trait InvarState: Default + Clone + Debug + Send + Sync + Sized {
    fn with_invar_state<'a, I: InvarState>(&'a self, invar_config: I) -> Cow<'a, Self>;
    fn with_write_mode_option<'a>(&'a self, write_mode: Option<WriteMode>) -> Cow<'a, Self>;
    fn with_write_mode<'a>(&'a self, write_mode: WriteMode) -> Cow<'a, Self>;
    fn write_mode(&self) -> WriteMode;
    fn write_mode_option(&self) -> Option<WriteMode>;
    fn with_executable_option<'a>(&'a self, executable: Option<bool>) -> Cow<'a, Self>;
    fn with_executable<'a>(&'a self, executable: bool) -> Cow<'a, Self>;
    fn executable(&self) -> bool;
    fn executable_option(&self) -> Option<bool>;
    fn with_interpolate_option<'a>(&'a self, interpolate: Option<bool>) -> Cow<'a, Self>;
    fn with_interpolate<'a>(&'a self, interpolate: bool) -> Cow<'a, Self>;
    fn interpolate(&self) -> bool;
    fn interpolate_option(&self) -> Option<bool>;
    fn with_props_option<'a>(&'a self, props: Option<Table>) -> Cow<'a, Self>;
    fn with_props<'a>(&'a self, props: Table) -> Cow<'a, Self>;
    fn props<'a>(&'a self) -> Cow<'a, Table>;
    fn props_option(&self) -> &Option<Table>;
    fn string_props(&self) -> AHashMap<String, String>;
}

pub fn from_str(body: &str, config_format: ConfigFormat) -> Result<impl InvarConfig> {
    InvarConfigData::from_str(body, config_format)
}

pub fn invar_config_or_default<'a, IC>(option: Option<&'a IC>) -> Cow<'a, IC>
where
    IC: InvarConfig + Default,
{
    if let Some(invar_defaults) = option {
        Cow::Borrowed(invar_defaults)
    } else {
        Cow::Owned(IC::default())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn invar_config_from_str() -> Result<()> {
        let toml_source = r#"write-mode = "WriteNew" "#;
        let invar_config = from_str(toml_source, ConfigFormat::TOML)?;
        let invar_state = invar_config.clone_state();
        assert_eq!(invar_state.write_mode(), WriteMode::WriteNew); // From YAML
        assert_eq!(invar_state.interpolate(), true); // Default value
        assert_eq!(invar_state.props(), Cow::Owned(Table::new())); // Default value
        assert_eq!(invar_config.target(), None); // Default value
        Ok(())
    }
}
