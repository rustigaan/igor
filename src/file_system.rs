#![allow(dead_code)]

use std::ffi::OsString;
use anyhow::Result;
use std::fmt::Debug;
use std::future::Future;
use std::path::PathBuf;
use tokio::sync::mpsc::Sender;
use tokio_stream::Stream;
use crate::path::AbsolutePath;

mod real;
pub use real::real_file_system;
use crate::config_model::WriteMode;

pub trait DirEntry: Debug + Send + Sync {
    fn path(&self) -> PathBuf;
    fn file_name(&self) -> OsString;
    fn is_dir(&self) -> impl Future<Output = Result<bool>> + Send;
}

pub trait TargetFile: Send + Sync {
    fn write_line<S: Into<String> + Send>(&self, line: S) -> impl Future<Output = Result<()>> + Send;
    fn close(&mut self) -> impl Future<Output=Result<()>> + Send;
}

pub trait SourceFile: Send + Sync {
    fn next_line(&mut self) -> impl Future<Output = Result<Option<String>>> + Send;
}

pub trait FileSystem: Debug + Send + Sync + Sized + Copy + Clone {
    type DirEntryItem: DirEntry;
    fn read_dir(&self, directory: &AbsolutePath) -> impl Future<Output = Result<impl Stream<Item = Result<Self::DirEntryItem>> + Send + Sync>> + Send;
    fn open_target(&self, file_path: AbsolutePath, write_mode: WriteMode) -> impl Future<Output = Result<Option<impl TargetFile>>> + Send;
    fn open_source(&self, file_path: AbsolutePath) -> impl Future<Output = Result<impl SourceFile>> + Send;
}