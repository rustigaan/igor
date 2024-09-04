use std::fmt;
use std::io::{BufRead, BufReader, Read};
use std::path::Component;
use std::sync::Arc;
use ahash::AHashMap;
use anyhow::anyhow;
use async_stream::stream;
use log::{debug, trace, warn};
use serde::{Deserialize, Deserializer};
use serde::de::{MapAccess, Visitor};
use stringreader::StringReader;
use tokio::sync::RwLock;
use tokio::sync::mpsc::{Receiver,channel};
use crate::config_model::WriteMode::{Ignore, Overwrite};
use crate::file_system::fixture::FixtureContent::{DirFixtureContent, FileFixtureContent};
use crate::path::AbsolutePath;
use super::*;

#[derive(Debug)]
enum FixtureContent {
    DirFixtureContent { entries: RwLock<AHashMap<OsString, Arc<FixtureEntry>>> },
    FileFixtureContent { lines: RwLock<Vec<String>>},
}

#[derive(Clone, Debug)]
struct FixtureFileSystem {
    data: Arc<FixtureEntry>,
}

#[derive(Debug)]
struct FixtureEntry {
    file_name: OsString,
    path: AbsolutePath,
    is_dir: bool,
    content: FixtureContent,
}

struct FixtureSourceFile {
    lines: Receiver<String>,
}

impl DirEntry for Arc<FixtureEntry> {
    fn path(&self) -> PathBuf {
        self.path.to_path_buf()
    }

    fn file_name(&self) -> OsString {
        self.file_name.clone()
    }

    async fn is_dir(&self) -> Result<bool> {
        Ok(self.is_dir)
    }
}

impl TargetFile for Arc<FixtureEntry> {
    async fn write_line<S: Into<String> + Debug + Send>(&self, line: S) -> Result<()> {
        if let FileFixtureContent { lines, .. } = &self.content {
            let mut lines = lines.write().await;
            lines.push(line.into());
            Ok(())
        } else {
            Err(anyhow!("Trying to write a line to a directory: {:?}: {:?}", &line, &self.path))
        }
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

impl SourceFile for FixtureSourceFile {
    async fn next_line(&mut self) -> Result<Option<String>> {
        Ok(self.lines.recv().await)
    }
}

impl FileSystem for FixtureFileSystem {
    type DirEntryItem = Arc<FixtureEntry>;

    async fn read_dir(&self, directory: &AbsolutePath) -> Result<impl Stream<Item=Result<Self::DirEntryItem>> + Send + Sync + Unpin> {
        let entries = stream! {
            let dir_entry = self.find_entry(directory, |_,_| Ok(None)).await?;
            if let DirFixtureContent { entries, .. } = &dir_entry.content {
                let entries_content = entries.read().await;
                for (_entry_name, entry) in entries_content.iter() {
                    yield Ok(entry.clone());
                }
            }
        };
        Ok(Box::pin(entries))
    }

    async fn path_type(&self, path: &AbsolutePath) -> PathType {
        let Ok(entry) = self.find_entry(path, |_,_| Ok(None)).await else { return PathType::Missing };
        if entry.is_dir {
            PathType::Directory
        } else {
            PathType::File
        }
    }

    async fn open_target(&self, file_path: AbsolutePath, write_mode: WriteMode) -> Result<Option<impl TargetFile>> {
        if write_mode == Ignore {
            return Ok(None);
        }
        let current = self.find_parent_entry(&file_path).await?;
        if let Some(file_name_ref) = file_path.file_name() {
            let file_name = file_name_ref.to_os_string();
            match &current.content {
                DirFixtureContent { entries, .. } => {
                    let mut entries_content = entries.write().await;
                    if let Some(file_entry) = entries_content.get(&file_name.clone()) {
                        if write_mode == Overwrite {
                            if let FileFixtureContent { lines, .. } = &file_entry.content {
                                {
                                    let mut lines_content = lines.write().await;
                                    lines_content.truncate(0)
                                }
                                Ok(Some(file_entry.clone()))
                            } else {
                                Err(anyhow!("Trying to write lines to a directory: {:?}", file_path))
                            }
                        } else {
                            Ok(None)
                        }
                    } else {
                        let content = FileFixtureContent{
                            lines: RwLock::new(Vec::new()),
                        };
                        let new_dir_entry = Arc::new(FixtureEntry {
                            file_name: file_name.clone(),
                            path: file_path.clone(),
                            is_dir: false,
                            content
                        });
                        entries_content.insert(file_name, new_dir_entry.clone());
                        Ok(Some(new_dir_entry))
                    }
                },
                _ => Err(anyhow!("Not a directory: {:?}", file_path.parent()))
            }
        } else {
            Err(anyhow!("Missing file name: {:?}", file_path))
        }
    }

    async fn open_source(&self, file_path: AbsolutePath) -> Result<impl SourceFile> {
        debug!("Open source: {:?}", &file_path);
        let file_entry = self.find_entry(&file_path, |_,_| Ok(None)).await?;
        if file_entry.is_dir().await? {
            Err(anyhow!("Trying to read lines from a directory: {:?}", file_path))
        } else {
            let (tx, rx) = channel(10);
            tokio::spawn(send_lines(file_entry.clone(), tx));
            Ok(FixtureSourceFile { lines: rx })
        }
    }
}

async fn send_lines(file: Arc<FixtureEntry>, tx: Sender<String>) {
    if let FileFixtureContent {lines, ..} = &file.content {
        let lines_read = lines.read().await;
        for line in lines_read.iter() {
            if let Err(e) = tx.send(line.to_string()).await {
                warn!("Error sending line: {:?}", e);
                break;
            }
        }
    }
}

impl FixtureFileSystem {
    async fn find_parent_entry(&self, child_path: &AbsolutePath) -> Result<Arc<FixtureEntry>> {
        if let Some(dir_path) = child_path.parent() {
            let dir_path = AbsolutePath::try_new(dir_path.to_path_buf())?;
            debug!("Find entry for: {:?}", &dir_path);
            Ok(self.find_entry(&dir_path, create_new_directory).await?)
        } else {
            debug!("Found root: {:?}", &self.data.path);
            Ok(self.data.clone())
        }
    }

    async fn find_entry(&self, dir_path: &AbsolutePath, dir_creator: impl DirectoryCreator) -> Result<Arc<FixtureEntry>> {
        let mut current = self.data.clone();
        let mut current_path = PathBuf::from("/");

        let mut components = dir_path.components().peekable();
        if let Some(Component::RootDir) = &mut components.peek() {
            let _root_dir = components.next();
        }

        for component in components {
            if let Component::RootDir = component {
                continue;
            }
            debug!("Component: {:?}", &component);
            let child_entry;
            if let DirFixtureContent {entries,..} = &current.content {
                current_path.push(component);
                debug!("Searching entry in {:?}", &current_path);
                let part = component.as_os_str().to_os_string();
                let entry_option = {
                    let entries_content = entries.read().await;
                    entries_content.get(&part).map(Arc::clone)
                };
                if let Some(entry) = entry_option {
                    child_entry = entry.clone();
                } else {
                    if let Some(new_dir_entry) = dir_creator(&current_path, &part)? {
                        child_entry = Arc::new(new_dir_entry);
                        let mut entries_content = entries.write().await;
                        entries_content.insert(part, child_entry.clone());
                        debug!("Created new directory: {:?}", child_entry);
                    } else {
                        debug!("Not found: {:?}", &current_path);
                        return Err(anyhow!("Not found: {:?}", &current_path));
                    }
                }
            } else {
                return Err(anyhow!("Not a directory: {:?}", &current_path))
            }
            current = child_entry;
        }
        debug!("Found entry: {:?}", &current.path);
        Ok(current)
    }
}

trait DirectoryCreator: Fn(&PathBuf, &OsString) -> Result<Option<FixtureEntry>> {}

// Trick to be able to pass functions with a matching signature as
// implementations of DirectoryCreator
impl<F> DirectoryCreator for F
where F: Fn(&PathBuf, &OsString) -> Result<Option<FixtureEntry>>,
{}

fn create_new_directory(current_path: &PathBuf, part: &OsString) -> Result<Option<FixtureEntry>> {
    let new_dir = DirFixtureContent {
        entries: RwLock::new(AHashMap::new()),
    };
    let new_entry_path = AbsolutePath::try_new(current_path.clone())?;
    let new_dir_entry = FixtureEntry {
        file_name: part.clone(),
        path: new_entry_path,
        is_dir: true,
        content: new_dir
    };
    debug!("Created new directory: {:?}", new_dir_entry);
    Ok(Some(new_dir_entry))
}

#[derive(Deserialize,Debug)]
#[serde(untagged)]
enum FixtureEnum {
    Dir(FixtureDirectory),
    File(String),
}

#[derive(Debug)]
struct FixtureDirectory(AHashMap<String,Box<FixtureEnum>>);

struct FixtureDirectoryVisitor;

impl<'de> Visitor<'de> for FixtureDirectoryVisitor {
    type Value = FixtureDirectory;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a map of fixture enum entries")
    }

    fn visit_map<M>(self, mut access: M) -> std::result::Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut map = AHashMap::new();
        while let Some((key, value)) = access.next_entry()? {
            map.insert(key, value);
        }
        Ok(FixtureDirectory(map))
    }
}

impl<'de> Deserialize<'de> for FixtureDirectory {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>
    {
        deserializer.deserialize_map(FixtureDirectoryVisitor)
    }
}

impl From<FixtureEnum> for FixtureFileSystem {
    fn from(value: FixtureEnum) -> Self {
        let root = AbsolutePath::try_new(PathBuf::from("/")).unwrap();
        let root_entry = convert_enum(&root, &"/", Box::new(value));
        FixtureFileSystem { data: Arc::new(root_entry) }
    }
}

fn convert_enum(parent_path: &AbsolutePath, file_name: &str, data: Box<FixtureEnum>) -> FixtureEntry {
    let this_path = AbsolutePath::new(file_name, &parent_path);
    match *data {
        FixtureEnum::File(body) => {
            let body_iter = BufReader::new(StringReader::new(&body)).lines();
            let mut lines = Vec::new();
            for line in body_iter {
                lines.push(line.unwrap())
            }
            FixtureEntry {
                file_name: OsString::from(file_name),
                path: this_path,
                is_dir: false,
                content: FileFixtureContent { lines: RwLock::new(lines) },
            }
        },
        FixtureEnum::Dir(entries) => {
            let mut content = AHashMap::new();
            for (entry_name, entry) in entries.0 {
                let entry = convert_enum(&this_path, &entry_name, entry);
                content.insert(OsString::from(entry_name), Arc::new(entry));
            }
            trace!("Convert directory: {:?}", &content);
            FixtureEntry {
                file_name: OsString::from(file_name),
                path: this_path.clone(),
                is_dir: true,
                content: DirFixtureContent { entries: RwLock::new(content) },
            }
        },
    }
}

pub fn fixture_file_system<R: Read>(reader: R) -> Result<impl FileSystem> {
    let data : FixtureEnum = serde_yaml::from_reader(reader)?;
    debug!("File system data: {:?}", data);
    Ok::<FixtureFileSystem, anyhow::Error>(data.into())
}

#[cfg(test)]
mod test {
    use std::pin::pin;
    use anyhow::bail;
    use indoc::indoc;
    use stringreader::StringReader;
    use test_log::test;
    use tokio_stream::StreamExt;
    use crate::config_model::WriteMode::WriteNew;
    use crate::path::test_utils::to_absolute_path;
    use super::*;

    #[test]
    fn test_create_test_fixture_file_system() -> Result<()> {
        let fs = create_test_fixture_file_system()?;
        debug!("Improved test fixture file-system: {:?}", fs);
        Ok(())
    }

    #[test(tokio::test)]
    async fn read_dir() -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;

        let entry_str = "other-dir";
        let parent_str = "top-dir";
        let path_os_string = OsString::from("/".to_string() + parent_str + "/" + entry_str);
        let parent = to_absolute_path(parent_str);

        // When
        let entries = read_dir_sorted(&fs, &parent).await?;

        // Then
        let expected = vec!["other-dir", "sibling-file", "sub-dir"].iter().map(OsString::from).collect::<Vec<_>>();
        let actual = entries.iter().map(DirEntry::file_name).collect::<Vec<_>>();
        assert_eq!(actual, expected);

        let entry = entries.get(0).unwrap();
        assert_eq!(entry.path(), path_os_string);
        assert!(entry.is_dir().await?);

        Ok(())
    }

    #[test(tokio::test)]
    async fn open_source() -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;
        let root = AbsolutePath::try_new(PathBuf::from("/"))?;

        // When
        let mut source_file = fs.open_source(AbsolutePath::new("top-dir/sub-dir/file", &root)).await?;

        // Then
        let Some(first_line) = source_file.next_line().await? else { bail!("File is empty") };
        assert_eq!(&first_line, "First line");
        let Some(second_line) = source_file.next_line().await? else { bail!("File is too short") };
        assert_eq!(&second_line, "Second line");
        let Some(third_line) = source_file.next_line().await? else { bail!("File is too short") };
        assert_eq!(&third_line, "Third line");
        assert!(source_file.next_line().await?.is_none());

        Ok(())
    }

    #[test(tokio::test)]
    async fn open_source_non_existent() -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;
        let dir = to_absolute_path("/not-found.txt");

        // When
        let result = fs.open_source(dir).await;

        // Then
        let Err(err) = result else { bail!("Opening a non-existent path should not be Ok") };
        assert!(err.to_string().starts_with("Not found:"), "Actual error: {:?}", &err);

        Ok(())
    }

    #[test(tokio::test)]
    async fn open_source_root() -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;
        let root = to_absolute_path("/");

        // When
        let result = fs.open_source(root).await;

        // Then
        let Err(err) = result else { bail!("Opening root as source should not be Ok") };
        assert!(err.to_string().starts_with("Trying to read lines from a directory:"), "Actual error: {:?}", &err);

        Ok(())
    }

    #[test(tokio::test)]
    async fn open_source_directory() -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;
        let dir = to_absolute_path("/top-dir");

        // When
        let result = fs.open_source(dir).await;

        // Then
        let Err(err) = result else { bail!("Opening a directory as source should not be Ok") };
        assert!(err.to_string().starts_with("Trying to read lines from a directory:"), "Actual error: {:?}", &err);

        Ok(())
    }

    #[test(tokio::test)]
    async fn open_source_file_in_file() -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;
        let dir = to_absolute_path("/.profile/file");

        // When
        let result = fs.open_source(dir).await;

        // Then
        let Err(err) = result else { bail!("Opening a file in a file should not be Ok") };
        assert!(err.to_string().starts_with("Not a directory:"), "Actual error: {:?}", &err);

        Ok(())
    }

    #[test(tokio::test)]
    async fn open_target_overwrite_existing() -> Result<()> {
        open_target_overwrite("top-dir/sub-dir/file").await
    }

    #[test(tokio::test)]
    async fn open_target_overwrite_new() -> Result<()> {
        open_target_overwrite("top-dir/sub-dir/new-file").await
    }

    #[test(tokio::test)]
    async fn open_target_overwrite_new_dir() -> Result<()> {
        open_target_overwrite("top-dir/new-dir/new-file").await
    }

    async fn open_target_overwrite(file: &str) -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;
        let file_path = to_absolute_path(file);

        // When
        let Some(mut target_file) = fs.open_target(file_path.clone(), Overwrite).await? else { bail!("Could not open target") };
        target_file.write_line("Replacement").await?;
        target_file.close().await?;

        // Then
        let mut source_file = fs.open_source(file_path).await?;
        let Some(line) = source_file.next_line().await? else { bail!("New file is empty") };
        assert_eq!(&line, "Replacement");

        Ok(())
    }

    #[test(tokio::test)]
    async fn open_target_dir_overwrite() -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;

        let parent_str = "top-dir/other-dir";
        let parent = to_absolute_path(parent_str);

        // When
        let result = fs.open_target(parent, Overwrite).await;

        // Then
        let Err(err) = result else { bail!("Opening directory as target file should not be Ok") };
        assert!(err.to_string().starts_with("Trying to write lines to a directory"), "Actual error: {:?}", &err);

        Ok(())
    }

    #[test(tokio::test)]
    async fn open_target_dir_write_new() -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;

        let parent_str = "top-dir/other-dir";
        let parent = to_absolute_path(parent_str);

        // When
        let target_file_option = fs.open_target(parent, WriteNew).await?;

        // Then
        assert!(target_file_option.is_none());
        Ok(())
    }

    #[test(tokio::test)]
    async fn open_target_root_ignore() -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;
        let root = to_absolute_path("/");

        // When
        let target_option = fs.open_target(root, Ignore).await?;

        // Then
        assert!(target_option.is_none());

        Ok(())
    }

    #[test(tokio::test)]
    async fn open_missing_target_file_name() -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;
        let root = to_absolute_path("/");
        debug!("File name of root: {:?}", root.file_name());
        assert!(root.file_name().is_none());

        // When
        let result = fs.open_target(root, WriteNew).await;

        // Then
        let Err(err) = result else { bail!("Opening the root of the file-system should not be Ok") };
        assert!(err.to_string().starts_with("Missing file name:"), "Actual error: {:?}", &err);

        Ok(())
    }

    #[test(tokio::test)]
    async fn open_target_file_in_file() -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;
        let file_in_file = to_absolute_path("/.profile/file");

        // When
        let result = fs.open_target(file_in_file, WriteNew).await;

        // Then
        let Err(err) = result else { bail!("Opening a target file in a file should not be Ok") };
        assert!(err.to_string().starts_with("Not a directory:"), "Actual error: {:?}", &err);

        Ok(())
    }

    #[test(tokio::test)]
    async fn open_target_after_attempt_to_read_child_entry() -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;
        let new_file = to_absolute_path("/new");
        let file_in_file = to_absolute_path("/new/file");
        let result = fs.open_source(file_in_file).await;
        let Err(_) = result else { bail!("Opening a source file in a file should not be Ok") };

        // When
        let result = fs.open_target(new_file.clone(), WriteNew).await;

        // Then
        if let Some(mut target_file) = result? {
            target_file.close().await?;
        } else {
            bail!("It should be possible to writ to new file: {:?}", new_file);
        }

        Ok(())
    }

    // Implementation details

    #[test(tokio::test)]
    async fn write_to_dir() -> Result<()> {
        // Given
        let fixture_entry = FixtureEntry {
            file_name: OsString::from("foo"),
            path: to_absolute_path("/foo"),
            is_dir: true,
            content: DirFixtureContent {
                entries: RwLock::new(AHashMap::new())
            },
        };
        let entry = Arc::new(fixture_entry);

        // When
        let result = entry.write_line("Something").await;

        // Then
        let Err(err) = result else { bail!("Opening directory as target file should not be Ok") };
        assert!(err.to_string().starts_with("Trying to write a line to a directory"), "Actual error: {:?}", &err);

        Ok(())
    }

    // Utilities

    fn create_test_fixture_file_system() -> Result<impl FileSystem> {
        let yaml = indoc! {r#"
                ---
                top-dir:
                    sub-dir:
                        file: |
                            First line
                            Second line
                            Third line
                        empty-dir: {}
                        empty-file: ""
                    other-dir:
                        file: |
                            Something completely different:
                            The Larch
                    sibling-file: Foo
                ".profile": echo "Shell!"
            "#};
        trace!("YAML: [{}]", &yaml);

        let yaml_source = StringReader::new(yaml);
        Ok(fixture_file_system(yaml_source)?)
    }

    async fn read_dir_sorted<FS: FileSystem>(fs: &FS, dir: &AbsolutePath) -> Result<Vec<FS::DirEntryItem>> {
        let mut dir_stream = pin!(fs.read_dir(&dir).await?);
        let mut entries = Vec::new();
        while let Some(entry_result) = dir_stream.next().await {
            entries.push(entry_result?)
        }
        entries.sort_by_key(|entry| entry.file_name());
        Ok(entries)
    }
}
