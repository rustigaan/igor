use stringreader::StringReader;
use super::*;

use crate::config_model::thundercloud_config_data::ThundercloudConfigData;

pub fn from_reader<R: Read>(reader: R) -> Result<impl ThundercloudConfig> {
    let config: ThundercloudConfigData = ThundercloudConfig::from_yaml(reader)?;
    Ok(config)
}

pub fn from_string(body: String) -> Result<impl ThundercloudConfig> {
    ThundercloudConfigData::from_yaml(StringReader::new(&body))
}

pub trait ThundercloudConfig : Debug + Sized {
    type InvarConfigImpl : InvarConfig;
    fn from_yaml<R: Read>(reader: R) -> Result<Self>;
    fn niche(&self) -> &impl NicheDescription;
    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl>;
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;
    use indoc::indoc;
    use log::debug;
    use serde_yaml::Mapping;
    use stringreader::StringReader;
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
        let yaml_source = StringReader::new(yaml);

        // When
        let thundercloud_config = from_reader(yaml_source)?;

        // Then
        assert_eq!(thundercloud_config.niche().name(), "example");
        assert_eq!(thundercloud_config.niche().description(), Some("Example thundercloud"));
        let invar_defaults = thundercloud_config.invar_defaults().into_owned();
        assert_eq!(invar_defaults.write_mode(), Overwrite);
        assert_eq!(invar_defaults.interpolate_option(), Some(true));

        let mut mapping = Mapping::new();
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
        let yaml_source = StringReader::new(yaml);

        // When
        let thundercloud_config = from_reader(yaml_source)?;

        // Then
        assert_eq!(thundercloud_config.niche().name(), "example");
        assert_eq!(thundercloud_config.niche().description(), None);
        let invar_defaults = thundercloud_config.invar_defaults().into_owned();
        assert_eq!(invar_defaults.write_mode_option(), None);
        assert_eq!(invar_defaults.write_mode(), Overwrite);
        assert_eq!(invar_defaults.interpolate_option(), None);
        assert_eq!(invar_defaults.interpolate(), true);

        let mapping = Mapping::new();
        assert_eq!(invar_defaults.props().as_ref(), &mapping);
        Ok(())
    }
}
