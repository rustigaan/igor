use std::path::{Path, PathBuf};
use ahash::AHashMap;
use log::{debug, info};
use serde::Deserialize;
use tokio::fs::DirEntry;
use crate::interpolate;
use crate::thundercloud;

#[derive(Deserialize,Debug)]
pub enum OnIncoming {
    Update,
    Ignore,
    Warn,
    Fail
}

#[derive(Deserialize,Debug)]
pub struct NicheConfig {
    thundercloud: ThundercloudConfig,
}

#[allow(dead_code)]
#[derive(Deserialize,Debug)]
pub struct ThundercloudConfig {
    directory: Option<String>,
    #[serde(rename = "git-remote")]
    git_remote: Option<GitRemoteConfig>,
}

#[allow(dead_code)]
#[derive(Deserialize,Debug)]
pub struct GitRemoteConfig {
    #[serde(rename = "fetch-url")]
    fetch_url: String,
    revision: String,
    #[serde(rename = "on-incoming")]
    on_incoming: Option<OnIncoming>,
}

pub async fn process_niche(project_root: impl AsRef<Path>, niches_directory: impl AsRef<Path>, entry: DirEntry) -> anyhow::Result<()> {
    let mut work_area = project_root.as_ref().to_owned();
    work_area.push("..");
    let mut niche_directory = niches_directory.as_ref().to_owned();
    niche_directory.push(entry.file_name());
    let config = get_config(&niche_directory)?;
    if let Some(directory) = config.thundercloud.directory {
        info!("Directory: {directory:?}");

        let mut substitutions = AHashMap::new();
        substitutions.insert("WORKSPACE".to_string(), work_area.to_string_lossy().to_string());
        substitutions.insert("PROJECT".to_string(), project_root.as_ref().to_string_lossy().to_string());
        let directory = interpolate::interpolate(&directory, substitutions);

        let thundercloud_directory = PathBuf::from(directory.into_owned());
        thundercloud::process_niche(&thundercloud_directory, &niche_directory).await?;
    }

    Ok(())
}

fn get_config(niche_directory: impl AsRef<Path>) -> anyhow::Result<NicheConfig> {
    let mut config_path = niche_directory.as_ref().to_owned();
    config_path.push("thettingth.yaml");
    info!("Config path: {config_path:?}");

    let file = std::fs::File::open(config_path)?;
    let config = serde_yaml::from_reader(file)?;
    debug!("Niche configuration: {config:?}");
    Ok(config)
}