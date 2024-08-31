use anyhow::Result;
use ahash::AHashMap;
use log::{debug, info};
use crate::config_model::{niche_config, NicheConfig, UseThundercloudConfig};
use crate::file_system;
use crate::file_system::{source_file_to_string, DirEntry, FileSystem};
use crate::interpolate;
use crate::thundercloud;
use crate::path::AbsolutePath;

pub async fn process_niche<DE: DirEntry>(project_root: AbsolutePath, niches_directory: AbsolutePath, entry: DE) -> Result<()> {
    let work_area = AbsolutePath::new("..", &project_root);
    let niche_directory = AbsolutePath::new(entry.file_name(), &niches_directory);
    let fs = file_system::real_file_system();
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
    let config = niche_config::from_string(body)?;
    debug!("Niche configuration: {config:?}");
    let use_thundercloud = config.use_thundercloud();
    debug!("Niche simplified: {:?}: {:?}", use_thundercloud.on_incoming(), use_thundercloud.features());
    Ok(config)
}