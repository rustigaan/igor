use crate::config_model::invar_config_data::InvarConfigData;
use crate::file_system::{DirEntry, FileSystem};
use super::{InvarConfig, InvarState, ThunderConfig, UseThundercloudConfig};
use super::use_thundercloud_config_data::UseThundercloudConfigData;
use crate::path::AbsolutePath;

#[derive(Clone, Debug)]
pub struct ThunderConfigData<TFS: FileSystem, PFS: FileSystem> {
    use_thundercloud: UseThundercloudConfigData,
    default_invar_config: InvarConfigData,
    thundercloud_directory: AbsolutePath,
    cumulus: AbsolutePath,
    invar: AbsolutePath,
    project: AbsolutePath,
    thundercloud_file_system: TFS,
    project_file_system: PFS,
}

impl<TFS: FileSystem, PFS: FileSystem> ThunderConfigData<TFS, PFS> {
    pub fn new<IC: InvarConfig>(use_thundercloud: UseThundercloudConfigData, default_invar_config: IC, thundercloud_directory: AbsolutePath, invar: AbsolutePath, project: AbsolutePath, thundercloud_file_system: TFS, project_file_system: PFS) -> Self {
        let invar_state = default_invar_config.clone_state();
        let invar_defaults = use_thundercloud.invar_defaults();
        let invar_state = invar_state
            .with_invar_state(invar_defaults.clone_state());
        let target = invar_defaults.target()
            .or(default_invar_config.target());
        let default_invar_config = InvarConfigData::new(invar_state.as_ref(), target);
        let mut cumulus = thundercloud_directory.clone();
        cumulus.push("cumulus");
        ThunderConfigData {
            use_thundercloud,
            default_invar_config,
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

    fn default_invar_config(&self) -> &impl InvarConfig {
        &self.default_invar_config
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

    fn thundercloud_file_system(&self) -> impl FileSystem<DirEntryItem=impl DirEntry> {
        self.thundercloud_file_system.clone()
    }

    fn project_file_system(&self) -> impl FileSystem<DirEntryItem=impl DirEntry> {
        self.project_file_system.clone()
    }
}
