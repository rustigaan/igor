use super::GitRemoteConfig;
use serde::{Deserialize, Serialize};

#[derive(Deserialize,Serialize,Debug,Clone)]
#[serde(rename_all = "kebab-case")]
pub struct GitRemoteConfigData {
    fetch_url: String,
    revision: String,
    sub_path: Option<String>,
}

impl GitRemoteConfig for GitRemoteConfigData {
    fn fetch_url(&self) -> &str {
        &self.fetch_url
    }
    fn revision(&self) -> &str {
        &self.revision
    }
    fn sub_path(&self) -> Option<&str> {
        self.sub_path.as_ref().map(|x| &**x)
    }
}

impl GitRemoteConfigData {
    pub fn new<S: Into<String>>(fetch_url: impl Into<String>, revision: impl Into<String>, sub_path: Option<S>) -> Self {
        GitRemoteConfigData {
            fetch_url: fetch_url.into(),
            revision: revision.into(),
            sub_path: sub_path.map(Into::into)
        }
    }
}
