use super::*;

use crate::config_model::thundercloud_config_data::ThundercloudConfigData;
use crate::file_system::ConfigFormat;

pub fn from_str(body: &str, config_format: ConfigFormat) -> Result<impl ThundercloudConfig> {
    match config_format {
        ConfigFormat::TOML => ThundercloudConfigData::from_toml(body),
        ConfigFormat::YAML => ThundercloudConfigData::from_yaml(body)
    }
}

pub trait ThundercloudConfig : Debug + Sized {
    type InvarConfigImpl : InvarConfig;
    fn from_toml(toml_data: &str) -> Result<Self>;
    fn from_yaml(yaml_data: &str) -> Result<Self>;
    fn niche(&self) -> &impl NicheDescription;
    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl>;
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;
    use indoc::indoc;
    use log::debug;
    use toml::Table;
    use crate::config_model::serde_test_utils::insert_entry;
    use crate::config_model::WriteMode::Overwrite;

    #[test]
    fn test_from_reader() -> Result<()> {
        // Given
        let yaml = indoc! {r#"
                ---
                niche:
                  name: example
                  description: Example thundercloud
                invar-defaults:
                  write-mode: Overwrite
                  interpolate: true
                  props:
                    milk-man: Ronny Soak
                    alter-ego: Lobsang
            "#};
        debug!("YAML: [{}]", &yaml);

        // When
        let thundercloud_config = from_str(yaml, ConfigFormat::YAML)?;

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
        let yaml = indoc! {r#"
                ---
                niche:
                  name: example
            "#};
        debug!("YAML: [{}]", &yaml);

        // When
        let thundercloud_config = from_str(yaml, ConfigFormat::YAML)?;

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
