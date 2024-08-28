use std::ops::Deref;
use std::path::{Component, Path, PathBuf};
use anyhow::{anyhow, Result};
use log::debug;

#[derive(Debug, Clone)]
pub struct AbsolutePath(PathBuf);

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct RelativePath(PathBuf);

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct SingleComponent(PathBuf);

impl AbsolutePath {
    pub fn new(path: PathBuf, reference: &AbsolutePath) -> Self {
        if path.is_absolute() {
            AbsolutePath(path)
        } else {
            RelativePath(path).relative_to(reference)
        }
    }

    pub fn try_new(path: PathBuf) -> Result<Self> {
        if path.is_absolute() {
            Ok(AbsolutePath(path))
        } else {
            Err(anyhow!("Not an absolute path: {path:?}"))
        }
    }

    pub fn push(&mut self, path: impl Into<RelativePath>) -> () {
        let relative_path = path.into();
        self.0.push(relative_path.0);
        self.canonicalize();
    }

    pub fn canonicalize(&mut self) {
        let mut canonical = PathBuf::new();
        for component in self.0.components() {
            canonical.push(component);
            // match component {
            //     Component::RootDir => debug!("RootDir"),
            //     Component::ParentDir => debug!("ParentDir"),
            //     Component::CurDir => debug!("CurDir"),
            //     Component::Prefix(prefix) => debug!("Prefix({:?})", prefix),
            //     Component::Normal(part) => debug!("Normal({:?})", part)
            // }
        }
        debug!("Canonical: {:?}", canonical);
        self.0 = canonical;
    }
}

impl TryFrom<String> for AbsolutePath {
    type Error = anyhow::Error;

    fn try_from(path: String) -> std::result::Result<Self, Self::Error> {
        let path_buf = PathBuf::from(path);
        AbsolutePath::try_new(path_buf)
    }
}

impl TryFrom<&str> for AbsolutePath {
    type Error = anyhow::Error;

    fn try_from(path: &str) -> std::result::Result<Self, Self::Error> {
        AbsolutePath::try_from(path.to_string())
    }
}

impl RelativePath {
    pub fn push(&mut self, path: impl Into<RelativePath>) -> () {
        let relative_path = path.into();
        self.0.push(relative_path.0);
    }

    pub fn relative_to(&self, path: &AbsolutePath) -> AbsolutePath {
        let mut result = path.clone();
        result.push(self.clone());
        result
    }
}

impl SingleComponent {
    pub fn try_new(path: impl AsRef<Path>) -> Result<SingleComponent> {
        let path = path.as_ref();
        let mut components = path.components();
        if path.is_absolute() {
            return Err(anyhow!("Attempt to create SigleComponent from absolute path: {path:?}"))
        }
        if let Some(component) = components.next() {
            if components.next().is_none() {
                Ok(SingleComponent(PathBuf::from(component.as_os_str())))
            } else {
                Err(anyhow!("Attempt to create SigleComponent from path with multiple components: {path:?}"))
            }
        } else {
            Err(anyhow!("Attempt to create SingleComponent from path with no components"))
        }
    }
}

impl Deref for AbsolutePath {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for RelativePath {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<SingleComponent> for RelativePath {
    fn from(value: SingleComponent) -> Self {
        RelativePath::from(value.0)
    }
}

impl From<PathBuf> for RelativePath {
    fn from(value: PathBuf) -> Self {
        if value.is_absolute() {
            let prefix = if let Some(prefix) = get_path_prefix(value.as_path()) {
                prefix
            } else {
                PathBuf::from("/")
            };
            RelativePath(value.as_path().strip_prefix(prefix).unwrap().to_owned())
        } else {
            RelativePath(value)
        }
    }
}

impl TryFrom<Component<'_>> for RelativePath {
    type Error = anyhow::Error;

    fn try_from(value: Component) -> std::result::Result<Self, Self::Error> {
        let result = PathBuf::from(value.as_os_str());
        if result.is_absolute() {
            Err(anyhow!("Could not convert absolute component to relative path: {value:?}"))
        } else {
            Ok(result.into())
        }
    }
}
impl From<&str> for RelativePath {
    fn from(value: &str) -> Self {
        PathBuf::from(value).into()
    }
}

impl From<String> for RelativePath {
    fn from(value: String) -> Self {
        PathBuf::from(&value).into()
    }
}

fn get_path_prefix(path: &Path) -> Option<PathBuf> {
    if let Some(Component::Prefix(prefix_component)) = path.components().next() {
        Some(PathBuf::from(prefix_component.as_os_str()))
    } else {
        None
    }
}

#[cfg(test)]
pub mod test_utils {
    use std::path::PathBuf;
    use crate::path::AbsolutePath;

    pub fn to_absolute_path<S: Into<String>>(path: S) -> AbsolutePath {
        let root = AbsolutePath::try_new(PathBuf::from("/")).unwrap();
        AbsolutePath::new(PathBuf::from(path.into()), &root)
    }
}