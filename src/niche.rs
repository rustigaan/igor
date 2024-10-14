use anyhow::Result;
use log::{debug, info};
use toml::{Table, Value};
use crate::config_model::{niche_config, InvarConfig, NicheConfig, UseThundercloudConfig};
use crate::file_system::{source_file_to_string, ConfigFormat, FileSystem};
use crate::interpolate;
use crate::thundercloud;
use crate::path::AbsolutePath;

pub async fn process_niche<FS: FileSystem, IC: InvarConfig>(project_root: AbsolutePath, niche_directory: AbsolutePath, settings_base: String, invar_config_default: IC, fs: FS) -> Result<()> {
    let work_area = AbsolutePath::new("..", &project_root);
    let config = get_config(&niche_directory, settings_base, &fs).await?;
    if let Some(directory) = config.use_thundercloud().directory() {
        info!("Directory: {directory:?}");

        let mut substitutions = Table::new();
        substitutions.insert("WORKSPACE".to_string(), Value::String(work_area.to_string_lossy().to_string()));
        substitutions.insert("PROJECT".to_string(), Value::String(project_root.to_string_lossy().to_string()));
        let directory = interpolate::interpolate(directory, &substitutions);

        let current_dir = AbsolutePath::current_dir()?;
        let thundercloud_directory = AbsolutePath::new(directory.to_string(), &current_dir);

        let mut invar = niche_directory.clone();
        invar.push("invar");
        let thunder_config = config.new_thunder_config(
            invar_config_default,
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

async fn get_config<FS: FileSystem>(niche_directory: &AbsolutePath, settings_base: String, fs: &FS) -> Result<impl NicheConfig> {
    let config_path = AbsolutePath::new(settings_base + ".toml", niche_directory);
    info!("Config path: {config_path:?}");

    let source_file = fs.open_source(config_path).await?;
    let body = source_file_to_string(source_file).await?;
    let config = niche_config::from_str(&body, ConfigFormat::TOML)?;
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
    use crate::config_model::invar_config;
    use crate::file_system::{fixture, FileSystem};
    use crate::file_system::ConfigFormat::TOML;
    use crate::path::test_utils::to_absolute_path;
    use super::*;

    #[test(tokio::test)]
    async fn test() -> Result<()> {
        // Given
        let fs = create_file_system_fixture()?;

        let project_root = to_absolute_path("/");
        let niche = to_absolute_path("/yeth-marthter/example");
        let default_invar_config = invar_config::from_str("", TOML)?;

        // When
        process_niche(project_root, niche, "igor-thettingth".to_string(), default_invar_config, fs.clone()).await?;

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
            "igor-thettingth.toml" = '''
            [use-thundercloud]
            directory = "{{PROJECT}}/example-thundercloud"
            features = ["glass"]
            '''

            [yeth-marthter.example.invar.workshop]
            "clock+config-glass.yaml.toml" = """
            write-mode = "Overwrite"

            [props]
            sweeper = "Lu Tse"
            """

            [example-thundercloud]
            "thundercloud.toml" = """
            [niche]
            name = "example"
            description = "Example thundercloud for demonstration purposes"
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