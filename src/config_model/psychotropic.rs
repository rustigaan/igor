use std::fmt::Debug;
use ahash::AHashSet;
use anyhow::Result;
use log::debug;
use super::psychotropic_data::{empty, PsychotropicConfigIndex};
use crate::file_system::{source_file_to_string, ConfigFormat, FileSystem, PathType};
use crate::path::AbsolutePath;

pub trait NicheTriggers: Debug {
    fn name(&self) -> String;
    fn wait_for(&self) -> &[String];
    fn triggers(&self) -> &[String];
}

pub trait PsychotropicConfig: Debug + Sized {
    type NicheTriggersImpl: NicheTriggers;

    fn independent(&self) -> AHashSet<String>;
    fn get(&self, key: &str) -> Option<&Self::NicheTriggersImpl>;
    fn is_empty(&self) -> bool;
    fn values(&self) -> Vec<Self::NicheTriggersImpl>;
}

pub fn from_str(body: &str, config_format: ConfigFormat) -> Result<impl PsychotropicConfig> {
    PsychotropicConfigIndex::from_str(body, config_format)
}

pub async fn from_path<FS: FileSystem>(source_path: &AbsolutePath, file_system: &FS) -> Result<impl PsychotropicConfig> {
    let source_path_type = file_system.path_type(source_path).await;
    if source_path_type != PathType::File {
        debug!("Source path is not a file: {:?}: {:?}", source_path, source_path_type);
        return Ok(empty())
    }
    let source_file = file_system.open_source(source_path.clone()).await?;
    let body = source_file_to_string(source_file).await?;
    PsychotropicConfigIndex::from_str(&body, ConfigFormat::YAML)
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
        let yaml = indoc! {r#"
            cues:
            - name: "non-existent"
              wait-for:
              - "example"
        "#};
        trace!("YAML: [{}]", &yaml);

        // When
        let result = from_str(&yaml, ConfigFormat::YAML)?;

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
        let yaml = indoc! {r#"
            cues:
            - name: "non-existent"
              wait-for:
              - "example"
            - name: "example"
        "#};
        trace!("YAML: [{}]", &yaml);

        // When
        let result = from_str(&yaml, ConfigFormat::YAML);

        // Then
        assert!(result.is_err(), "An assumed precursor should not appear again");
    }

    #[test(tokio::test)]
    async fn from_source_file() -> Result<()> {
        // Given
        let fs = create_file_system_fixture()?;
        let path = to_absolute_path("/yeth-marthter/psychotropic.yaml");

        // When
        let result = from_path(&path, &fs).await?;

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
        let result = from_path(&path, &fs).await?;

        // Then
        assert!(result.is_empty());

        Ok(())
    }

    fn create_file_system_fixture() -> anyhow::Result<impl FileSystem> {
        let toml_data = indoc! {r#"
            [yeth-marthter]
            "psychotropic.yaml" = '''
            cues:
            - name: "default-settings"
            - name: "example"
            - name: "non-existent"
              wait-for:
              - "example"
            '''
        "#};
        trace!("TOML: [{}]", &toml_data);

        Ok(fixture::from_toml(toml_data)?)
    }
}