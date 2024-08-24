use std::fmt;
use std::io::{BufRead, BufReader, Read};
use std::path::Component;
use std::sync::Arc;
use ahash::AHashMap;
use anyhow::anyhow;
use async_stream::stream;
use log::{debug, warn};
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
    file: Arc<FixtureEntry>,
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
            Err(anyhow!(format!("Trying to write a line to a directory: {:?}: {:?}", &line, &self.path)))
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

    async fn read_dir(&self, directory: &AbsolutePath) -> Result<impl Stream<Item=Result<Self::DirEntryItem>> + Send + Sync> {
        let entries = stream! {
            let dir_entry = self.find_entry(directory).await?;
            if let DirFixtureContent { entries, .. } = &dir_entry.content {
                let entries_content = entries.read().await;
                for (_entry_name, entry) in entries_content.iter() {
                    yield Ok(entry.clone());
                }
            }
        };
        Ok(entries)
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
                    let entries_content = entries.write().await;
                    if let Some(file_entry) = entries_content.get(&file_name.clone()) {
                        if let FileFixtureContent {lines,..} = &file_entry.content {
                            if write_mode == Overwrite {
                                {
                                    let mut lines_content = lines.write().await;
                                    lines_content.truncate(0)
                                }
                                Ok(Some(file_entry.clone()))
                            } else {
                                Ok(None)
                            }
                        } else {
                            Err(anyhow!(format!("Trying to write lines to a directory: {:?}", file_path)))
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
                        let mut entries_content = entries.write().await;
                        entries_content.insert(file_name, new_dir_entry.clone());
                        Ok(Some(new_dir_entry))
                    }
                },
                _ => Err(anyhow!(format!("Not a directory: {:?}", file_path.parent())))
            }
        } else {
            Err(anyhow!(format!("Missing file name: {:?}", file_path)))
        }
    }

    async fn open_source(&self, file_path: AbsolutePath) -> Result<impl SourceFile> {
        let current = self.find_parent_entry(&file_path).await?;
        if let Some(file_name_ref) = file_path.file_name() {
            let file_name = file_name_ref.to_os_string();
            debug!("Open source: {:?}: {:?}", &file_name, &current);
            if let DirFixtureContent { entries, .. } = &current.content {
                let entries_content = entries.read().await;
                if let Some(file_entry) = entries_content.get(&file_name.clone()) {
                    if file_entry.is_dir().await? {
                        Err(anyhow!(format!("Trying to read lines from a directory: {:?}", file_path)))
                    } else {
                        let (tx, rx) = channel(10);
                        tokio::spawn(send_lines(file_entry.clone(), tx));
                        Ok(FixtureSourceFile { file: file_entry.clone(), lines: rx })
                    }
                } else {
                    for (n, _) in entries_content.iter() {
                        debug!("Entry: {:?}", &n);
                    }
                    Err(anyhow!(format!("Not found: {:?}", &file_path)))
                }
            } else {
                Err(anyhow!(format!("Not a directory: {:?}", &file_path.parent())))
            }
        } else {
            Err(anyhow!(format!("Missing file name: {:?}", &file_path)))
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
            Ok(self.find_entry(&dir_path).await?)
        } else {
            debug!("Found root: {:?}", &self.data.path);
            Ok(self.data.clone())
        }
    }
    async fn find_entry(&self, dir_path: &AbsolutePath) -> Result<Arc<FixtureEntry>> {
        let mut current = self.data.clone();
        let mut current_path = PathBuf::new();

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
                let mut entries_content = entries.write().await;
                let part = component.as_os_str().to_os_string();
                if let Some(entry) = entries_content.get(&part) {
                    child_entry = entry.clone();
                } else {
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
                    child_entry = Arc::new(new_dir_entry);
                    entries_content.insert(part, current.clone());
                    debug!("Created new directory: {:?}", child_entry);
                }
            } else {
                return Err(anyhow!(format!("Not a directory: {:?}", &current_path)))
            }
            current = child_entry;
        }
        debug!("Found dir: {:?}", &current.path);
        Ok(current)
    }
}

struct FixtureDataEntries(AHashMap<String,Box<FixtureData>>);

#[derive(Deserialize)]
struct FixtureData {
    entries: Option<FixtureDataEntries>,
    body: Option<String>,
}

struct FixtureDataEntriesVisitor;

impl<'de> Visitor<'de> for FixtureDataEntriesVisitor {
    type Value = FixtureDataEntries;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a map of fixture dat entries")
    }

    fn visit_map<M>(self, mut access: M) -> std::result::Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut map = AHashMap::new();
        while let Some((key, value)) = access.next_entry()? {
            map.insert(key, value);
        }
        Ok(FixtureDataEntries(map))
    }
}

impl<'de> Deserialize<'de> for FixtureDataEntries {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(FixtureDataEntriesVisitor)
    }
}

impl From<FixtureData> for FixtureFileSystem {
    fn from(_value: FixtureData) -> Self {
        let root = AbsolutePath::try_new(PathBuf::from("/")).unwrap();
        let root_entry = convert(&root, &"/", Box::new(_value));
        FixtureFileSystem { data: Arc::new(root_entry) }
    }
}

fn convert(parent_path: &AbsolutePath, file_name: &str, data: Box<FixtureData>) -> FixtureEntry {
    debug!("Convert: {:?}", file_name);
    let this_path = AbsolutePath::new(PathBuf::from(file_name), &parent_path);
    if let Some(entries) = data.entries {
        let mut content = AHashMap::new();
        for (entry_name, entry) in entries.0 {
            let entry = convert(&this_path, &entry_name, entry);
            content.insert(OsString::from(entry_name), Arc::new(entry));
        }
        debug!("Convert directory: {:?}", &content);
        FixtureEntry {
            file_name: OsString::from(file_name),
            path: this_path.clone(),
            is_dir: true,
            content: DirFixtureContent { entries: RwLock::new(content) },
        }
    } else if let Some(body) = data.body {
        let body = BufReader::new(StringReader::new(&body)).lines();
        let mut lines = Vec::new();
        for line in body {
            lines.push(line.unwrap())
        }
        debug!("Convert file: {:?}: {:?}: {:?}", &this_path, file_name, &lines);
        FixtureEntry {
            file_name: OsString::from(file_name),
            path: this_path.clone(),
            is_dir: false,
            content: FileFixtureContent { lines: RwLock::new(lines) },
        }
    } else {
        FixtureEntry {
            file_name: OsString::from(file_name),
            path: this_path.clone(),
            is_dir: true,
            content: DirFixtureContent { entries: RwLock::new(AHashMap::new()) },
        }
    }
}

pub fn fixture_file_system<R: Read>(reader: R) -> Result<impl FileSystem> {
    let data : FixtureData = serde_yaml::from_reader(reader)?;
    Ok::<FixtureFileSystem, anyhow::Error>(data.into())
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use log::debug;
    use stringreader::StringReader;
    use super::*;

    #[tokio::test]
    async fn open_source() -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;
        let root = AbsolutePath::try_new(PathBuf::from("/"))?;

        // When
        let mut source_file = fs.open_source(AbsolutePath::new(PathBuf::from("top-dir/sub-dir/file"), &root)).await?;

        // Then
        if let Some(line) = source_file.next_line().await? {
            assert_eq!(&line, "First line");
        } else {
            assert!(false);
        }

        Ok(())
    }

    #[tokio::test]
    async fn open_target() -> Result<()> {
        // Given
        let fs = create_test_fixture_file_system()?;
        let file_path = to_absolute_path("top-dir/sub-dir/file");

        // When
        if let Some(target_file) = fs.open_target(file_path.clone(), Overwrite).await? {
            target_file.write_line("Replacement").await?;
        } else {
            assert!(false, "Could not open target");
        }

        // Then
        let mut source_file = fs.open_source(file_path).await?;
        if let Some(line) = source_file.next_line().await? {
            assert_eq!(&line, "Replacement");
        } else {
            assert!(false, "New file is empty");
        }

        Ok(())
    }

    fn to_absolute_path<S: Into<String>>(path: S) -> AbsolutePath {
        let root = AbsolutePath::try_new(PathBuf::from("/")).unwrap();
        AbsolutePath::new(PathBuf::from(path.into()), &root)
    }

    fn create_test_fixture_file_system() -> Result<impl FileSystem> {
        let yaml = indoc! {r#"
                ---
                entries:
                    top-dir:
                        entries:
                            sub-dir:
                                entries:
                                    file:
                                        body: |
                                            First line
                                            Second line
                                    empty-dir: {}
                                    empty-file:
                                        body: ""
                            other-dir:
                                entries:
                                    file:
                                        body: |
                                            Something completely different:
                                            The Larch
                            sibling-file:
                                body: |
                                    Foo
                    ".profile":
                        body: |
                            echo "Shell!"
            "#};
        debug!("YAML: [{}]", &yaml);

        let yaml_source = StringReader::new(yaml);
        Ok(fixture_file_system(yaml_source)?)
    }
}
