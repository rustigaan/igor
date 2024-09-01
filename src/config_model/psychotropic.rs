use std::fmt::Debug;
use std::io::Read;
use anyhow::Result;
use log::debug;
use stringreader::StringReader;
use super::psychotropic_data::{data_to_index, empty, PsychotropicConfigData, PsychotropicConfigIndex};
use crate::file_system::{source_file_to_string, FileSystem, PathType};
use crate::path::AbsolutePath;

pub trait NicheCue: Debug {
    fn name(&self) -> String;
    fn wait_for(&self) -> &[String];
}

pub trait PsychotropicConfig: Debug + Sized {
    type NicheCueImpl: NicheCue;

    fn get(&self, key: &str) -> Option<&impl NicheCue>;
    fn is_empty(&self) -> bool;
}

pub fn from_reader<R: Read>(reader: R) -> Result<impl PsychotropicConfig> {
    index_from_reader(reader)
}

pub fn index_from_reader<R: Read>(reader: R) -> Result<PsychotropicConfigIndex> {
    let data: PsychotropicConfigData = serde_yaml::from_reader(reader)?;
    data_to_index(data)
}

pub async fn from_path<FS: FileSystem>(source_path: &AbsolutePath, file_system: &FS) -> Result<impl PsychotropicConfig> {
    let source_path_type = file_system.path_type(source_path).await;
    if source_path_type != PathType::File {
        debug!("Source path is not a file: {:?}: {:?}", source_path, source_path_type);
        return Ok(empty())
    }
    let source_file = file_system.open_source(source_path.clone()).await?;
    let body = source_file_to_string(source_file).await?;
    from_string(&body)
}

fn from_string(body: &str) -> Result<PsychotropicConfigIndex> {
    index_from_reader(StringReader::new(body))
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use log::trace;
    use stringreader::StringReader;
    use test_log::test;
    use crate::file_system::{fixture_file_system, FileSystem};
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
        let result = from_string(&yaml)?;

        // Then
        assert_eq!(result.get("example").unwrap().wait_for(), Vec::<&str>::new());
        assert_eq!(result.get("non-existent").unwrap().wait_for(), vec!["example"]);

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
        let result = from_string(&yaml);

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
        assert_eq!(result.get("example").unwrap().wait_for(), Vec::<&str>::new());
        assert_eq!(result.get("non-existent").unwrap().wait_for(), vec!["example"]);

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
        let yaml = indoc! {r#"
                yeth-marthter:
                    psychotropic.yaml: |
                        cues:
                        - name: "default-settings"
                        - name: "example"
                        - name: "non-existent"
                          wait-for:
                          - "example"
        "#};
        trace!("YAML: [{}]", &yaml);

        let yaml_source = StringReader::new(yaml);
        Ok(fixture_file_system(yaml_source)?)
    }
}