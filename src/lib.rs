use std::path::PathBuf;
use std::sync::Arc;
use ahash::AHashMap;
use anyhow::Result;
use clap::Parser;
use log::{debug, error, info, warn};
use tokio::sync::mpsc::{channel, Receiver, Sender};

mod config_model;
mod file_system;
mod interpolate;
mod niche;
mod path;
mod thundercloud;

use crate::config_model::{project_config, NicheTriggers, PsychotropicConfig, UseThundercloudConfig};
use crate::file_system::{ConfigFormat, FileSystem, PathType};
use crate::niche::process_niche;
use crate::path::AbsolutePath;
use crate::config_model::project_config::ProjectConfig;

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

#[derive(Clone,Debug,Hash,PartialEq,Eq)]
struct NicheName(String);

impl NicheName {
    fn new<S: Into<String>>(name: S) -> Self {
        NicheName(name.into())
    }
    #[allow(dead_code)]
    fn to_string(&self) -> String {
        self.0.clone()
    }
    fn to_str(&self) -> &str {
        &self.0
    }
}

enum NicheStatus {
    Run(NicheName),
    AllScheduled(usize),
}

pub async fn application<FS: FileSystem + 'static>(project_root_option: Option<PathBuf>, fs: &FS) -> Result<()> {
    let cwd = AbsolutePath::current_dir()?;
    let project_root_path = project_root_option.unwrap_or(PathBuf::from("."));
    let project_root = AbsolutePath::new(project_root_path, &cwd);

    let project_config_path = AbsolutePath::new("CargoCult.toml", &project_root);
    let project_config_data = if fs.path_type(&project_config_path).await == PathType::File {
        fs.get_content(project_config_path).await?
    } else {
        "".to_string()
    };
    let project_configuration = project_config::from_str(&project_config_data, ConfigFormat::TOML)?;

    let niches_directory= AbsolutePath::new(project_configuration.niches_directory().as_path(), &project_root);
    info!("Niches configuration directory: {niches_directory:?}");

    let project_config = Arc::new(project_configuration);
    info!("Project configuration: {project_config:?}");

    let mut handles = Vec::new();
    let permits = 5;
    let (tx_work, mut rx_work) = channel(permits);
    let (tx_done, rx_done) = channel(permits);
    let (tx_permit, mut rx_permit) = channel(permits);
    for _ in 1..permits {
        tx_permit.send(()).await?;
    }
    let collector_join_handle = tokio::spawn(collect_done(project_config.clone(), permits, rx_done, tx_work.clone(), tx_permit.clone()));
    handles.push(collector_join_handle);
    let emitter_join_handle = tokio::spawn(emit_niches(project_config.clone(), tx_work.clone()));
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
                let niche_join_handle = tokio::spawn(run_process_niche(project_root.clone(), niche.clone(), niche_fs, project_config.clone(), tx_done.clone()));
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

async fn collect_done<PC>(project_config: Arc<PC>, max_slack: usize, mut rx_done: Receiver<NicheName>, tx_work: Sender<NicheStatus>, tx_permit: Sender<()>) -> Result<()>
where PC: ProjectConfig
{
    let psychotropic_config = project_config.psychotropic()?;
    let mut wait_count = AHashMap::new();
    let mut waiting: AHashMap<NicheName, Vec<NicheName>> = AHashMap::new();
    for triggers in psychotropic_config.values() {
        let later = NicheName::new(triggers.name());
        wait_count.insert(later.clone(), triggers.wait_for().len());
        for dep in triggers.wait_for() {
            let dep_name = NicheName::new(dep);
            if let Some(existing) = waiting.get_mut(&dep_name) {
                existing.push(later.clone());
            } else {
                let new_list = vec![later.clone()];
                waiting.insert(dep_name.clone(), new_list);
            }
        }
    }

    let mut slack = max_slack;
    let mut ready: Vec<NicheName> = Vec::new();
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

async fn emit_niches<PC>(project_config: Arc<PC>, tx: Sender<NicheStatus>) -> Result<()>
where
    PC: ProjectConfig,
{
    let mut count = 0;
    let result = do_emit_independent(&project_config, &tx).await;
    if let Ok(independent) = &result {
        count += independent;
    } else {
        error!("Error while emitting independent niches: {:?}", result);
    }
    debug!("Send all scheduled: {:?}", count);
    tx.send(NicheStatus::AllScheduled(count)).await?;
    debug!("All scheduled sent: {:?}", count);
    result?;
    Ok(())
}

async fn do_emit_independent<PC>(project_config: &Arc<PC>, tx: &Sender<NicheStatus>) -> Result<usize>
where PC: ProjectConfig
{
    let psychotropic_config = project_config.psychotropic()?;
    let independent = psychotropic_config.independent();
    let mut count = 0;
    for niche in independent {
        debug!("Send independent: {:?}", &niche);
        tx.send(NicheStatus::Run(NicheName::new(&niche))).await?;
        debug!("Independent sent: {:?}", &niche);
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

async fn run_process_niche<FS: FileSystem, PC: ProjectConfig>(project_root: AbsolutePath, niche: NicheName, niche_fs: FS, project_config: Arc<PC>, tx_done: Sender<NicheName>) -> Result<()> {
    debug!("Processing niche: {:?}", &niche);
    let psychotropic = project_config.psychotropic()?;
    let result = if let Some(use_thundercloud) = get_use_thundercloud_option(&niche, &niche_fs, &psychotropic).await? {
        let niches_directory = project_config.niches_directory();
        process_niche(project_root, niches_directory, niche.clone(), use_thundercloud.clone(), project_config.invar_defaults().into_owned(), niche_fs).await
    } else {
        warn!("Niche not found: {:?}", &niche);
        Ok(())
    };
    debug!("Send done: {:?}", &niche);
    tx_done.send(niche.clone()).await?;
    debug!("Done sent: {:?}", &niche);
    result
}

async fn get_use_thundercloud_option<FS: FileSystem, PC: PsychotropicConfig>(niche: &NicheName, niche_fs: &FS, psychotropic: &PC) -> Result<Option<impl UseThundercloudConfig>> {
    let niche_triggers = psychotropic
        .get(niche.to_str());
    let use_thundercloud_inline_option = niche_triggers
        .map(NicheTriggers::use_thundercloud).flatten().map(Clone::clone);
    if use_thundercloud_inline_option.is_some() {
        Ok(use_thundercloud_inline_option)
    } else if let Some(path) = niche_triggers.map(NicheTriggers::use_thundercloud_path).flatten() {
        let content = niche_fs.get_content(path).await?;
        Ok(Some(toml::from_str(&content)?))
    } else {
        Ok(None)
    }
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
    async fn test_application() -> Result<()> {
        // Given
        let fs = create_file_system_fixture()?;

        // When
        application(Some(PathBuf::from("/")), &fs).await?;

        // Then
        let content = fs.get_content(to_absolute_path("/workshop/clock.yaml")).await?;
        let expected = indoc! {r#"
            ---
            raising:
              - "steam"
              - "money"
        "#};
        assert_eq!(&content, expected);

        Ok(())
    }

    fn create_file_system_fixture() -> Result<impl FileSystem> {
        let toml_data = indoc! {r#"
            "CargoCult.toml" = '''
            niches-directory = "yeth-marthter"

            [psychotropic]

            [[psychotropic.cues]]
            name = "default-settings"

            [[psychotropic.cues]]
            name = "example"
            use-thundercloud = "/yeth-marthter/example/use-thundercloud.toml"

            [[psychotropic.cues]]
            name = "non-existent"
            wait-for = ["example"]
            '''

            [yeth-marthter]

            [yeth-marthter.example]
            "use-thundercloud.toml" = '''
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