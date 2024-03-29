use log::info;
use std::error::Error;

use igor::application;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    info!("Igor starting");

    application().await?;
    Ok(())
}
