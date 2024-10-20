use anyhow::Result;
use log::{debug, info};
use toml::{Table, Value};
use crate::config_model::{InvarConfig, UseThundercloudConfig};
use crate::file_system::FileSystem;
use crate::{interpolate, NicheName};
use crate::thundercloud;
use crate::path::{AbsolutePath, RelativePath};

pub async fn process_niche<UT: UseThundercloudConfig, FS: FileSystem, IC: InvarConfig>(project_root: AbsolutePath, niches_directory: RelativePath, niche: NicheName, use_thundercloud: UT, invar_config_default: IC, fs: FS) -> Result<()> {
    if let Some(directory) = use_thundercloud.directory() {
        info!("Directory: {directory:?}");

        let work_area = AbsolutePath::new("..", &project_root);
        let absolute_niches_directory = AbsolutePath::new(niches_directory.as_path(), &project_root);
        let niche_directory = AbsolutePath::new(niche.to_str(), &absolute_niches_directory);

        let mut substitutions = Table::new();
        substitutions.insert("WORKSPACE".to_string(), Value::String(work_area.to_string_lossy().to_string()));
        substitutions.insert("PROJECT".to_string(), Value::String(project_root.to_string_lossy().to_string()));
        let directory = interpolate::interpolate(directory, &substitutions);

        let current_dir = AbsolutePath::current_dir()?;
        let thundercloud_directory = AbsolutePath::new(directory.to_string(), &current_dir);

        let mut invar = niche_directory.clone();
        invar.push("invar");
        let thunder_config = use_thundercloud.new_thunder_config(
            invar_config_default,
            fs.clone().read_only(),
            thundercloud_directory,
            fs,
            invar,
            project_root,
        );
        debug!("Thunder_config: {thunder_config:?}");

        thundercloud::process_niche(thunder_config).await?;
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use log::trace;
    use test_log::test;
    use crate::config_model::{invar_config, project_config, NicheTriggers, ProjectConfig, PsychotropicConfig};
    use crate::file_system::{fixture, source_file_to_string, FileSystem};
    use crate::file_system::ConfigFormat::TOML;
    use crate::path::test_utils::to_absolute_path;
    use super::*;

    #[test(tokio::test)]
    async fn test() -> Result<()> {
        // Given
        let fs = create_file_system_fixture()?;

        let project_root = to_absolute_path("/");
        let cargo_cult_toml = fs.open_source(AbsolutePath::new("CargoCult.toml", &project_root)).await?;
        let cargo_cult_toml_data = source_file_to_string(cargo_cult_toml).await?;
        let project_config = project_config::from_str(&cargo_cult_toml_data, TOML)?;
        let niche = NicheName::new("example");
        let psychotropic = project_config.psychotropic()?;
        let use_thundercloud = psychotropic
            .get(niche.to_str())
            .map(NicheTriggers::use_thundercloud).flatten()
            .unwrap();
        let niches_directory = RelativePath::from("yeth-marthter");
        let default_invar_config = invar_config::from_str("", TOML)?;

        // When
        process_niche(project_root, niches_directory, niche, use_thundercloud.clone(), default_invar_config, fs.clone()).await?;

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
            "CargoCult.toml" = """
            [[psychotropic.cues]]
            name = "example"

            [psychotropic.cues.use-thundercloud]
            directory = "{{PROJECT}}/example-thundercloud"
            features = ["glass"]
            """

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