use std::path::{Path, PathBuf};
use ahash::AHashMap;
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_yaml::{Mapping, Value};

#[derive(Deserialize,Debug,Clone)]
pub enum OnIncoming {
    Update,
    Ignore,
    Warn,
    Fail
}

#[derive(Deserialize,Debug)]
pub struct NicheConfig {
    #[serde(rename = "use-thundercloud")]
    use_thundercloud: UseThundercloudConfig,
}

impl NicheConfig {
    pub fn use_thundercloud(&self) -> &UseThundercloudConfig {
        &self.use_thundercloud
    }
}

#[derive(Debug)]
pub struct ThunderConfig {
    use_thundercloud: UseThundercloudConfig,
    thundercloud_directory: PathBuf,
    cumulus: PathBuf,
    invar: PathBuf,
    project: PathBuf,
}

impl ThunderConfig {
    pub fn new(use_thundercloud: UseThundercloudConfig, thundercloud_directory: PathBuf, invar: PathBuf, project: PathBuf) -> Self {
        let mut cumulus = thundercloud_directory.clone();
        cumulus.push("cumulus");
        ThunderConfig {
            use_thundercloud,
            thundercloud_directory,
            cumulus,
            invar,
            project,
        }
    }

    pub fn use_thundercloud(&self) -> &UseThundercloudConfig {
        &self.use_thundercloud
    }

    pub fn thundercloud_directory(&self) -> &Path {
        &self.thundercloud_directory
    }

    pub fn cumulus(&self) -> &Path {
        &self.cumulus
    }

    pub fn invar(&self) -> &Path {
        &self.invar
    }

    pub fn project_root(&self) -> &Path {
        &self.project
    }
}

#[allow(dead_code)]
#[derive(Deserialize,Debug,Clone)]
pub struct UseThundercloudConfig {
    directory: Option<String>,
    #[serde(rename = "git-remote")]
    git_remote: Option<GitRemoteConfig>,
    #[serde(rename = "on-incoming")]
    on_incoming: Option<OnIncoming>,
    options: Option<Vec<String>>,
    params: Option<Mapping>,
}

impl UseThundercloudConfig {
    pub fn directory(&self) -> Option<&String> {
        self.directory.as_ref()
    }
}

static UPDATE: Lazy<OnIncoming> = Lazy::new(|| OnIncoming::Update);
static EMPTY_VEC: Lazy<Vec<String>> = Lazy::new(Vec::new);

impl UseThundercloudConfig {
    pub fn on_incoming(&self) -> &OnIncoming {
        &self.on_incoming.as_ref().unwrap_or(&UPDATE)
    }
    pub fn options(&self) -> &[String] {
        &self.options.as_deref().unwrap_or(&EMPTY_VEC)
    }

    pub fn params(&self) -> AHashMap<String,String> {
        if let Some(params) = &self.params {
            let map: AHashMap<String, String> = params.iter().map(to_strings).filter(Option::is_some).map(Option::unwrap).collect();
            map
        } else {
            AHashMap::new()
        }
    }
}

fn to_strings(entry: (&Value, &Value)) -> Option<(String, String)> {
    if let (Value::String(key), Value::String(val)) = entry {
        Some((key.to_owned(), val.to_owned()))
    } else {
        None
    }
}

#[allow(dead_code)]
#[derive(Deserialize,Debug,Clone)]
pub struct GitRemoteConfig {
    #[serde(rename = "fetch-url")]
    fetch_url: String,
    revision: String,
    #[serde(rename = "on-incoming")]
    on_incoming: Option<OnIncoming>,
}
