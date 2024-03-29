use std::path::PathBuf;
use anyhow::Result;
use clap::Parser;
use log::{error, info};

mod interpolate;
mod niche;
mod thundercloud;

use niche::process_niche;

#[derive(Parser,Debug)]
#[command(version, about, long_about = None)]
struct Arguments {
    /// Location of configuration file
    #[arg(short, long, value_name = "DIRECTORY")]
    niches: Option<PathBuf>,
}

pub async fn application() -> Result<()> {
    info!("Igor started");

    let arguments = Arguments::parse();
    let niches_directory = arguments.niches.unwrap_or(PathBuf::from("yeth-mathtur"));
    info!("Niche configuration directory: {niches_directory:?}");
    let mut niches = tokio::fs::read_dir(&niches_directory).await?;
    let mut handles = Vec::new();
    loop {
        let niche = niches.next_entry().await;
        let handle = match niche {
            Ok(None) => None,
            Ok(Some(entry)) => {
                info!("Niche configuration entry: {entry:?}");
                Some(tokio::spawn(process_niche(niches_directory.clone(), entry)))
            }
            Err(err) => {
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
