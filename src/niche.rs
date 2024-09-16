use anyhow::Result;
use ahash::AHashMap;
use log::{debug, info};
use crate::config_model::{niche_config, NicheConfig, UseThundercloudConfig};
use crate::file_system::{source_file_to_string, ConfigFormat, FileSystem};
use crate::interpolate;
use crate::thundercloud;
use crate::path::AbsolutePath;

pub async fn process_niche<FS: FileSystem>(project_root: AbsolutePath, niche_directory: AbsolutePath, fs: FS) -> Result<()> {
    let work_area = AbsolutePath::new("..", &project_root);
    let config = get_config(&niche_directory, &fs).await?;
    if let Some(directory) = config.use_thundercloud().directory() {
        info!("Directory: {directory:?}");

        let mut substitutions = AHashMap::new();
        substitutions.insert("WORKSPACE".to_string(), work_area.to_string_lossy().to_string());
        substitutions.insert("PROJECT".to_string(), project_root.to_string_lossy().to_string());
        let directory = interpolate::interpolate(directory, substitutions);

        let current_dir = AbsolutePath::current_dir()?;
        let thundercloud_directory = AbsolutePath::new(directory.to_string(), &current_dir);

        let mut invar = niche_directory.clone();
        invar.push("invar");
        let thunder_config = config.new_thunder_config(
            fs.clone().read_only(),
            thundercloud_directory,
            fs,
            invar,
            project_root
        );
        debug!("Thunder_config: {thunder_config:?}");

        thundercloud::process_niche(thunder_config).await?;
    }

    Ok(())
}

async fn get_config<FS: FileSystem>(niche_directory: &AbsolutePath, fs: &FS) -> Result<impl NicheConfig> {
    let config_path = AbsolutePath::new("igor-thettingth.yaml", niche_directory);
    info!("Config path: {config_path:?}");

    let source_file = fs.open_source(config_path).await?;
    let body = source_file_to_string(source_file).await?;
    let config = niche_config::from_str(&body, ConfigFormat::YAML)?;
    debug!("Niche configuration: {config:?}");
    let use_thundercloud = config.use_thundercloud();
    debug!("Niche simplified: {:?}: {:?}", use_thundercloud.on_incoming(), use_thundercloud.features());
    Ok(config)
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use log::trace;
    use test_log::test;
    use crate::file_system::{fixture, FileSystem};
    use crate::path::test_utils::to_absolute_path;
    use super::*;

    #[test(tokio::test)]
    async fn test() -> Result<()> {
        // Given
        let fs = create_file_system_fixture()?;

        let project_root = to_absolute_path("/");
        let niche = to_absolute_path("/yeth-marthter/example");

        // When
        process_niche(project_root, niche, fs.clone()).await?;

        // Then
        let source_file = fs.open_source(to_absolute_path("/workshop/clock.yaml")).await?;
        let body = source_file_to_string(source_file).await?;
        let expected = indoc! {r#"
            ---
            raising:
              - "steam"
              - "money"
        "#};
        assert_eq!(&body, expected);

        Ok(())
    }

    fn create_file_system_fixture() -> Result<impl FileSystem> {
        let toml_data = indoc! {r#"
            [yeth-marthter.example]
            "igor-thettingth.yaml" = '''
            ---
            use-thundercloud:
              directory: "{{PROJECT}}/example-thundercloud"
              features:
                - glass
            '''

            [yeth-marthter.example.invar.workshop]
            "clock+config-glass.yaml" = """
            write-mode: Overwrite
            props:
              sweeper: Lu Tse
            """

            [example-thundercloud]
            "thundercloud.yaml" = """
            ---
            niche:
              name: example
              description: Example thundercloud for demonstration purposes
            """

            [example-thundercloud.cumulus.workshop]
            "clock+option-glass.yaml" = '''
            ---
            raising:
              - "steam"
              - "money"
            '''
        "#};
        trace!("TOML: [{}]", &toml_data);
        Ok(fixture::from_toml(toml_data)?)
    }
}