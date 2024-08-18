use std::fmt::Debug;
use crate::config_model::UseThundercloudConfig;
use crate::file_system::{DirEntry,FileSystem};
use crate::path::AbsolutePath;

pub trait ThunderConfig : Debug + Send + Sync {
    fn use_thundercloud(&self) -> &impl UseThundercloudConfig;
    fn thundercloud_directory(&self) -> &AbsolutePath;
    fn cumulus(&self) -> &AbsolutePath;
    fn invar(&self) -> &AbsolutePath;
    fn project_root(&self) -> &AbsolutePath;
    fn thundercloud_file_system(&self) -> &impl FileSystem<DirEntryItem=impl DirEntry>;
    fn project_file_system(&self) -> &impl FileSystem<DirEntryItem=impl DirEntry>;
}

#[cfg(test)]
mod test {
    use anyhow::Result;

    #[test]
    fn test_new_thunder_config() -> Result<()> {
        super::super::niche_config::test::test_new_thunder_config()
    }
}
