use super::*;

use crate::config_model::thundercloud_config_data::ThundercloudConfigData;
use crate::file_system::ConfigFormat;

pub fn from_str(body: &str, config_format: ConfigFormat) -> Result<impl ThundercloudConfig> {
    ThundercloudConfigData::from_str(body, config_format)
}

pub trait ThundercloudConfig : Debug + Sized {
    type InvarConfigImpl : InvarConfig;
    fn from_str(toml_data: &str, config_format: ConfigFormat) -> Result<Self>;
    fn niche(&self) -> &impl NicheDescription;
    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl>;
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;
    use indoc::indoc;
    use log::debug;
    use test_log::test;
    use toml::Table;
    use crate::config_model::serde_test_utils::insert_entry;
    use crate::config_model::WriteMode::Overwrite;

    #[test]
    fn test_from_str() -> Result<()> {
        // Given
        let toml = indoc! {r#"
            [niche]
            name = "example"
            description = "Example thundercloud"

            [invar-defaults]
            write-mode = "Overwrite"
            interpolate = true

            [invar-defaults.props]
            alter-ego = "Lobsang"
            milk-man = "Ronny Soak"
        "#};
        debug!("TOML: [{}]", &toml);

        // When
        let thundercloud_config = from_str(toml, ConfigFormat::TOML)?;

        // Then
        assert_eq!(thundercloud_config.niche().name(), "example");
        assert_eq!(thundercloud_config.niche().description(), Some("Example thundercloud"));
        let invar_defaults = thundercloud_config.invar_defaults().into_owned();
        assert_eq!(invar_defaults.write_mode(), Overwrite);
        assert_eq!(invar_defaults.interpolate_option(), Some(true));

        let mut mapping = Table::new();
        insert_entry(&mut mapping, "milk-man", "Ronny Soak");
        insert_entry(&mut mapping, "alter-ego", "Lobsang");
        let mapping = mapping;
        assert_eq!(invar_defaults.props().as_ref(), &mapping);
        Ok(())
    }

    #[test]
    fn test_empty_default_invar() -> Result<()> {
        // Given
        let toml = indoc! {r#"
            [niche]
            name = "example"
        "#};
        debug!("TOML: [{}]", &toml);

        // When
        let thundercloud_config = from_str(toml, ConfigFormat::TOML)?;

        // Then
        assert_eq!(thundercloud_config.niche().name(), "example");
        assert_eq!(thundercloud_config.niche().description(), None);
        let invar_defaults = thundercloud_config.invar_defaults().into_owned();
        assert_eq!(invar_defaults.write_mode_option(), None);
        assert_eq!(invar_defaults.write_mode(), Overwrite);
        assert_eq!(invar_defaults.interpolate_option(), None);
        assert_eq!(invar_defaults.interpolate(), true);

        let mapping = Table::new();
        assert_eq!(invar_defaults.props().as_ref(), &mapping);
        Ok(())
    }
}
