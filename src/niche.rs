use std::path::{Path, PathBuf};
use ahash::AHashMap;
use log::{debug, info};
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_yaml::{Mapping, Value};
use tokio::fs::DirEntry;
use crate::interpolate;
use crate::niche::OnIncoming::Update;
use crate::thundercloud;

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

#[derive(Debug)]
pub struct ThunderConfig {
    use_thundercloud: UseThundercloudConfig,
    cumulus: PathBuf,
    invar: PathBuf,
    project: PathBuf,
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

static UPDATE: Lazy<OnIncoming> = Lazy::new(|| Update);
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

pub async fn process_niche(project_root: impl AsRef<Path>, niches_directory: impl AsRef<Path>, entry: DirEntry) -> anyhow::Result<()> {
    let mut work_area = project_root.as_ref().to_owned();
    work_area.push("..");
    let mut niche_directory = niches_directory.as_ref().to_owned();
    niche_directory.push(entry.file_name());
    let config = get_config(&niche_directory)?;
    if let Some(directory) = &config.use_thundercloud.directory {
        info!("Directory: {directory:?}");

        let mut substitutions = AHashMap::new();
        substitutions.insert("WORKSPACE".to_string(), work_area.to_string_lossy().to_string());
        substitutions.insert("PROJECT".to_string(), project_root.as_ref().to_string_lossy().to_string());
        let directory = interpolate::interpolate(directory, substitutions);

        let thundercloud_directory = PathBuf::from(directory.into_owned());

        let mut cumulus = thundercloud_directory.clone();
        cumulus.push("cumulus");
        let mut invar = niche_directory.clone();
        invar.push("invar");
        let thunder_config = ThunderConfig {
            use_thundercloud: config.use_thundercloud.clone(),
            cumulus,
            invar,
            project: project_root.as_ref().to_owned()
        };
        debug!("Thunder_config: {thunder_config:?}");

        thundercloud::process_niche(&thundercloud_directory, &niche_directory, project_root).await?;
    }

    Ok(())
}

fn get_config(niche_directory: impl AsRef<Path>) -> anyhow::Result<NicheConfig> {
    let mut config_path = niche_directory.as_ref().to_owned();
    config_path.push("thettingth.yaml");
    info!("Config path: {config_path:?}");

    let file = std::fs::File::open(config_path)?;
    let config: NicheConfig = serde_yaml::from_reader(file)?;
    debug!("Niche configuration: {config:?}");
    let use_thundercloud = &config.use_thundercloud;
    debug!("Niche simplified: {:?}: {:?}: {:?}", use_thundercloud.on_incoming(), use_thundercloud.options(), use_thundercloud.params());
    Ok(config)
}