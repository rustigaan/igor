use std::path::PathBuf;
use anyhow::{anyhow, Result};
use base16ct::encoded_len;
use log::{debug, info};
use sha2::{Digest, Sha256};
use toml::{Table, Value};
use tokio::process::Command;
use tokio::fs::create_dir_all;
use crate::config_model::{GitRemoteConfig, InvarConfig, UseThundercloudConfig};
use crate::file_system::{real_file_system, FileSystem};
use crate::{interpolate, NicheName};
use crate::file_system::PathType::Directory;
use crate::niche::UseFileSystem::{ProjectFs, RealFs};
use crate::thundercloud;
use crate::path::{AbsolutePath, RelativePath};

#[derive(Eq,PartialEq)]
enum UseFileSystem { ProjectFs, RealFs }

pub async fn process_niche<UT: UseThundercloudConfig, FS: FileSystem, IC: InvarConfig>(project_root: AbsolutePath, niches_directory: RelativePath, niche: NicheName, use_thundercloud: UT, invar_config_default: IC, fs: FS, target_dir: AbsolutePath) -> Result<()> {
    let absolute_niches_directory = AbsolutePath::new(niches_directory.as_path(), &project_root);
    let niche_directory = AbsolutePath::new(niche.to_str(), &absolute_niches_directory);
    let mut invar = niche_directory.clone();
    invar.push("invar");

    let thundercloud_directory = get_thundercloud_directory(&project_root, &niche, &use_thundercloud, &fs, target_dir).await?;
    if let (Some(thundercloud_directory), use_fs) = thundercloud_directory {
        info!("Thundercloud directory: {thundercloud_directory:?}");

        match use_fs {
            RealFs => {
                let  tfs = real_file_system().read_only();
                create_config_and_call_process_niche(project_root, use_thundercloud, invar_config_default, fs, tfs, thundercloud_directory, invar).await
            },
            ProjectFs => {
                let  tfs = fs.clone();
                create_config_and_call_process_niche(project_root, use_thundercloud, invar_config_default, fs, tfs, thundercloud_directory, invar).await
            }
        }
    } else {
        Ok(())
    }
}

async fn create_config_and_call_process_niche<UT: UseThundercloudConfig, FS: FileSystem, TFS: FileSystem, IC: InvarConfig>(project_root: AbsolutePath, use_thundercloud: UT, invar_config_default: IC, fs: FS, tfs: TFS, thundercloud_directory: AbsolutePath, invar: AbsolutePath) -> Result<()> {
    let thunder_config = use_thundercloud.new_thunder_config(
        invar_config_default,
        tfs.read_only(),
        thundercloud_directory,
        fs,
        invar,
        project_root,
    );
    debug!("Thunder_config: {thunder_config:?}");

    thundercloud::process_niche(thunder_config).await?;

    Ok(())
}

async fn get_thundercloud_directory<UT: UseThundercloudConfig, FS: FileSystem>(project_root: &AbsolutePath, niche: &NicheName, use_thundercloud: &UT, fs: &FS, target_dir: AbsolutePath) -> Result<(Option<AbsolutePath>, UseFileSystem)> {
    if let Some(directory) = use_thundercloud.directory() {
        debug!("Directory: {niche:?}: {directory:?}");

        let work_area = AbsolutePath::new("..", &project_root);
        let mut substitutions = Table::new();
        substitutions.insert("WORKSPACE".to_string(), Value::String(work_area.to_string_lossy().to_string()));
        substitutions.insert("PROJECT".to_string(), Value::String(project_root.to_string_lossy().to_string()));
        let directory = interpolate::interpolate(&directory, &substitutions);

        let current_dir = AbsolutePath::current_dir()?;
        let thundercloud_directory = AbsolutePath::new(directory.to_string(), &current_dir);
        if fs.path_type(&thundercloud_directory).await == Directory {
            info!("Thundercloud directory: {niche:?}: {directory:?}");
            return Ok((Some(thundercloud_directory.to_owned()), ProjectFs))
        } else {
            info!("Not found: Directory: {niche:?}: {directory:?}. Try Git");
        }
    }
    if let Some(git_remote) = use_thundercloud.git_remote() {
        let thundercloud_fs = real_file_system();
        let fetch_url = git_remote.fetch_url();
        info!("Fetch URL: {niche:?}: {fetch_url:?}");
        let mut path = target_dir.clone();
        let dir = digest(fetch_url)?;
        let git_path = AbsolutePath::new(dir.clone(), &path);
        info!("Git directory: {niche:?}: {git_path:?}");
        if thundercloud_fs.path_type(&git_path).await == Directory {
            path.push(dir);
            info!("TODO: Update repository [{fetch_url:?}] in [{path:?}]");
            git_pull(&path).await?;
        } else {
            info!("TODO: Clone repository [{fetch_url:?}] into [{path:?}] / [{dir:?}]");
            git_clone(fetch_url, &path, &dir).await?;
            path.push(dir);
        }
        return Ok((Some(path), RealFs));
    }
    Ok((None, ProjectFs))
}

async fn git_pull(path: &AbsolutePath) -> Result<()> {
    let path_clone = PathBuf::clone(path);
    let path_os_string = path_clone.into_os_string();
    let mut child = Command::new("git")
        .arg("-C").arg(path_os_string)
        .arg("pull")
        .spawn()?;
    let status = child.wait().await?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("Git clone exited with status {status:?}"))
    }
}

async fn git_clone(fetch_url: &str, path: &AbsolutePath, dir: &str) -> Result<()> {
    let path_clone = PathBuf::clone(path);
    create_dir_all(path_clone.clone()).await?;
    let mut child = Command::new("git")
        .current_dir(path_clone)
        .arg("clone").arg(fetch_url).arg(dir)
        .spawn()?;
    let status = child.wait().await?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("Git clone exited with status {status:?}"))
    }
}

fn digest(fetch_url: &str) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(fetch_url.as_bytes());
    let hash = hasher.finalize();
    let length = encoded_len(hash.as_slice());
    let mut buffer = [0u8; 64];
    base16ct::lower::encode(hash.as_slice(), &mut buffer).map_err(|e| anyhow!(e))?;
    let digest = String::from_utf8_lossy(buffer.get(0..length).ok_or_else(|| anyhow!(""))?);
    Ok(digest.to_string())
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use log::trace;
    use test_log::test;
    use crate::config_model::{invar_config, project_config, NicheTriggers, ProjectConfig, PsychotropicConfig};
    use crate::file_system::{fixture, FileSystem};
    use crate::file_system::ConfigFormat::TOML;
    use crate::path::test_utils::to_absolute_path;
    use super::*;

    #[test(tokio::test)]
    async fn test() -> Result<()> {
        // Given
        let fs = create_file_system_fixture()?;

        let project_root = AbsolutePath::root();
        let cargo_cult_toml_data = fs.get_content(AbsolutePath::new("CargoCult.toml", &project_root)).await?;
        let project_config = project_config::from_str(&cargo_cult_toml_data, TOML)?;
        let niche = NicheName::new("example");
        let psychotropic = project_config.psychotropic()?;
        let use_thundercloud = psychotropic
            .get(niche.to_str())
            .map(NicheTriggers::use_thundercloud).flatten()
            .unwrap();
        let niches_directory = RelativePath::from("yeth-marthter");
        let default_invar_config = invar_config::from_str("", TOML)?;
        let target_dir = create_target_dir()?;

        // When
        process_niche(project_root, niches_directory, niche.clone(), use_thundercloud.clone(), default_invar_config, fs.clone(), target_dir).await?;

        // Then
        let content = fs.get_content(to_absolute_path("/workshop/clock.yaml")).await?;
        let expected = indoc! {r#"
            ---
            raising:
              - "steam"
              - "money"
        "#};
        assert_eq!(&content, expected);

        Ok(())
    }

    //#[test(tokio::test)]
    async fn test_git_remote() -> Result<()> {
        // Given
        let fs = create_file_system_fixture()?;

        let project_root = AbsolutePath::root();
        let cargo_cult_toml_data = fs.get_content(AbsolutePath::new("CargoCult.toml", &project_root)).await?;
        let project_config = project_config::from_str(&cargo_cult_toml_data, TOML)?;
        let niche = NicheName::new("example-git-remote");
        let psychotropic = project_config.psychotropic()?;
        let use_thundercloud = psychotropic
            .get(niche.to_str())
            .map(NicheTriggers::use_thundercloud).flatten()
            .unwrap();
        let niches_directory = RelativePath::from("yeth-marthter");
        let default_invar_config = invar_config::from_str("", TOML)?;
        let target_dir = create_target_dir()?;

        // When
        process_niche(project_root, niches_directory, niche.clone(), use_thundercloud.clone(), default_invar_config, fs.clone(), target_dir).await?;

        // Then
        let content = fs.get_content(to_absolute_path("/workshop/clock.yaml")).await?;
        let expected = indoc! {r#"
            ---
            raising:
              - "steam"
              - "money"
        "#};
        assert_eq!(&content, expected);

        Ok(())
    }

    fn create_target_dir() -> Result<AbsolutePath> {
        let cwd = AbsolutePath::current_dir()?;
        Ok(AbsolutePath::new("target/igor", &cwd))
    }

    fn create_file_system_fixture() -> Result<impl FileSystem> {
        let toml_data = indoc! {r#"
            "CargoCult.toml" = """
            [[psychotropic.cues]]
            name = "example"

            [psychotropic.cues.use-thundercloud]
            directory = "{{PROJECT}}/example-thundercloud"
            features = ["glass"]

            [[psychotropic.cues]]
            name = "example-git-remote"

            [psychotropic.cues.use-thundercloud]
            directory = "{{PROJECT}}/non-existent"
            features = ["glass"]

            [psychotropic.cues.use-thundercloud.git-remote]
            fetch-url = "https://github.com/rustigaan/example-thundercloud.git"
            revision = "main"
            """

            [yeth-marthter.example.invar.workshop]
            "clock+config-glass.yaml.toml" = """
            write-mode = "Overwrite"

            [props]
            sweeper = "Lu Tse"
            """

            [example-thundercloud]
            "thundercloud.toml" = """
            [niche]
            name = "example"
            description = "Example thundercloud for demonstration purposes"
            """

            [example-thundercloud.cumulus.workshop]
            "clock+option-glass.yaml" = '''
            ---
            raising:
              - "steam"
              - "money"
            '''
        "#};
        trace!("TOML: [{}]", &toml_data);
        Ok(fixture::from_toml(toml_data)?)
    }
}