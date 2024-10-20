use super::niche_config_data::NicheConfigData;

use std::fmt::Debug;
use crate::config_model::{ThunderConfig, UseThundercloudConfig};
use crate::file_system::{ConfigFormat, FileSystem};

pub trait NicheConfig : Sized + Debug {
    fn from_str(body: &str, config_format: ConfigFormat) -> Result<Self>;
    fn use_thundercloud(&self) -> &impl UseThundercloudConfig;
}
use super::*;

pub fn from_str(body: &str, config_format: ConfigFormat) -> Result<impl NicheConfig> {
    NicheConfigData::from_str(body, config_format)
}

#[cfg(test)]
pub mod test {
    use super::*;
    use super::serde_test_utils::insert_entry;
    use super::OnIncoming::Update;
    use super::WriteMode::Ignore;
    use indoc::indoc;
    use log::debug;
    use test_log::test;
    use toml::Table;
    use crate::config_model::invar_config;
    use crate::file_system::ConfigFormat::TOML;
    use crate::file_system::fixture;

    #[test]
    pub fn test_from_reader() -> Result<()> {
        // Given
        let toml_data = indoc! {r#"
            [use-thundercloud]
            directory = "{{PROJECT}}/example-thundercloud"
            on-incoming = "Update"
            features = ["glass", "bash_config", "kermie"]

            [use-thundercloud.invar-defaults]
            write-mode = "Ignore"
            interpolate = false

            [use-thundercloud.invar-defaults.props]
            marthter = "Jeremy"
            buyer = "Myra LeJean"
            milk-man = "Kaos"
        "#};
        debug!("TOML: [[[\n{}\n]]]", &toml_data);

        // When
        let niche_config = from_str(toml_data, ConfigFormat::TOML)?;

        // Then
        let use_thundercloud = niche_config.use_thundercloud();
        assert_eq!(use_thundercloud.directory(), Some("{{PROJECT}}/example-thundercloud"));
        assert_eq!(use_thundercloud.on_incoming(), &Update);
        assert_eq!(use_thundercloud.features(), &["glass", "bash_config", "kermie"]);

        let invar_defaults = use_thundercloud.invar_defaults().into_owned();
        assert_eq!(invar_defaults.write_mode_option(), Some(Ignore));
        assert_eq!(invar_defaults.interpolate_option(), Some(false));

        let mut mapping = Table::new();
        insert_entry(&mut mapping, "marthter", "Jeremy");
        insert_entry(&mut mapping, "buyer", "Myra LeJean");
        insert_entry(&mut mapping, "milk-man", "Kaos");
        assert_eq!(invar_defaults.props().into_owned(), mapping);

        Ok(())
    }
}
