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

pub async fn application() -> Result<()> {
    info!("Igor started");

    let arguments = Arguments::parse();
    let cwd = AbsolutePath::current_dir()?;
    let project_root_path = arguments.project_root.unwrap_or(PathBuf::from("."));
    let project_root = AbsolutePath::new(project_root_path, &cwd);
    let niches_directory= AbsolutePath::new("yeth-marthter", &project_root);
    info!("Niches configuration directory: {niches_directory:?}");

    let fs = file_system::real_file_system();

    let psychotropic_path = AbsolutePath::new("psychotropic.yaml", &niches_directory);
    let psychotropic_config = psychotropic::from_path(&psychotropic_path, &fs).await?;
    info!("Psychotropic configuration: {psychotropic_config:?}");

    let mut niches = fs.read_dir(&niches_directory).await?;
    let mut handles = Vec::new();
    loop {
        let niche = niches.next().await;
        let handle = match niche {
            None => None,
            Some(Ok(entry)) => {
                info!("Niche configuration entry: {entry:?}");
                Some(tokio::spawn(process_niche(project_root.clone(), niches_directory.clone(), entry)))
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
