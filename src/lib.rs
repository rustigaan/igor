use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Arc;
use ahash::AHashMap;
use anyhow::Result;
use clap::Parser;
use log::{debug, info, warn};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio_stream::StreamExt;

mod config_model;
mod file_system;
mod interpolate;
mod niche;
mod path;
mod thundercloud;

use crate::config_model::psychotropic;
use crate::config_model::psychotropic::{NicheTriggers,PsychotropicConfig};
use crate::file_system::{ConfigFormat, DirEntry, FileSystem, PathType};
use crate::niche::process_niche;
use crate::path::AbsolutePath;

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

enum NicheStatus {
    Run(AbsolutePath),
    AllScheduled(usize),
}

pub async fn application<FS: FileSystem + 'static>(project_root_option: Option<PathBuf>, fs: &FS) -> Result<()> {
    let cwd = AbsolutePath::current_dir()?;
    let project_root_path = project_root_option.unwrap_or(PathBuf::from("."));
    let project_root = AbsolutePath::new(project_root_path, &cwd);
    let niches_directory= AbsolutePath::new("yeth-marthter", &project_root);
    info!("Niches configuration directory: {niches_directory:?}");

    let psychotropic_path = AbsolutePath::new("psychotropic.toml", &niches_directory);
    let psychotropic_config = Arc::new(psychotropic::from_path(&psychotropic_path, ConfigFormat::TOML, fs).await?);
    info!("Psychotropic configuration: {psychotropic_config:?}");

    let mut handles = Vec::new();
    let permits = 5;
    let (tx_work, mut rx_work) = channel(permits);
    let (tx_done, rx_done) = channel(permits);
    let (tx_permit, mut rx_permit) = channel(permits);
    for _ in 1..permits {
        tx_permit.send(()).await?;
    }
    let collector_join_handle = tokio::spawn(collect_done(psychotropic_config.clone(), permits, niches_directory.clone(), rx_done, tx_work.clone(), tx_permit.clone()));
    handles.push(collector_join_handle);
    let emitter_join_handle = tokio::spawn(emit_niches(niches_directory.clone(), fs.clone(), psychotropic_config.clone(), tx_work.clone()));
    handles.push(emitter_join_handle);

    let mut scheduled_count = None;
    let mut started_count: usize = 0;
    while let Some(niche_status) = rx_work.recv().await {
        match niche_status {
            NicheStatus::Run(niche) => {
                debug!("Getting permit for: {:?}", &niche);
                if let None = rx_permit.recv().await {
                    warn!("Received None instead of permit: wrapping up");
                    break;
                }
                debug!("Got permit for: {:?}", &niche);
                let niche_fs = fs.clone();
                let niche_join_handle = tokio::spawn(run_process_niche(project_root.clone(), niche.clone(), niche_fs, tx_done.clone()));
                handles.push(niche_join_handle);
                started_count += 1;
                if scheduled_count.map(|scheduled| started_count >= scheduled).unwrap_or(false) {
                    debug!("All niches were started: wrapping up");
                    break;
                }
            },
            NicheStatus::AllScheduled(scheduled) => {
                debug!("Got all scheduled: {:?}", scheduled);
                scheduled_count = Some(scheduled);
                if started_count >= scheduled {
                    debug!("All niches were started: wrapping up");
                    break;
                }
            }
        };
    }
    drop(rx_work);
    drop(tx_done);

    for handle in handles {
        match handle.await {
            Err(err) => info!("Error in join: {err:?}"),
            Ok(Err(err)) => info!("Error while processing niche: {err:?}"),
            _ => ()
        }
    }

    Ok(())
}

async fn collect_done<PC>(psychotropic_config: Arc<PC>, max_slack: usize, niches_directory: AbsolutePath, mut rx_done: Receiver<AbsolutePath>, tx_work: Sender<NicheStatus>, tx_permit: Sender<()>) -> Result<()>
where PC: PsychotropicConfig
{
    let mut wait_count = AHashMap::new();
    let mut waiting: AHashMap<AbsolutePath, Vec<AbsolutePath>> = AHashMap::new();
    for triggers in psychotropic_config.values() {
        let later = niche_path(triggers.name(), &niches_directory);
        wait_count.insert(later.clone(), triggers.wait_for().len());
        for dep in triggers.wait_for() {
            let dep_path = niche_path(dep, &niches_directory);
            if let Some(existing) = waiting.get_mut(&dep_path) {
                existing.push(later.clone());
            } else {
                let new_list = vec![later.clone()];
                waiting.insert(dep_path.clone(), new_list);
            }
        }
    }

    let mut slack = max_slack;
    let mut ready: Vec<AbsolutePath> = Vec::new();
    while let Some(niche_path) = rx_done.recv().await {
        debug!("Send permit");
        tx_permit.send(()).await?;
        if let Some(later) = ready.pop() {
            debug!("Send work: {:?}", &later);
            tx_work.send(NicheStatus::Run(later.clone())).await?;
            debug!("Work sent: {:?}", &later);
        } else {
            slack += 1;
        }
        debug!("Notify niches waiting for: {:?}", &niche_path);
        if let Some(later_list) = waiting.remove(&niche_path) {
            for later in later_list {
                if let Some(count) = wait_count.get_mut(&later) {
                    if *count == 0 {
                        continue;
                    }
                    if *count == 1 {
                        if slack > 0 {
                            debug!("Send work: {:?}", &later);
                            tx_work.send(NicheStatus::Run(later.clone())).await?;
                            debug!("Work sent: {:?}", &later);
                            slack -= 1;
                        } else {
                            ready.push(later.clone())
                        }
                    }
                    *count -= 1;
                }
            }
        }
        debug!("Get done message");
    }
    debug!("End collect done messages");
    Ok(())
}

async fn emit_niches<FS, PC>(niches_directory: AbsolutePath, fs: FS, psychotropic_config: Arc<PC>, tx: Sender<NicheStatus>) -> Result<()>
where
    FS: FileSystem,
    PC: PsychotropicConfig,
{
    let mut count = 0;
    let mut result = do_emit_independent(&psychotropic_config, &niches_directory, &tx).await;
    if let Ok(independent) = &result {
        count += independent;
        result = do_emit_niches(niches_directory, fs, &psychotropic_config, &tx).await;
        match &result {
            Ok(niches) => {
                count += niches;
            },
            _ => {
                count = 0; // Wrap up as quickly as possible
            }
        }
    }
    debug!("Send all scheduled: {:?}", count);
    tx.send(NicheStatus::AllScheduled(count)).await?;
    debug!("All scheduled sent: {:?}", count);
    result?;
    Ok(())
}

async fn do_emit_independent<PC>(psychotropic_config: &Arc<PC>, niches_directory: &AbsolutePath, tx: &Sender<NicheStatus>) -> Result<usize>
where PC: PsychotropicConfig
{
    let independent = psychotropic_config.independent();
    let mut count = 0;
    for niche in independent {
        let niche_path = AbsolutePath::new(niche, niches_directory);
        debug!("Send independent: {:?}", &niche_path);
        tx.send(NicheStatus::Run(niche_path.clone())).await?;
        debug!("Independent sent: {:?}", &niche_path);
        count += 1;
    }
    for triggers in psychotropic_config.values() {
        if !triggers.wait_for().is_empty() {
            debug!("Count niche that must wait: {:?}", &triggers.name());
            count += 1;
        }
    }
    Ok(count)
}

async fn do_emit_niches<FS, PC>(niches_directory: AbsolutePath, fs: FS, psychotropic_config: &Arc<PC>, tx: &Sender<NicheStatus>) -> Result<usize>
where
    FS: FileSystem,
    PC: PsychotropicConfig,
{
    info!("Emitting niches");
    let mut count = 0;
    let mut entries = fs.read_dir(&niches_directory).await?;
    while let Some(entry_result) = entries.next().await {
        let entry = entry_result?;
        let niche_dir = AbsolutePath::new(entry.file_name(), &niches_directory);
        let settings_file = AbsolutePath::new("igor-thettingth.toml", &niche_dir);
        debug!("Looking for file: {:?}", &settings_file);
        if fs.path_type(&settings_file).await == PathType::File {
            let niche_name = niche_name(&niche_dir);
            if let Some(_) = psychotropic_config.get(&niche_name) {
                debug!("Postpone niche: {:?}", &niche_dir);
                continue;
            }
            debug!("Schedule niche: {:?}", &niche_dir);
            tx.send(NicheStatus::Run(niche_dir.clone())).await?;
            debug!("Niche scheduled: {:?}", &niche_dir);
            count += 1;
        } else {
            debug!("Not a file: {:?}", &settings_file);
        }
    }
    info!("Emitted niches");
    Ok(count)
}

fn niche_name(niche: &AbsolutePath) -> String {
    niche.file_name().map(OsStr::to_string_lossy).map_or_else(String::new, Cow::into_owned)
}

fn niche_path<S: Into<String>>(name: S, niches_directory: &AbsolutePath) -> AbsolutePath {
    AbsolutePath::new(PathBuf::from(name.into()), niches_directory)
}

async fn run_process_niche<FS: FileSystem>(project_root: AbsolutePath, niche: AbsolutePath, niche_fs: FS, tx_done: Sender<AbsolutePath>) -> Result<()> {
    debug!("Processing niche: {:?}", &niche);
    let config_path = niche_path("igor-thettingth.toml", &niche);
    let result = if niche_fs.path_type(&config_path).await == PathType::File {
        process_niche(project_root, niche.clone(), niche_fs).await
    } else {
        warn!("Not found: {:?}", &config_path);
        Ok(())
    };
    debug!("Send done: {:?}", &niche);
    tx_done.send(niche.clone()).await?;
    debug!("Done sent: {:?}", &niche);
    result
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use log::trace;
    use test_log::test;
    use crate::file_system::{fixture, source_file_to_string, FileSystem};
    use crate::path::test_utils::to_absolute_path;
    use super::*;

    #[test(tokio::test)]
    async fn test_application() -> Result<()> {
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

#[cfg(test)]
mod test_utils {
    use anyhow::Result;
    use log::{debug, warn};
    use serde::Serialize;

    pub fn log_toml<T: Serialize>(label: &str,item: &T) -> Result<()> {
        let toml_string = toml::to_string(item)?;
        warn!("YAML is deprecated, use TOML (the debug logging shows the equivalent TOML data)");
        debug!("TOML: {:?}: [[[\n{}\n]]]", label, toml_string);
        Ok(())
    }
}