use std::fmt::Debug;
use crate::config_model::UseThundercloudConfig;
use crate::path::AbsolutePath;

pub trait ThunderConfig : Debug + Send + Sync {
    fn use_thundercloud(&self) -> &impl UseThundercloudConfig;
    fn thundercloud_directory(&self) -> &AbsolutePath;
    fn cumulus(&self) -> &AbsolutePath;
    fn invar(&self) -> &AbsolutePath;
    fn project_root(&self) -> &AbsolutePath;
}
