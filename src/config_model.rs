#![allow(dead_code)]

pub mod invar_config;
pub use invar_config::{InvarConfig, WriteMode};
mod invar_config_data;
use invar_config_data::InvarConfigData;

pub mod niche_description;
pub use niche_description::NicheDescription;
mod niche_description_data;

pub mod thundercloud_config;
pub use thundercloud_config::ThundercloudConfig;
mod thundercloud_config_data;

use anyhow::Result;
use std::borrow::Cow;
use std::fmt::Debug;
use std::io::Read;
use once_cell::sync::Lazy;
use serde::Deserialize;
use crate::path::AbsolutePath;

#[derive(Deserialize,Debug,Clone,Eq, PartialEq)]
pub enum OnIncoming {
    Update,
    Ignore,
    Warn,
    Fail
}

pub trait NicheConfig : Sized + Debug {
    fn from_reader<R: Read>(reader: R) -> Result<Self>;
    fn use_thundercloud(&self) -> &impl UseThundercloudConfig;
    fn new_thunder_config(&self, thundercloud_directory: AbsolutePath, invar: AbsolutePath, project_root: AbsolutePath) -> impl ThunderConfig;
}

#[derive(Deserialize,Debug)]
struct NicheConfigData {
    #[serde(rename = "use-thundercloud")]
    use_thundercloud: UseThundercloudConfigData,
}

pub mod niche_config {
    use super::*;

    pub fn from_reader<R: Read>(reader: R) -> Result<impl NicheConfig> {
        let config: NicheConfigData = NicheConfig::from_reader(reader)?;
        Ok(config)
    }

    impl NicheConfig for NicheConfigData {
        fn from_reader<R: Read>(reader: R) -> Result<Self> {
            let config: NicheConfigData = serde_yaml::from_reader(reader)?;
            Ok(config)
        }

        fn use_thundercloud(&self) -> &impl UseThundercloudConfig {
            &self.use_thundercloud
        }

        fn new_thunder_config(&self, thundercloud_directory: AbsolutePath, invar: AbsolutePath, project_root: AbsolutePath) -> impl ThunderConfig {
            ThunderConfigData::new(
                self.use_thundercloud.clone(),
                thundercloud_directory,
                invar,
                project_root
            )
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;
        use super::test_utils::insert_entry;
        use super::OnIncoming::Update;
        use super::WriteMode::Ignore;
        use indoc::indoc;
        use log::debug;
        use serde_yaml::Mapping;
        use std::path::PathBuf;
        use stringreader::StringReader;

        #[test]
        fn test_from_reader() -> Result<()> {
            // Given
            let yaml = indoc! {r#"
                ---
                use-thundercloud:
                  directory: "{{PROJECT}}/example-thundercloud"
                  on-incoming: Update
                  features:
                    - glass
                    - bash_config
                    - kermie
                  invar-defaults:
                    write-mode: Ignore
                    interpolate: false
                    props:
                      mathtur: Jeremy
                      buyer: Myra LeJean
                      milk-man: Kaos
            "#};
            debug!("YAML: [{}]", &yaml);
            let yaml_source = StringReader::new(yaml);

            // When
            let niche_config = from_reader(yaml_source)?;

            // Then
            let use_thundercloud = niche_config.use_thundercloud();
            assert_eq!(use_thundercloud.directory(), Some("{{PROJECT}}/example-thundercloud".to_string()).as_ref());
            assert_eq!(use_thundercloud.on_incoming(), &Update);
            assert_eq!(use_thundercloud.features(), &["glass", "bash_config", "kermie"]);

            let invar_defaults = use_thundercloud.invar_defaults().into_owned();
            assert_eq!(invar_defaults.write_mode_option(), Some(Ignore));
            assert_eq!(invar_defaults.interpolate_option(), Some(false));

            let mut mapping = Mapping::new();
            insert_entry(&mut mapping, "mathtur", "Jeremy");
            insert_entry(&mut mapping, "buyer", "Myra LeJean");
            insert_entry(&mut mapping, "milk-man", "Kaos");
            assert_eq!(invar_defaults.props().into_owned(), mapping);

            Ok(())
        }

        #[test]
        fn test_new_thunder_config() -> Result<()> {
            // Given
            let yaml_source = StringReader::new(indoc! {r#"
                ---
                use-thundercloud:
                  directory: "{{PROJECT}}/example-thundercloud"
            "#});
            let niche_config = from_reader(yaml_source)?;
            let thunder_cloud_dir = AbsolutePath::try_from("/tmp")?;
            let invar_dir = AbsolutePath::try_from("/var/tmp")?;
            let project_root = AbsolutePath::try_from("/")?;
            let cumulus = AbsolutePath::new(PathBuf::from("cumulus"), &thunder_cloud_dir);

            // When
            let thunder_config = niche_config.new_thunder_config(thunder_cloud_dir.clone(), invar_dir.clone(), project_root.clone());

            // Then
            assert_eq!(thunder_config.use_thundercloud().directory(), niche_config.use_thundercloud().directory());
            assert_eq!(thunder_config.project_root().as_path(), project_root.as_path());
            assert_eq!(thunder_config.thundercloud_directory().as_path(), thunder_cloud_dir.as_path());
            assert_eq!(thunder_config.invar().as_path(), invar_dir.as_path());
            assert_eq!(thunder_config.cumulus().as_path(), cumulus.as_path());
            Ok(())
        }
    }
}

pub trait ThunderConfig : Debug + Send + Sync {
    fn use_thundercloud(&self) -> &impl UseThundercloudConfig;
    fn thundercloud_directory(&self) -> &AbsolutePath;
    fn cumulus(&self) -> &AbsolutePath;
    fn invar(&self) -> &AbsolutePath;
    fn project_root(&self) -> &AbsolutePath;
}

#[derive(Debug)]
struct ThunderConfigData {
    use_thundercloud: UseThundercloudConfigData,
    thundercloud_directory: AbsolutePath,
    cumulus: AbsolutePath,
    invar: AbsolutePath,
    project: AbsolutePath,
}

impl ThunderConfigData {
    fn new(use_thundercloud: UseThundercloudConfigData, thundercloud_directory: AbsolutePath, invar: AbsolutePath, project: AbsolutePath) -> Self {
        let mut cumulus = thundercloud_directory.clone();
        cumulus.push("cumulus");
        ThunderConfigData {
            use_thundercloud,
            thundercloud_directory,
            cumulus,
            invar,
            project,
        }
    }
}

impl ThunderConfig for ThunderConfigData {

    fn use_thundercloud(&self) -> &impl UseThundercloudConfig {
        &self.use_thundercloud
    }

    fn thundercloud_directory(&self) -> &AbsolutePath {
        &self.thundercloud_directory
    }

    fn cumulus(&self) -> &AbsolutePath {
        &self.cumulus
    }

    fn invar(&self) -> &AbsolutePath {
        &self.invar
    }

    fn project_root(&self) -> &AbsolutePath {
        &self.project
    }
}

pub trait UseThundercloudConfig : Debug + Clone {
    type InvarConfigImpl : InvarConfig;
    type GitRemoteConfigImpl : GitRemoteConfig;
    fn directory(&self) -> Option<&String>;
    fn on_incoming(&self) -> &OnIncoming;
    fn features(&self) -> &[String];
    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl>;
    fn git_remote(&self) -> Option<&Self::GitRemoteConfigImpl>;
}

#[derive(Deserialize,Debug,Clone)]
struct UseThundercloudConfigData {
    directory: Option<String>,
    #[serde(rename = "git-remote")]
    git_remote: Option<GitRemoteConfigData>,
    #[serde(rename = "on-incoming")]
    on_incoming: Option<OnIncoming>,
    features: Option<Vec<String>>,
    #[serde(rename = "invar-defaults")]
    invar_defaults: Option<InvarConfigData>,
}

pub trait GitRemoteConfig {
    fn fetch_url(&self) -> &str;
    fn revision(&self) -> &str;
}

#[derive(Deserialize,Debug,Clone)]
struct GitRemoteConfigData {
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

static UPDATE: Lazy<OnIncoming> = Lazy::new(|| OnIncoming::Update);
static EMPTY_VEC: Lazy<Vec<String>> = Lazy::new(Vec::new);

impl UseThundercloudConfig for UseThundercloudConfigData {
    type InvarConfigImpl = InvarConfigData;
    type GitRemoteConfigImpl = GitRemoteConfigData;
    fn directory(&self) -> Option<&String> {
        self.directory.as_ref()
    }
    fn on_incoming(&self) -> &OnIncoming {
        &self.on_incoming.as_ref().unwrap_or(&UPDATE)
    }
    fn features(&self) -> &[String] {
        &self.features.as_deref().unwrap_or(&EMPTY_VEC)
    }
    fn invar_defaults(&self) -> Cow<Self::InvarConfigImpl> {
        if let Some(invar_defaults) = &self.invar_defaults {
            Cow::Borrowed(invar_defaults)
        } else {
            Cow::Owned(Self::InvarConfigImpl::new())
        }
    }
    fn git_remote(&self) -> Option<&Self::GitRemoteConfigImpl> {
        self.git_remote.as_ref()
    }
}

#[cfg(test)]
mod test_utils {
    use serde_yaml::{Mapping, Value};

    pub fn insert_entry<K: Into<String>, V: Into<String>>(props: &mut Mapping, key: K, value: V) {
        let wrapped_key = Value::String(key.into());
        let wrapped_value = Value::String(value.into());
        props.insert(wrapped_key, wrapped_value);
    }
}
