use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::Path;
use anyhow::{Result,anyhow};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::fs::{DirBuilder, DirEntry as TokioDirEntry, File, OpenOptions};
use tokio::sync::mpsc::{channel, Receiver};
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReadDirStream;
use crate::config_model::WriteMode;
use super::*;
use crate::path::AbsolutePath;

#[derive(Debug, Copy, Clone)]
struct RealFileSystem {}

struct RealTargetFile {
    file_path: AbsolutePath,
    tx: Sender<String>,
    join_handle: Option<JoinHandle<Result<()>>>
}

struct RealSourceFile {
    file_path: AbsolutePath,
    lines: Lines<BufReader<File>>
}

impl DirEntry for TokioDirEntry {
    fn path(&self) -> PathBuf {
        self.path()
    }

    fn file_name(&self) -> OsString {
        self.file_name()
    }

    async fn is_dir(&self) -> Result<bool> {
        let file_type = self.file_type().await?;
        Ok(file_type.is_dir())
    }
}

impl FileSystem for RealFileSystem {
    type DirEntryItem = TokioDirEntry;

    async fn read_dir(&self, directory: &AbsolutePath) -> Result<impl Stream<Item = Result<Self::DirEntryItem>> + Send + Sync> {
        let entries = tokio::fs::read_dir(&directory as &Path).await
            .map_err(|e| anyhow!(format!("error reading {:?}: {:?}", &directory, e)))?;
        Ok(ReadDirStream::new(entries).map(move |item| item.map_err(|e| anyhow!(format!("error traversing {:?}: {:?}", &directory, e)))))
    }

    async fn open_target(&self, target_file: AbsolutePath, write_mode: WriteMode) -> Result<Option<impl TargetFile>> {
        let mut open_options = OpenOptions::new().read(false).write(true).to_owned();
        let open_options = match write_mode {
            WriteMode::Ignore => {
                return Ok(None)
            },
            WriteMode::WriteNew => open_options.create_new(true),
            WriteMode::Overwrite => open_options.create(true).truncate(true),
        };

        let mut target_dir = target_file.to_path_buf();
        target_dir.pop();
        let mut dir_builder = DirBuilder::new();
        dir_builder.recursive(true);
        dir_builder.create(target_dir.as_path()).await?;

        let result = open_options.open(target_file.as_path()).await;
        let file_option = match result {
            Ok(file) => Some(file),
            Err(error) => {
                if let ErrorKind::AlreadyExists = error.kind() {
                    None
                } else {
                    return Err(error.into())
                }
            }
        };
        if let Some(file) = file_option {
            let (tx, rx) = channel(10);
            let join_handle = tokio::task::spawn(file_writer(rx, file));
            Ok(Some(RealTargetFile {
                file_path: target_file,
                tx,
                join_handle: Some(join_handle),
            }))
        } else {
            Ok(None)
        }
    }

    async fn open_source(&self, source_path: AbsolutePath) -> Result<impl SourceFile> {
        let file = File::open(source_path.as_path()).await?;
        let buffered_reader = BufReader::new(file);
        let lines = buffered_reader.lines();
        Ok(RealSourceFile {
            file_path: source_path.clone(),
            lines
        })
    }
}

async fn file_writer(rx: Receiver<String>, mut target: File) -> Result<()> {
    let mut rx = rx;
    while let Some(line) = rx.recv().await {
        target.write_all(line.as_bytes()).await?;
    }
    Ok(())
}

impl TargetFile for RealTargetFile {
    async fn write_line<S: Into<String> + Send>(&self, line: S) -> Result<()> {
        self.tx.send(line.into()).await.map_err(|e| anyhow!(format!("Error wirting line to {:?}: {:?}", &self.file_path, e)))
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(join_handle) = &mut self.join_handle.take() {
            join_handle.await?.map_err(|e| anyhow!(format!("Error closing {:?}: {:?}", &self.file_path, e)))
        } else {
            Err(anyhow!("Closed already: {:?}", &self.file_path))
        }
    }
}

impl SourceFile for RealSourceFile {
    async fn next_line(&mut self) -> Result<Option<String>> {
        self.lines.next_line().await.map_err(|e| anyhow!(format!("Error fetching next line from: {:?}: {:?}", &self.file_path, e)))
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
        File::create(&file_path.as_path()).await?;

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