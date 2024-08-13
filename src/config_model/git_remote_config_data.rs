use super::GitRemoteConfig;
use serde::Deserialize;

#[derive(Deserialize,Debug,Clone)]
pub struct GitRemoteConfigData {
    #[serde(rename = "fetch-url")]
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
