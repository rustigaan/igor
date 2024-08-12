use super::{ThunderConfig, UseThundercloudConfig};
use super::use_thundercloud_config_data::UseThundercloudConfigData;
use crate::path::AbsolutePath;

#[derive(Debug)]
pub struct ThunderConfigData {
    use_thundercloud: UseThundercloudConfigData,
    thundercloud_directory: AbsolutePath,
    cumulus: AbsolutePath,
    invar: AbsolutePath,
    project: AbsolutePath,
}

impl ThunderConfigData {
    pub fn new(use_thundercloud: UseThundercloudConfigData, thundercloud_directory: AbsolutePath, invar: AbsolutePath, project: AbsolutePath) -> Self {
        let mut cumulus = thundercloud_directory.clone();
        cumulus.push("cumulus");
        ThunderConfigData {
            use_thundercloud,
            thundercloud_directory,
            cumulus,
            invar,
            project,
        }
    }
}

impl ThunderConfig for ThunderConfigData {

    fn use_thundercloud(&self) -> &impl UseThundercloudConfig {
        &self.use_thundercloud
    }

    fn thundercloud_directory(&self) -> &AbsolutePath {
        &self.thundercloud_directory
    }

    fn cumulus(&self) -> &AbsolutePath {
        &self.cumulus
    }

    fn invar(&self) -> &AbsolutePath {
        &self.invar
    }

    fn project_root(&self) -> &AbsolutePath {
        &self.project
    }
}
