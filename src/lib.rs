use std::path::PathBuf;
use anyhow::Result;
use clap::Parser;
use log::{error, info};
use tokio_stream::StreamExt;

mod config_model;
mod file_system;
mod interpolate;
mod niche;
mod path;
mod thundercloud;

use niche::process_niche;
use crate::file_system::FileSystem;
use crate::path::AbsolutePath;
use crate::config_model::psychotropic;

#[derive(Parser,Debug)]
#[command(version, about, long_about = None)]
struct Arguments {
    /// Location of the project root (this is where the thunderbolts hit)
    #[arg(short, long)]
    project_root: Option<PathBuf>,

    /// Location of the directory that specifies the niches to fill (default: PROJECT_ROOT/yeth-marthter)
    #[arg(short, long, value_name = "DIRECTORY")]
    niches: Option<PathBuf>,
}

pub async fn igor() -> Result<()> {
    info!("Igor started");
    let arguments = Arguments::parse();

    let fs = file_system::real_file_system();
    application(arguments.project_root, &fs).await
}

pub async fn application<FS: FileSystem + 'static>(project_root_option: Option<PathBuf>, fs: &FS) -> Result<()> {
    let cwd = AbsolutePath::current_dir()?;
    let project_root_path = project_root_option.unwrap_or(PathBuf::from("."));
    let project_root = AbsolutePath::new(project_root_path, &cwd);
    let niches_directory= AbsolutePath::new("yeth-marthter", &project_root);
    info!("Niches configuration directory: {niches_directory:?}");

    let psychotropic_path = AbsolutePath::new("psychotropic.yaml", &niches_directory);
    let psychotropic_config = psychotropic::from_path(&psychotropic_path, fs).await?;
    info!("Psychotropic configuration: {psychotropic_config:?}");

    let mut niches = fs.read_dir(&niches_directory).await?;
    let mut handles = Vec::new();
    loop {
        let niche = niches.next().await;
        let handle = match niche {
            None => None,
            Some(Ok(entry)) => {
                info!("Niche configuration entry: {entry:?}");
                let niche_fs = fs.clone();
                Some(tokio::spawn(process_niche(project_root.clone(), niches_directory.clone(), entry, niche_fs)))
            }
            Some(Err(err)) => {
                error!("Error while reading niche directory entry: {err:?}");
                None
            }
        };
        let Some(handle) = handle else {
            break;
        };
        handles.push(handle);
    }
    for handle in handles {
        match handle.await {
            Err(err) => info!("Error in join: {err:?}"),
            Ok(Err(err)) => info!("Error while processing niche: {err:?}"),
            _ => ()
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use log::trace;
    use stringreader::StringReader;
    use test_log::test;
    use crate::file_system::{fixture_file_system, source_file_to_string, FileSystem};
    use crate::path::test_utils::to_absolute_path;
    use super::*;

    #[test(tokio::test)]
    async fn test() -> Result<()> {
        // Given
        let fs = create_file_system_fixture()?;

        // When
        application(Some(PathBuf::from("/")), &fs).await?;

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
        let yaml = indoc! {r#"
                example-thundercloud:
                    thundercloud.yaml: |
                        ---
                        niche:
                          name: example
                          description: Example thundercloud for demonstration purposes
                    cumulus:
                        workshop:
                            clock+option-glass.yaml: |
                                ---
                                raising:
                                  - "steam"
                                  - "money"
                yeth-marthter:
                    psychotropic.yaml: |
                        cues:
                        - name: "default-settings"
                        - name: "example"
                        - name: "non-existent"
                          wait-for:
                          - "example"
                    example:
                        igor-thettingth.yaml: |
                            ---
                            use-thundercloud:
                              directory: "{{PROJECT}}/example-thundercloud"
                              features:
                                - glass
                        invar:
                            workshop:
                                clock+config-glass.yaml: |
                                    write-mode: Overwrite
                                    props:
                                      sweeper: Lu Tse
            "#};
        trace!("YAML: [{}]", &yaml);

        let yaml_source = StringReader::new(yaml);
        Ok(fixture_file_system(yaml_source)?)
    }
}
