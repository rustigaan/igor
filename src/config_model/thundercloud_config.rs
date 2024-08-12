use super::*;

use crate::config_model::thundercloud_config_data::ThundercloudConfigData;

pub fn from_reader<R: Read>(reader: R) -> Result<impl ThundercloudConfig> {
    let config: ThundercloudConfigData = ThundercloudConfig::from_reader(reader)?;
    Ok(config)
}

pub trait ThundercloudConfig : Debug + Sized {
    type InvarConfigImpl : InvarConfig;
    fn from_reader<R: Read>(reader: R) -> Result<Self>;
    fn niche(&self) -> &impl NicheDescription;
    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl>;
}
