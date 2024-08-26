use super::niche_config_data::NicheConfigData;

use std::fmt::Debug;
use std::io::Read;
use crate::config_model::{ThunderConfig, UseThundercloudConfig};
use crate::file_system::FileSystem;
use crate::path::AbsolutePath;

pub trait NicheConfig : Sized + Debug {
    fn from_reader<R: Read>(reader: R) -> anyhow::Result<Self>;
    fn use_thundercloud(&self) -> &impl UseThundercloudConfig;
    fn new_thunder_config<TFS: FileSystem, PFS: FileSystem>(&self, thundercloud_fs: TFS, thundercloud_directory: AbsolutePath, project_fs: PFS, invar: AbsolutePath, project_root: AbsolutePath) -> impl ThunderConfig;
}
use super::*;

pub fn from_reader<R: Read>(reader: R) -> Result<impl NicheConfig> {
    let config: NicheConfigData = NicheConfig::from_reader(reader)?;
    Ok(config)
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use stringreader::StringReader;
    use crate::config_model::NicheConfig;

    pub fn from_string<S: Into<String>>(body: S) -> anyhow::Result<impl NicheConfig> {
        let config: NicheConfigData = NicheConfig::from_reader(StringReader::new(&body.into()))?;
        Ok(config)
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use super::serde_test_utils::insert_entry;
    use super::OnIncoming::Update;
    use super::WriteMode::Ignore;
    use indoc::indoc;
    use log::debug;
    use serde_yaml::Mapping;
    use std::path::PathBuf;
    use stringreader::StringReader;
    use crate::file_system::fixture_file_system;

    #[test]
    pub fn test_from_reader() -> Result<()> {
        // Given
        let yaml = indoc! {r#"
                ---
                use-thundercloud:
                  directory: "{{PROJECT}}/example-thundercloud"
                  on-incoming: Update
                  features:
                    - glass
                    - bash_config
                    - kermie
                  invar-defaults:
                    write-mode: Ignore
                    interpolate: false
                    props:
                      mathtur: Jeremy
                      buyer: Myra LeJean
                      milk-man: Kaos
            "#};
        debug!("YAML: [{}]", &yaml);
        let yaml_source = StringReader::new(yaml);

        // When
        let niche_config = from_reader(yaml_source)?;

        // Then
        let use_thundercloud = niche_config.use_thundercloud();
        assert_eq!(use_thundercloud.directory(), Some("{{PROJECT}}/example-thundercloud"));
        assert_eq!(use_thundercloud.on_incoming(), &Update);
        assert_eq!(use_thundercloud.features(), &["glass", "bash_config", "kermie"]);

        let invar_defaults = use_thundercloud.invar_defaults().into_owned();
        assert_eq!(invar_defaults.write_mode_option(), Some(Ignore));
        assert_eq!(invar_defaults.interpolate_option(), Some(false));

        let mut mapping = Mapping::new();
        insert_entry(&mut mapping, "mathtur", "Jeremy");
        insert_entry(&mut mapping, "buyer", "Myra LeJean");
        insert_entry(&mut mapping, "milk-man", "Kaos");
        assert_eq!(invar_defaults.props().into_owned(), mapping);

        Ok(())
    }

    #[test]
    pub fn test_new_thunder_config() -> Result<()> {
        // Given
        let yaml_source = StringReader::new(indoc! {r#"
                ---
                use-thundercloud:
                  directory: "{{PROJECT}}/example-thundercloud"
            "#});
        let niche_config = from_reader(yaml_source)?;
        let thunder_cloud_dir = AbsolutePath::try_from("/tmp")?;
        let invar_dir = AbsolutePath::try_from("/var/tmp")?;
        let project_root = AbsolutePath::try_from("/")?;
        let cumulus = AbsolutePath::new(PathBuf::from("cumulus"), &thunder_cloud_dir);
        let fs = fixture_file_system(StringReader::new(""))?;

        // When
        let thunder_config = niche_config.new_thunder_config(fs.clone(), thunder_cloud_dir.clone(), fs.clone(), invar_dir.clone(), project_root.clone());

        // Then
        assert_eq!(thunder_config.use_thundercloud().directory(), niche_config.use_thundercloud().directory());
        assert_eq!(thunder_config.project_root().as_path(), project_root.as_path());
        assert_eq!(thunder_config.thundercloud_directory().as_path(), thunder_cloud_dir.as_path());
        assert_eq!(thunder_config.invar().as_path(), invar_dir.as_path());
        assert_eq!(thunder_config.cumulus().as_path(), cumulus.as_path());
        Ok(())
    }
}
