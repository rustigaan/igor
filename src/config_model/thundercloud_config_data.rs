use std::borrow::Cow;
use std::io::Read;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use super::invar_config_data::InvarConfigData;
use crate::config_model::{NicheDescription, ThundercloudConfig};
use crate::config_model::niche_description::NicheDescriptionData;

#[derive(Deserialize,Serialize,Debug)]
#[serde(rename_all = "kebab-case")]
pub struct ThundercloudConfigData {
    niche: NicheDescriptionData,
    invar_defaults: Option<InvarConfigData>
}

impl ThundercloudConfig for ThundercloudConfigData {
    type InvarConfigImpl = InvarConfigData;

    fn from_yaml<R: Read>(reader: R) -> Result<Self> {
        let config: ThundercloudConfigData = serde_yaml::from_reader(reader)?;

        #[cfg(test)]
        crate::test_utils::log_toml("Fixture file system", &config)?;

        Ok(config)
    }

    fn niche(&self) -> &impl NicheDescription {
        &self.niche
    }

    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl> {
        let result: Cow<Self::InvarConfigImpl>;
        if let Some(invar_config) = &self.invar_defaults {
            result = Cow::Borrowed(invar_config)
        } else {
            result = Cow::Owned(InvarConfigData::new())
        }
        result
    }
}
