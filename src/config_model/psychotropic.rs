use std::ffi::OsString;
use std::fmt::Debug;
use std::io::Read;
use anyhow::Result;
use log::debug;
use stringreader::StringReader;
use super::psychotropic_data::{data_to_index, empty, PsychotropicConfigData, PsychotropicConfigIndex};
use crate::file_system::{source_file_to_string, FileSystem, PathType};
use crate::path::AbsolutePath;

pub trait NicheCue: Debug {
    fn name(&self) -> OsString;
    fn wait_for(&self) -> &[OsString];
}

pub trait PsychotropicConfig: Debug + Sized {
    type NicheCueImpl: NicheCue;

    fn get(&self, key: &OsString) -> Option<&impl NicheCue>;
}

pub fn from_reader<R: Read>(reader: R) -> Result<impl PsychotropicConfig> {
    index_from_reader(reader)
}

pub fn index_from_reader<R: Read>(reader: R) -> Result<PsychotropicConfigIndex> {
    let data: PsychotropicConfigData = serde_yaml::from_reader(reader)?;
    data_to_index(data)
}

pub async fn from_path<FS: FileSystem>(source_path: &AbsolutePath, file_system: &FS) -> Result<impl PsychotropicConfig> {
    let source_path_type = file_system.path_type(source_path).await;
    if source_path_type != PathType::File {
        debug!("Source path is not a file: {:?}: {:?}", source_path, source_path_type);
        return Ok(empty())
    }
    let source_file = file_system.open_source(source_path.clone()).await?;
    let body = source_file_to_string(source_file).await?;
    index_from_reader(StringReader::new(&body))
}
