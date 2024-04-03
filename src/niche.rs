use std::path::{Path,PathBuf};
use ahash::AHashMap;
use log::{debug, info};
use tokio::fs::DirEntry;
use crate::interpolate;
use crate::thundercloud;
use crate::config_model::*;

pub async fn process_niche(project_root: impl AsRef<Path>, niches_directory: impl AsRef<Path>, entry: DirEntry) -> anyhow::Result<()> {
    let mut work_area = project_root.as_ref().to_owned();
    work_area.push("..");
    let mut niche_directory = niches_directory.as_ref().to_owned();
    niche_directory.push(entry.file_name());
    let config = get_config(&niche_directory)?;
    if let Some(directory) = config.use_thundercloud().directory() {
        info!("Directory: {directory:?}");

        let mut substitutions = AHashMap::new();
        substitutions.insert("WORKSPACE".to_string(), work_area.to_string_lossy().to_string());
        substitutions.insert("PROJECT".to_string(), project_root.as_ref().to_string_lossy().to_string());
        let directory = interpolate::interpolate(directory, substitutions);

        let thundercloud_directory = PathBuf::from(directory.into_owned());

        let mut cumulus = thundercloud_directory.clone();
        cumulus.push("cumulus");
        let mut invar = niche_directory.clone();
        invar.push("invar");
        let thunder_config = ThunderConfig::new(
            config.use_thundercloud().clone(),
            thundercloud_directory.clone(),
            invar,
            project_root.as_ref().to_owned()
        );
        debug!("Thunder_config: {thunder_config:?}");

        thundercloud::process_niche(thunder_config).await?;
    }

    Ok(())
}

fn get_config(niche_directory: impl AsRef<Path>) -> anyhow::Result<NicheConfig> {
    let mut config_path = niche_directory.as_ref().to_owned();
    config_path.push("thettingth.yaml");
    info!("Config path: {config_path:?}");

    let file = std::fs::File::open(config_path)?;
    let config: NicheConfig = serde_yaml::from_reader(file)?;
    debug!("Niche configuration: {config:?}");
    let use_thundercloud = config.use_thundercloud();
    debug!("Niche simplified: {:?}: {:?}: {:?}", use_thundercloud.on_incoming(), use_thundercloud.options(), use_thundercloud.params());
    Ok(config)
}