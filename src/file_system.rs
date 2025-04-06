use std::ffi::OsString;
use anyhow::{anyhow, Result};
use std::fmt::Debug;
use std::future::Future;
use std::path::PathBuf;
use tokio::sync::mpsc::Sender;
use tokio_stream::Stream;
use crate::config_model::WriteMode;
use crate::path::AbsolutePath;

mod real;
pub use real::real_file_system;

#[cfg(test)]
pub mod fixture;

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum PathType { Missing, File, Directory, Other }

#[derive(Debug, Copy, Clone)]
pub enum ConfigFormat { TOML, YAML }

#[derive(Debug, Clone)]
struct ReadOnlyFileSystem<FS: FileSystem>(FS);

pub trait DirEntry: Debug + Send + Sync {
    fn path(&self) -> PathBuf;
    fn file_name(&self) -> OsString;
    fn is_dir(&self) -> impl Future<Output = Result<bool>> + Send;
}

pub trait TargetFile: Send + Sync {
    fn write_line<S: Into<String> + Debug + Send>(&self, line: S) -> impl Future<Output = Result<()>> + Send;
    fn close(&mut self) -> impl Future<Output=Result<()>> + Send;
}

pub trait SourceFile: Send + Sync {
    fn next_line(&mut self) -> impl Future<Output = Result<Option<String>>> + Send;
}

pub trait FileSystem: Debug + Send + Sync + Sized + Clone {
    type DirEntryItem: DirEntry;
    fn read_dir(&self, directory: &AbsolutePath) -> impl Future<Output = Result<impl Stream<Item = Result<Self::DirEntryItem>> + Send + Sync + Unpin>> + Send;
    fn path_type(&self, path: &AbsolutePath) -> impl Future<Output = PathType> + Send;
    fn open_target(&self, file_path: AbsolutePath, write_mode: WriteMode, executable: bool) -> impl Future<Output = Result<Option<impl TargetFile>>> + Send;
    fn open_source(&self, file_path: AbsolutePath) -> impl Future<Output = Result<impl SourceFile>> + Send;
    fn get_content(&self, file_path: AbsolutePath) -> impl Future<Output = Result<String>> + Send {
        async {
            let source_file = self.open_source(file_path).await?;
            source_file_to_string(source_file).await
        }
    }
    fn read_only(self) -> impl FileSystem {
        ReadOnlyFileSystem(self)
    }
}

#[allow(dead_code)]
struct DummyTarget;

impl TargetFile for DummyTarget {
    async fn write_line<S: Into<String> + Debug + Send>(&self, _line: S) -> Result<()> {
        Err(anyhow!("Trying to write a line to a dummy target"))
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

impl<FS: FileSystem> FileSystem for ReadOnlyFileSystem<FS> {
    type DirEntryItem = FS::DirEntryItem;

    fn read_dir(&self, directory: &AbsolutePath) -> impl Future<Output=Result<impl Stream<Item=Result<Self::DirEntryItem>> + Send + Sync>> + Send {
        self.0.read_dir(directory)
    }

    async fn path_type(&self, path: &AbsolutePath) -> PathType {
        self.0.path_type(path).await
    }

    async fn open_target(&self, _file_path: AbsolutePath, _write_mode: WriteMode, _executable: bool) -> Result<Option<impl TargetFile>> {
        Ok(None::<DummyTarget>)
    }

    fn open_source(&self, file_path: AbsolutePath) -> impl Future<Output=Result<impl SourceFile>> + Send {
        self.0.open_source(file_path)
    }

    fn read_only(self) -> impl FileSystem {
        self
    }
}

pub async fn source_file_to_string<SF: SourceFile>(mut source_file: SF) -> Result<String> {
    let mut lines = Vec::new();
    while let Some(line) = source_file.next_line().await? {
        lines.push(line);
    }
    lines.push("".to_string());
    Ok(lines.join("\n"))
}