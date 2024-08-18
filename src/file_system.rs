#![allow(dead_code)]

use anyhow::Result;
use std::fmt::Debug;
use std::path::PathBuf;
use tokio_stream::Stream;
use crate::path::AbsolutePath;

mod real;
pub use real::real_file_system;

pub trait DirEntry {
    fn path(&self) -> PathBuf;
}

// #[async_trait::async_trait]
pub trait FileSystem: Debug + Send + Sync + Sized + Copy + Clone {
    type DirEntryItem: DirEntry;
    async fn read_dir(&self, directory: &AbsolutePath) -> Result<impl Stream<Item = Result<Self::DirEntryItem>>>;
}