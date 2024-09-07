use super::GitRemoteConfig;
use serde::{Deserialize, Serialize};

#[derive(Deserialize,Serialize,Debug,Clone)]
#[serde(rename_all = "kebab-case")]
pub struct GitRemoteConfigData {
    fetch_url: String,
    revision: String,
}

impl GitRemoteConfig for GitRemoteConfigData {
    fn fetch_url(&self) -> &str {
        &self.fetch_url
    }
    fn revision(&self) -> &str {
        &self.revision
    }
}

impl GitRemoteConfigData {
    pub fn new(fetch_url: impl Into<String>, revision: impl Into<String>) -> Self {
        GitRemoteConfigData {
            fetch_url: fetch_url.into(),
            revision: revision.into(),
        }
    }
}
