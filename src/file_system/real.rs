use std::path::Path;
use anyhow::anyhow;
use tokio::fs::DirEntry as TokioDirEntry;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReadDirStream;
use super::*;
use crate::path::AbsolutePath;

#[derive(Debug, Copy, Clone)]
struct RealFileSystem {}

impl DirEntry for TokioDirEntry {
    fn path(&self) -> PathBuf {
        self.path()
    }
}

// #[async_trait::async_trait]
impl FileSystem for RealFileSystem {
    type DirEntryItem = TokioDirEntry;

    async fn read_dir(&self, directory: &AbsolutePath) -> Result<impl Stream<Item=Result<Self::DirEntryItem>>> {
        let entries = tokio::fs::read_dir(directory as &Path).await
            .map_err(|e| anyhow!(format!("error reading {:?}: {:?}", &directory, e)))?;
        let directory = directory.clone();
        Ok(ReadDirStream::new(entries).map(move |item| item.map_err(|e| anyhow!(format!("error traversing {:?}: {:?}", &directory, e)))))
    }
}

pub fn real_file_system() -> impl FileSystem<DirEntryItem = impl DirEntry> {
    RealFileSystem{}
}

#[cfg(test)]
mod test {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use assert_fs::TempDir;
    use std::pin::pin;
    use super::*;

    #[tokio::test]
    async fn empty_dir() -> Result<()> {
        // Given
        let tmp_dir = TempDir::new()?;
        let fs = real_file_system();
        let path = AbsolutePath::try_new(tmp_dir.to_path_buf())?;

        // When
        let entries = fs.read_dir(&path).await?;
        let mut entries = pin!(entries);
        let entry = entries.next().await;

        // Then
        assert_eq!(entry.is_none(), true);

        Ok(())
    }

    #[tokio::test]
    async fn non_empty_dir() -> Result<()> {
        // Given
        let tmp_dir = TempDir::new()?;
        let fs = real_file_system();
        let path = AbsolutePath::try_new(tmp_dir.to_path_buf())?;
        let file_path = AbsolutePath::new(PathBuf::from("empty"), &path);
        tokio::fs::File::create(&file_path.as_path()).await?;

        // When
        let entries = fs.read_dir(&path).await?;
        let mut entries = pin!(entries);
        let entry = entries.next().await;

        // Then
        assert_eq!(entry.is_some(), true);
        if let Some(result) = entry {
            let dir_entry = result?;
            let path: PathBuf = dir_entry.path();
            if let Some(last) = path.components().last() {
                assert_eq!(last.as_os_str(), OsStr::from_bytes("empty".as_bytes()));
            }
        }

        Ok(())
    }
}
