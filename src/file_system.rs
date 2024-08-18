#![allow(dead_code)]

use std::ffi::OsString;
use anyhow::Result;
use std::fmt::Debug;
use std::future::Future;
use std::path::PathBuf;
use tokio_stream::Stream;
use crate::path::AbsolutePath;

mod real;
pub use real::real_file_system;

pub trait DirEntry: Debug + Send + Sync {
    fn path(&self) -> PathBuf;
    fn file_name(&self) -> OsString;
    fn is_dir(&self) -> impl Future<Output = Result<bool>> + Send;
}

// #[async_trait::async_trait]
pub trait FileSystem: Debug + Send + Sync + Sized + Copy + Clone {
    type DirEntryItem: DirEntry;
    fn read_dir(&self, directory: &AbsolutePath) -> impl Future<Output = Result<impl Stream<Item = Result<Self::DirEntryItem>> + Send + Sync>> + Send;
}