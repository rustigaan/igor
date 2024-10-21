use std::fmt::Debug;
use ahash::AHashSet;
use anyhow::Result;
use log::debug;
use serde::Deserialize;
use crate::config_model::UseThundercloudConfig;
use super::psychotropic_data::{empty, PsychotropicConfigIndex};
use crate::file_system::{ConfigFormat, FileSystem, PathType};
use crate::path::AbsolutePath;

pub trait NicheTriggers: Clone + Debug {
    type UseThundercloudConfigImpl: UseThundercloudConfig + for<'a> Deserialize<'a>;
    fn name(&self) -> String;
    fn use_thundercloud(&self) -> Option<&Self::UseThundercloudConfigImpl>;
    fn use_thundercloud_path(&self) -> Option<AbsolutePath>;
    fn wait_for(&self) -> &[String];
    fn triggers(&self) -> &[String];
}

pub trait PsychotropicConfig: Debug + Sized + Send {
    type NicheTriggersImpl: NicheTriggers;

    fn independent(&self) -> AHashSet<String>;
    fn get(&self, key: &str) -> Option<&Self::NicheTriggersImpl>;
    fn is_empty(&self) -> bool;
    fn values(&self) -> Vec<Self::NicheTriggersImpl>;
}

pub fn from_str(body: &str, config_format: ConfigFormat) -> Result<impl PsychotropicConfig> {
    PsychotropicConfigIndex::from_str(body, config_format)
}

pub async fn from_path<FS: FileSystem>(source_path: &AbsolutePath, config_format: ConfigFormat, file_system: &FS) -> Result<impl PsychotropicConfig> {
    let source_path_type = file_system.path_type(source_path).await;
    if source_path_type != PathType::File {
        debug!("Source path is not a file: {:?}: {:?}", source_path, source_path_type);
        return Ok(empty())
    }
    let content = file_system.get_content(source_path.clone()).await?;
    PsychotropicConfigIndex::from_str(&content, config_format)
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use log::trace;
    use test_log::test;
    use crate::file_system::{fixture, FileSystem};
    use crate::path::test_utils::to_absolute_path;
    use super::*;

    #[test]
    fn missing_precursor() -> Result<()> {
        // Given
        let toml = indoc! {r#"
            [[cues]]
            name = "non-existent"
            wait-for = ["example"]
        "#};
        trace!("TOML: [{}]", &toml);

        // When
        let result = from_str(&toml, ConfigFormat::TOML)?;

        // Then
        assert_eq!(result.get("example").unwrap().wait_for(), Vec::<&str>::new());
        assert_eq!(result.get("example").unwrap().triggers(), vec!["non-existent"]);
        assert_eq!(result.get("non-existent").unwrap().wait_for(), vec!["example"]);
        assert_eq!(result.get("non-existent").unwrap().triggers(), Vec::<&str>::new());

        Ok(())
    }

    #[test]
    fn assumed_precursor_appears_again() {
        // Given
        let toml = indoc! {r#"
            [[cues]]
            name = "non-existent"
            wait-for = ["example"]

            [[cues]]
            name = "example"
        "#};
        trace!("TOML: [{}]", &toml);

        // When
        let result = from_str(&toml, ConfigFormat::TOML);

        // Then
        assert!(result.is_err(), "An assumed precursor should not appear again");
    }

    #[test(tokio::test)]
    async fn from_source_file() -> Result<()> {
        // Given
        let fs = create_file_system_fixture()?;
        let path = to_absolute_path("/yeth-marthter/psychotropic.toml");

        // When
        let result = from_path(&path, ConfigFormat::TOML, &fs).await?;

        // Then
        assert_eq!(result.get("default-settings").unwrap().wait_for(), Vec::<&str>::new());
        assert_eq!(result.get("default-settings").unwrap().triggers(), Vec::<&str>::new());
        assert_eq!(result.get("example").unwrap().wait_for(), Vec::<&str>::new());
        assert_eq!(result.get("example").unwrap().triggers(), vec!["non-existent"]);
        assert_eq!(result.get("non-existent").unwrap().wait_for(), vec!["example"]);
        assert_eq!(result.get("non-existent").unwrap().triggers(), Vec::<&str>::new());

        Ok(())
    }

    #[test(tokio::test)]
    async fn from_directory() -> Result<()> {
        // Given
        let fs = create_file_system_fixture()?;
        let path = to_absolute_path("/yeth-marthter");

        // When
        let result = from_path(&path, ConfigFormat::TOML, &fs).await?;

        // Then
        assert!(result.is_empty());

        Ok(())
    }

    fn create_file_system_fixture() -> Result<impl FileSystem> {
        let toml_data = indoc! {r#"
            [yeth-marthter]
            "psychotropic.toml" = '''
            [[cues]]
            name = "default-settings"

            [[cues]]
            name = "example"

            [[cues]]
            name = "non-existent"
            wait-for = ["example"]
            '''
        "#};
        trace!("TOML: [{}]", &toml_data);

        Ok(fixture::from_toml(toml_data)?)
    }
}