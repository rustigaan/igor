use crate::file_system::{DirEntry, FileSystem};
use super::{ThunderConfig, UseThundercloudConfig};
use super::use_thundercloud_config_data::UseThundercloudConfigData;
use crate::path::AbsolutePath;

#[derive(Debug)]
pub struct ThunderConfigData<TFS: FileSystem, PFS: FileSystem> {
    use_thundercloud: UseThundercloudConfigData,
    thundercloud_directory: AbsolutePath,
    cumulus: AbsolutePath,
    invar: AbsolutePath,
    project: AbsolutePath,
    thundercloud_file_system: TFS,
    project_file_system: PFS,
}

impl<TFS: FileSystem, PFS: FileSystem> ThunderConfigData<TFS, PFS> {
    pub fn new(use_thundercloud: UseThundercloudConfigData, thundercloud_directory: AbsolutePath, invar: AbsolutePath, project: AbsolutePath, thundercloud_file_system: &TFS, project_file_system: &PFS) -> Self {
        let mut cumulus = thundercloud_directory.clone();
        cumulus.push("cumulus");
        ThunderConfigData {
            use_thundercloud,
            thundercloud_directory,
            cumulus,
            invar,
            project,
            thundercloud_file_system: thundercloud_file_system.clone(),
            project_file_system: project_file_system.clone(),
        }
    }
}

impl<TFS: FileSystem, PFS: FileSystem> ThunderConfig for ThunderConfigData<TFS, PFS> {

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

    fn thundercloud_file_system(&self) -> &impl FileSystem<DirEntryItem=impl DirEntry> {
        &self.thundercloud_file_system
    }

    fn project_file_system(&self) -> &impl FileSystem<DirEntryItem=impl DirEntry> {
        &self.project_file_system
    }
}
