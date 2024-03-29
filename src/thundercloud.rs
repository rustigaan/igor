use anyhow::Result;
use std::path::PathBuf;
use log::{debug, info};
use serde::Deserialize;

#[derive(Deserialize,Debug)]
struct ThundercloudConfig {
    niche: NicheConfig,
}

#[derive(Deserialize,Debug)]
struct NicheConfig {
    name: String,
    description: Option<String>,
}

pub async fn process_niche(thundercloud_directory: &PathBuf, niche_directory: &PathBuf) -> Result<()> {
    info!("Apply: {thundercloud_directory:?} -> {niche_directory:?}");
    let config = get_config(thundercloud_directory)?;
    info!("Thundercloud: {:?}: {:?}", config.niche.name, config.niche.description.unwrap_or("-".to_string()));
    Ok(())
}

fn get_config(thundercloud_directory: &PathBuf) -> Result<ThundercloudConfig> {
    let mut config_path = thundercloud_directory.clone();
    config_path.push("thundercloud.yaml");
    info!("Config path: {config_path:?}");

    let file = std::fs::File::open(config_path)?;
    let config = serde_yaml::from_reader(file)?;
    debug!("Thundercloud configuration: {config:?}");
    Ok(config)
}