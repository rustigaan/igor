use crate::config_model::invar_config_data::InvarConfigData;

use anyhow::Result;
use std::borrow::Cow;
use std::fmt::Debug;
use std::io::Read;
use ahash::AHashMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Mapping;
use stringreader::StringReader;

#[derive(Deserialize,Serialize,Debug,Clone,Copy,Eq, PartialEq)]
pub enum WriteMode {
    Overwrite,
    WriteNew,
    Ignore
}

pub trait InvarConfig : Clone + Debug + Send + Sync + Sized {
    fn from_yaml<R: Read>(reader: R) -> Result<Self>;
    fn with_invar_config<I: InvarConfig>(&self, invar_config: I) -> Cow<Self>;
    fn with_write_mode_option(&self, write_mode: Option<WriteMode>) -> Cow<Self>;
    fn with_write_mode(&self, write_mode: WriteMode) -> Cow<Self>;
    fn write_mode(&self) -> WriteMode;
    fn write_mode_option(&self) -> Option<WriteMode>;
    fn with_interpolate_option(&self, interpolate: Option<bool>) -> Cow<Self>;
    fn with_interpolate(&self, interpolate: bool) -> Cow<Self>;
    fn interpolate(&self) -> bool;
    fn interpolate_option(&self) -> Option<bool>;
    fn with_props_option(&self, props: Option<Mapping>) -> Cow<Self>;
    fn with_props(&self, props: Mapping) -> Cow<Self>;
    fn props(&self) -> Cow<Mapping>;
    fn props_option(&self) -> &Option<Mapping>;
    fn string_props(&self) -> AHashMap<String,String>;
}

/// Reads invar configuration from a YAML file.
pub fn from_reader<R: Read>(reader: R) -> Result<impl InvarConfig> {
    let config: InvarConfigData = InvarConfigData::from_yaml(reader)?;
    Ok(config.with_props_option(None).into_owned())
}

pub fn from_string(body: String) -> Result<impl InvarConfig> {
    let invar_config = InvarConfigData::from_yaml(StringReader::new(&body))?;
    Ok(invar_config)
}

#[cfg(test)]
mod test {
    use super::*;
    use stringreader::StringReader;

    #[test]
    fn invar_config_from_reader() -> Result<()> {
        let yaml_source = StringReader::new(r#"{ "write-mode": "WriteNew" }"#);
        let invar_config = from_reader(yaml_source)?;
        assert_eq!(invar_config.write_mode(), WriteMode::WriteNew); // From YAML
        assert_eq!(invar_config.interpolate(), true); // Default value
        assert_eq!(invar_config.props(), Cow::Owned(Mapping::new())); // Default value
        Ok(())
    }
}
