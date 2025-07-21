use ahash::{AHashMap, AHashSet};
use anyhow::{anyhow, bail, Result};
use std::borrow::Cow;
use std::hash::Hash;
use std::ops::Add;
use std::path::Path;
use std::pin::pin;
use log::{debug, info, trace, warn};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use tokio_stream::StreamExt;
use crate::config_model::{invar_config, InvarConfig, InvarState, NicheDescription, thundercloud_config, ThundercloudConfig, ThunderConfig, WriteMode};
use crate::path::{AbsolutePath, RelativePath, SingleComponent};
use crate::thundercloud::Thumbs::{FromBothCumulusAndInvar, FromCumulus, FromInvar};
use crate::config_model::UseThundercloudConfig;
use crate::file_system::{source_file_to_string, ConfigFormat, DirEntry, FileSystem, PathType, SourceFile, TargetFile};
use crate::thundercloud::DirectoryContext::{Project, ThunderCloud};

pub async fn process_niche<T: ThunderConfig>(thunder_config: T) -> Result<()> {
    let generation_context = GenerationContext(thunder_config);
    process_niche_in_context(&generation_context).await
}

async fn process_niche_in_context<T: ThunderConfig>(generation_context: &GenerationContext<T>) -> Result<()> {
    let thundercloud_fs = generation_context.0.thundercloud_file_system();
    let thundercloud_directory = generation_context.0.thundercloud_directory();
    let cumulus = generation_context.0.cumulus();
    let invar = generation_context.0.invar();
    let project_root = generation_context.0.project_root();
    info!("Apply: {:?} ⊕ {:?} ⇒ {:?}", cumulus, invar, project_root);
    let config = get_config(thundercloud_directory, thundercloud_fs).await?;
    let niche = config.niche();
    info!("Thundercloud: {:?}: {:?}", niche.name(), niche.description().unwrap_or(&"-".to_string()));
    debug!("Use thundercloud: {:?}", generation_context.0.use_thundercloud());
    let current_directory = RelativePath::from(".");
    let default_invar_config = config.invar_defaults();
    let default_invar_state = default_invar_config.clone_state();
    let generation_default_invar_config = generation_context.0.default_invar_config();
    let invar_defaults = generation_default_invar_config.clone_state();
    let target = generation_default_invar_config
        .target().or_else(|| default_invar_config.target())
        .map(String::clone).map(RelativePath::from).unwrap_or_else(|| current_directory.clone());
    let target = RelativePath::from(target);
    let invar_state = default_invar_state.with_invar_state(invar_defaults);
    debug!("String properties: {:?}", invar_state.string_props());
    generation_context.visit_subtree(&current_directory, &target, FromBothCumulusAndInvar, invar_state.as_ref()).await?;
    Ok(())
}

async fn get_config<FS: FileSystem>(thundercloud_directory: &AbsolutePath, fs: FS) -> Result<impl ThundercloudConfig> {
    debug!("Get config: {:?}", thundercloud_directory);
    let source_file;
    let config_format;
    let config_toml = AbsolutePath::new("thundercloud.toml", &thundercloud_directory);
    if fs.path_type(&config_toml).await == PathType::File {
        source_file = fs.open_source(config_toml).await?;
        config_format = ConfigFormat::TOML;
    } else {
        let config_yaml = AbsolutePath::new("thundercloud.yaml", &thundercloud_directory);
        source_file = fs.open_source(config_yaml).await?;
        config_format = ConfigFormat::YAML;
    }
    let body = source_file_to_string(source_file).await?;
    let config = thundercloud_config::from_str(&body, config_format)?;

    debug!("Thundercloud configuration: {config:?}");
    Ok(config)
}

#[derive(Debug, Clone, Copy)]
enum DirectoryContext { ThunderCloud, Project }

#[derive(Debug, Clone)]
struct FileLocation {
    path: AbsolutePath,
    context: DirectoryContext,
}

#[derive(Debug, Clone)]
struct Bolt {
    base_name: String,
    extension: String,
    feature_name: String,
    source: FileLocation,
    kind: BoltKind,
}

#[derive(Debug, Clone)]
enum BoltKind {
    Option,
    Fragment {
        qualifier: Option<String>
    },
    Config {
        format: ConfigFormat
    },
    Unknown {
        qualifier: Option<String>
    },
}

impl Bolt {
    fn kind_name(&self) -> &'static str {
        match self.kind {
            BoltKind::Option => "option",
            BoltKind::Config { .. } => "config",
            BoltKind::Fragment { .. } => "fragment",
            BoltKind::Unknown { .. } => "unknown",
        }
    }
    fn base_name(&self) -> String {
        self.base_name.clone()
    }
    fn extension(&self) -> String {
        self.extension.clone()
    }
    fn target_name(&self) -> String {
        let result = self.base_name().clone();
        result.add(self.extension().as_str())
    }
    fn feature_name(&self) -> String {
        self.feature_name.clone()
    }
    fn source(&self) -> &AbsolutePath {
        &self.source.path
    }
    fn context(&self) -> DirectoryContext { self.source.context }
    fn qualifier(&self) -> Option<String> {
        match &self.kind {
            BoltKind::Fragment { qualifier, .. } => qualifier.clone(),
            BoltKind::Unknown { qualifier, .. } => qualifier.clone(),
            _ => None
        }
    }
}

static CONFIG_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new("^(?<base>.*)[+]config(-(?<feature>[a-z0-9_]+|@))?(?<extension>[.][^.]*)?[.](?<format>toml|yaml)$").unwrap()
});
static BOLT_REGEX_WITH_DOT: Lazy<Regex> = Lazy::new(|| {
    Regex::new("^(?<base>.*)[+](?<bolt_type>[a-z0-9_]+)(-(?<feature>[a-z0-9_]+|@)(-(?<qualifier>[a-z0-9_]+))?)?(?<extension>[.][^.]*)$").unwrap()
});
static BOLT_REGEX_WITHOUT_DOT: Lazy<Regex> = Lazy::new(|| {
    Regex::new("^(?<base>[^.]+)[+](?<bolt_type>[a-z0-9_]+)(-(?<feature>[a-z0-9_]+|@)(-(?<qualifier>[a-z0-9_]+))?)?$").unwrap()
});
static PLAIN_FILE_REGEX_WITH_DOT: Lazy<Regex> = Lazy::new(|| {
    Regex::new("^(?<base>.*)(?<extension>[.][^.]*)").unwrap()
});
static ILLEGAL_FILE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new("^([.][.]?)?$").unwrap()
});

static FRAGMENT_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new("==== (?<bracket>(BEGIN|END) )?FRAGMENT (?<feature>[a-z0-9_]+|@)(-(?<qualifier>[a-z0-9_]+))? ====").unwrap()
});

#[derive(Clone, Copy)]
enum Thumbs {
    FromCumulus,
    FromInvar,
    FromBothCumulusAndInvar,
}

impl Thumbs {
    fn visit_cumulus(&self) -> bool {
        match self {
            FromCumulus => true,
            FromBothCumulusAndInvar => true,
            _ => false,
        }
    }
    fn visit_invar(&self) -> bool {
        match self {
            FromInvar => true,
            FromBothCumulusAndInvar => true,
            _ => false,
        }
    }
}

trait DirectoryLocation {
    fn file_system(&self) -> &impl FileSystem<DirEntryItem=impl DirEntry>;
    fn directory<'a, T: ThunderConfig>(&self, thunder_config: &'a T) -> &'a AbsolutePath;
    fn context(&self) -> DirectoryContext;
}

struct CumulusDirectoryLocation<FS: FileSystem>(FS);
struct InvarDirectoryLocation<FS: FileSystem>(FS);

impl<FS: FileSystem> DirectoryLocation for InvarDirectoryLocation<FS> {
    fn file_system(&self) -> &impl FileSystem<DirEntryItem=impl DirEntry> {
        &self.0
    }

    fn directory<'a, T: ThunderConfig>(&self, thunder_config: &'a T) -> &'a AbsolutePath {
        thunder_config.invar()
    }

    fn context(&self) -> DirectoryContext {
        Project
    }
}

impl<FS: FileSystem> DirectoryLocation for CumulusDirectoryLocation<FS> {
    fn file_system(&self) -> &impl FileSystem<DirEntryItem=impl DirEntry> {
        &self.0
    }

    fn directory<'a, T: ThunderConfig>(&self, thunder_config: &'a T) -> &'a AbsolutePath {
        thunder_config.cumulus()
    }

    fn context(&self) -> DirectoryContext {
        ThunderCloud
    }
}

struct GenerationContext<TC: ThunderConfig>(TC);

impl<TC: ThunderConfig> GenerationContext<TC> {
    async fn visit_subtree<IS: InvarState>(&self, source_dir: &RelativePath, target_dir: &RelativePath, thumbs: Thumbs, invar_config: &IS) -> Result<()> {
        let cumulus_directory_location = CumulusDirectoryLocation(self.0.thundercloud_file_system().clone());
        let (cumulus_bolts, cumulus_subdirectories) =
            self.try_visit_directory(thumbs.visit_cumulus(), &cumulus_directory_location, source_dir).await?;
        let invar_directory_location = InvarDirectoryLocation(self.0.project_file_system().clone());
        let (invar_bolts, invar_subdirectories) =
            self.try_visit_directory(thumbs.visit_invar(), &invar_directory_location, source_dir).await?;

        let bolts = combine(cumulus_bolts, invar_bolts);
        for (key, bolt_lists) in &bolts {
            debug!("Bolts entry: {:?}: {:?}", key, bolt_lists);
        }

        let new_target_option = self.generate_files(target_dir, bolts, invar_config).await?;
        let target_dir = new_target_option.as_ref().unwrap_or(target_dir);

        self.visit_subdirectories(source_dir, target_dir, cumulus_subdirectories, invar_subdirectories, invar_config).await?;

        Ok(())
    }

    async fn generate_files<IS: InvarState>(&self, directory: &RelativePath, bolts: AHashMap<String, (Vec<Bolt>, Vec<Bolt>)>, invar_config: &IS) -> Result<Option<RelativePath>> {
        let mut new_target_dir = None;
        let mut bolts = bolts;
        let mut use_config = Cow::Borrowed(invar_config);
        let mut use_target = Cow::Borrowed(directory);
        if let Some(dir_bolts) = bolts.remove(".") {
            let dir_bolt_list = combine_bolt_lists(&dir_bolts.0, &dir_bolts.1);
            let (updated_config, target_dir) = self.update_invar_config(invar_config, &dir_bolt_list).await?;
            use_config = updated_config;
            if let Some(target_dir) = target_dir {
                let mut new_target = RelativePath::from(directory.clone().parent().unwrap_or(directory).to_path_buf());
                let interpolated_target = interpolate(&target_dir, use_config.as_ref());
                new_target.push(RelativePath::from(interpolated_target));
                use_target = Cow::Owned(new_target.clone());
                new_target_dir = Some(new_target);
            }
        }
        let use_config = use_config; // No longer mutable
        let use_target = use_target; // No longer mutable
        let bolts = bolts;

        let target_directory = use_target.relative_to(self.0.project_root());
        debug!("Generate files in {:?} with config {:?}", &target_directory, &use_config);
        for (name, bolt_lists) in &bolts {
            if ILLEGAL_FILE_REGEX.is_match(name) {
                warn!("Target filename is not legal: {name:?}");
                continue;
            }
            let half_config = self.update_invar_config(use_config.as_ref(), &bolt_lists.0).await?;
            let whole_config = self.update_invar_config(half_config.0.as_ref(), &bolt_lists.1).await?;
            let (option, bolts) = self.combine_and_filter_bolt_lists(&bolt_lists.0, &bolt_lists.1);
            let target = half_config.1.or(whole_config.1).unwrap_or_else(|| name.to_string());
            let interpolated_target = interpolate(&target, whole_config.0.as_ref());
            let target_file = RelativePath::from(interpolated_target).relative_to(&target_directory);
            self.generate_file(&target_file, option, bolts, whole_config.0.as_ref()).await?;
        }
        Ok(new_target_dir)
    }

    async fn generate_file<IS: InvarState>(&self, target_path: &AbsolutePath, option: Option<Bolt>, bolts: Vec<Bolt>, invar_config: &IS) -> Result<()> {
        let option =
            if let Some(option) = option {
                option
            } else {
                debug!("Skip (only fragments): {:?}: {:?}", target_path, &bolts);
                return Ok(())
            }
            ;
        if invar_config.write_mode() == WriteMode::Ignore {
            debug!("Ignore: {:?}: {:?}: {:?}", target_path, &bolts, &invar_config);
            return Ok(())
        }
        let file_system = self.0.project_file_system();
        if let Some(target_file) = file_system.open_target(target_path.clone(), invar_config.write_mode(), invar_config.executable()).await? {
            let source = option.source();
            match option.context() {
                ThunderCloud => {
                    let fs = self.0.thundercloud_file_system();
                    let source_file = fs.open_source(source.clone()).await?;
                    self.generate_option(option, bolts, invar_config, source_file, &target_file).await?
                },
                Project => {
                    let fs = self.0.project_file_system();
                    let source_file = fs.open_source(source.clone()).await?;
                    self.generate_option(option, bolts, invar_config, source_file, &target_file).await?
                }
            }
            let mut target_file_mut = target_file;
            target_file_mut.close().await?;
        } else {
            debug!("Skip (target exists): {:?}: {:?}: {:?}", target_path, &bolts, &invar_config);
        }
        Ok(())
    }

    async fn generate_option<IS, SF, TF>(&self, option: Bolt, fragments: Vec<Bolt>, invar_config: &IS, mut source_file: SF, target_file: &TF) -> Result<()>
    where
        IS: InvarState,
        SF: SourceFile,
        TF: TargetFile
    {
        debug!("Generating option: {:?}: {:?}: {:?}", &option, &fragments, invar_config);
        while let Some(line) = source_file.next_line().await? {
            let line = interpolate(&line, invar_config);
            if let Some(captures) = FRAGMENT_REGEX.captures(&line) {
                let feature = captures.name("feature").map(|m| m.as_str().to_string()).unwrap_or("@".to_string());
                let qualifier = captures.name("qualifier").map(|m| m.as_str().to_string()).unwrap_or("".to_string());
                debug!("Found fragment: {:?}: {:?}", &feature, &qualifier);
                if let Some(bracket) = captures.name("bracket") {
                    if bracket.as_str() == "BEGIN " {
                        skip_to_end_of_fragment(&mut source_file, &feature, &qualifier).await?;
                    }
                }
                self.find_and_include_fragment(&feature, &qualifier, target_file, &fragments, invar_config).await?;
                continue;
            }
            send_to_writer(&line, target_file).await?;
        }
        Ok(())
    }

    async fn find_and_include_fragment<IS, TF>(&self, feature: &str, qualifier: &str, target_file: &TF, fragments: &Vec<Bolt>, invar_config: &IS) -> Result<()>
    where
        IS: InvarState,
        TF: TargetFile
    {
        for bolt in fragments {
            if let BoltKind::Fragment { qualifier: fragment_qualifier, .. } = &bolt.kind {
                let fragment_qualifier = fragment_qualifier.as_ref().map(ToOwned::to_owned).unwrap_or("".to_string());
                if bolt.feature_name == feature && fragment_qualifier == qualifier {
                    debug!("Found fragment to include: {:?}", bolt);
                    let source = bolt.source();
                    match bolt.context() {
                        ThunderCloud => {
                            let fs = self.0.thundercloud_file_system();
                            let source_file = fs.open_source(source.clone()).await?;
                            self.include_fragment(source_file, feature, qualifier, target_file, fragments, invar_config).await?;
                        },
                        Project => {
                            let fs = self.0.project_file_system();
                            let source_file = fs.open_source(source.clone()).await?;
                            self.include_fragment(source_file, feature, qualifier, target_file, fragments, invar_config).await?;
                        }
                    }
                    break;
                }
            }
        }
        Ok(())
    }

    async fn include_fragment<SF, TF, IS>(&self, mut source_file: SF, feature: &str, qualifier: &str, target_file: &TF, fragments: &Vec<Bolt>, invar_config: &IS) -> Result<()>
    where
        SF: SourceFile,
        TF: TargetFile,
        IS: InvarState
    {
        while let Some(line) = source_file.next_line().await? {
            let line = interpolate(&line, invar_config);
            if let Some(captures) = FRAGMENT_REGEX.captures(&line) {
                let placeholder_feature = captures.name("feature").map(|m| m.as_str().to_string()).unwrap_or("@".to_string());
                let placeholder_qualifier = captures.name("qualifier").map(|m| m.as_str().to_string()).unwrap_or("".to_string());
                debug!("Found placeholder: {:?}: {:?}", &feature, &qualifier);
                if let Some(bracket) = captures.name("bracket") {
                    if bracket.as_str() == "BEGIN " && placeholder_feature == feature && placeholder_qualifier == qualifier {
                        send_to_writer(&line, target_file).await?;
                        self.copy_to_end_of_fragment(&mut source_file, &feature, &qualifier, target_file, fragments, invar_config).await?;
                    }
                }
                return Ok(());
            }
        }
        debug!("Include fragment: placeholder not found");
        Ok(())
    }

    async fn copy_to_end_of_fragment<SF, TF, IS>(&self, lines: &mut SF, feature: &str, qualifier: &str, target_file: &TF, fragments: &Vec<Bolt>, invar_config: &IS) -> Result<()>
    where
        SF: SourceFile,
        TF: TargetFile,
        IS: InvarState
    {
        while let Some(fragment_line) = lines.next_line().await? {
            let line = interpolate(&fragment_line, invar_config);
            if let Some(captures) = FRAGMENT_REGEX.captures(&line) {
                debug!("Found inner fragment: {:?}", &captures);
                if is_matching_end(&captures, feature, qualifier) {
                    send_to_writer(&line, target_file).await?;
                    break;
                } else {
                    if let Some(bracket) = captures.name("bracket") {
                        if bracket.as_str() == "BEGIN " {
                            skip_to_end_of_fragment(lines, &feature, &qualifier).await?;
                        }
                    }
                    Box::pin(self.find_and_include_fragment(&feature, &qualifier, target_file, fragments, invar_config)).await?;
                    continue;
                }
            }
            send_to_writer(&line, target_file).await?;
        }
        Ok(())
    }

    async fn update_invar_config<'a, IS>(&self, invar_config: &'a IS, bolts: &Vec<Bolt>) -> Result<(Cow<'a, IS>, Option<String>)>
    where
        IS: InvarState,
    {
        let mut use_config = Cow::Borrowed(invar_config);
        let mut target = None;
        for bolt in bolts {
            debug!("Bolt kind: {:?}", bolt.kind_name());
            if let BoltKind::Config { format } = bolt.kind {
                let thundercloud_fs = self.0.thundercloud_file_system();
                let project_fs = self.0.project_file_system();
                debug!("Bolt context: {:?}", bolt.context());
                let bolt_invar_config_body = match bolt.context() {
                    ThunderCloud => thundercloud_fs.get_content(bolt.source().clone()).await?,
                    Project => project_fs.get_content(bolt.source().clone()).await?,
                };
                let bolt_invar_config = get_invar_config(&bolt_invar_config_body, format)?;
                if let Some(bolt_target) = bolt_invar_config.target() {
                    target = Some(bolt_target.clone());
                }
                let bolt_invar_state = bolt_invar_config.clone_state();
                debug!("Apply bolt configuration: {:?}: {:?} += {:?}", bolt.target_name(), invar_config, &bolt_invar_config);
                let new_use_config = use_config.to_owned().with_invar_state(bolt_invar_state).into_owned();
                use_config = Cow::Owned(new_use_config);
            }
        }
        debug!("Updated invar config: {:?}", use_config);
        Ok((use_config, target))
    }

    fn combine_and_filter_bolt_lists(&self, cumulus_bolts_list: &Vec<Bolt>, invar_bolts_list: &Vec<Bolt>) -> (Option<Bolt>, Vec<Bolt>) {
        let combined = combine_bolt_lists(cumulus_bolts_list, invar_bolts_list);
        self.filter_options(&combined)
    }

    fn filter_options(&self, bolt_list: &Vec<Bolt>) -> (Option<Bolt>, Vec<Bolt>) {
        let mut features = AHashSet::new();
        features.insert("@");
        for feature in self.0.use_thundercloud().features() {
            features.insert(feature);
        }
        let mut options = Vec::new();
        let mut fragments = Vec::new();
        for bolt in bolt_list {
            if features.contains(&bolt.feature_name() as &str) {
                if let BoltKind::Option = bolt.kind {
                    options.push(bolt.clone());
                } else if let BoltKind::Fragment { .. } = bolt.kind {
                    fragments.push(bolt.clone())
                }
            }
        }
        let first_option = if options.is_empty() {
            None
        } else {
            Some(options.remove(0))
        };
        (first_option, fragments)
    }

    async fn visit_subdirectories<IS>(&self, source_dir: &RelativePath, target_dir: &RelativePath, cumulus_subdirectories: AHashSet<SingleComponent>, invar_subdirectories: AHashSet<SingleComponent>, invar_config: &IS) -> Result<()>
    where
        TC: ThunderConfig,
        IS: InvarState
    {
        let mut invar_subdirectories = invar_subdirectories;
        for path in cumulus_subdirectories {
            let subdirectory_thumbs = if let Some(_) = invar_subdirectories.get(&path) {
                invar_subdirectories.remove(&path);
                FromBothCumulusAndInvar
            } else {
                FromCumulus
            };
            let path: RelativePath = path.try_into()?;
            let mut source_subdir = source_dir.clone();
            source_subdir.push(path.clone());
            let mut target_subdir = target_dir.clone();
            target_subdir.push(path);
            
            Box::pin(self.visit_subtree(&source_subdir, &target_subdir, subdirectory_thumbs, invar_config)).await?;
        }
        for path in invar_subdirectories {
            let path: RelativePath = path.try_into()?;
            let mut source_subdir = source_dir.clone();
            source_subdir.push(path.clone());
            let mut target_subdir = target_dir.clone();
            target_subdir.push(path);
            Box::pin(self.visit_subtree(&source_subdir, &target_subdir, FromInvar, invar_config)).await?;
        }
        Ok(())
    }

    async fn try_visit_directory<DL>(&self, exists: bool, directory_location: &DL, directory: &RelativePath) -> Result<(AHashMap<String, Vec<Bolt>>, AHashSet<SingleComponent>)>
    where DL: DirectoryLocation
    {
        if exists {
            let source_root = directory_location.directory(&self.0);
            let in_cumulus = directory.clone().relative_to(source_root);
            self.visit_directory(directory_location, &in_cumulus).await
        } else {
            Ok(void_subtree())
        }
    }

    async fn visit_directory<DL>(&self, directory_location: &DL, directory: &AbsolutePath) -> Result<(AHashMap<String, Vec<Bolt>>, AHashSet<SingleComponent>)>
    where DL: DirectoryLocation
    {
        trace!("Visit directory: {:?} ⇒ {:?} [{:?}]", &directory, self.0.project_root(), self.0.invar());
        let mut bolts = AHashMap::new();
        let mut subdirectories = AHashSet::new();
        let file_system = directory_location.file_system();
        if file_system.path_type(directory).await != PathType::Directory {
            debug!("Ignoring directory {:?}", directory);
            return Ok((bolts, subdirectories));
        }
        let entries = file_system.read_dir(directory).await
            .map_err(|e| anyhow!(format!("error reading {:?}: {:?}", &directory, e)))?;
        let mut entries = pin!(entries);
        while let Some(entry) = entries.next().await {
            let entry = entry?;
            trace!("Visit entry: {entry:?}");
            if entry.is_dir().await? {
                if let Some(component) = entry.path().components().last() {
                    let component = SingleComponent::try_new(Path::new(component.as_os_str()))?;
                    subdirectories.insert(component);
                }
            } else {
                let file_name = entry.file_name().to_string_lossy().into_owned();
                let source_path = RelativePath::from(file_name.as_str()).relative_to(directory);
                let source = FileLocation { path: source_path, context: directory_location.context() };
                let bolt;
                if let Some(captures) = CONFIG_REGEX.captures(&file_name) {
                    bolt = config_captures_to_bolt(captures, source)?;
                } else if let Some(captures) = BOLT_REGEX_WITH_DOT.captures(&file_name) {
                    debug!("Bolt regex with dot: {:?}", &file_name);
                    bolt = captures_to_bolt(captures, source)?;
                } else if let Some(captures) = BOLT_REGEX_WITHOUT_DOT.captures(&file_name) {
                    debug!("Bolt regex without dot: {:?}", &file_name);
                    bolt = captures_to_bolt(captures, source)?;
                } else if let Some(captures) = PLAIN_FILE_REGEX_WITH_DOT.captures(&file_name) {
                    debug!("Plain file regex with dot: {:?}", &file_name);
                    let (base_name, extension) =
                        if let (Some(b), Some(e)) = (captures.name("base"), captures.name("extension")) {
                            (b.as_str(), e.as_str())
                        } else {
                            (&*file_name, "")
                        };
                    bolt = Bolt{
                        base_name: base_name.to_string(),
                        extension: extension.to_string(),
                        feature_name: "@".to_string(),
                        source,
                        kind: BoltKind::Option
                    }
                } else {
                    debug!("Unrecognized file name: {:?}", &file_name);
                    bolt = Bolt{
                        base_name: file_name.to_string(),
                        extension: "".to_string(),
                        feature_name: "@".to_string(),
                        source,
                        kind: BoltKind::Option
                    }
                }
                debug!("Bolt: {bolt:?}");
                add(&mut bolts, &bolt.target_name(), bolt);
            }
        }
        for (target_name, bolts) in &bolts {
            let mut qualifiers = Vec::new();
            for bolt in bolts {
                let qualifier = match &bolt.kind {
                    BoltKind::Fragment { qualifier, .. } => qualifier,
                    BoltKind::Unknown { qualifier, .. } => qualifier,
                    _ => &None
                };
                if let Some(qualifier) = qualifier {
                    qualifiers.push(qualifier.to_owned());
                }
            }
            debug!("Found bolts: {:?}: {:?}: {:?}: {:?}", &directory, target_name, bolts, qualifiers);
        }
        Ok((bolts, subdirectories))
    }
}

async fn skip_to_end_of_fragment<SF>(lines: &mut SF, feature: &str, qualifier: &str) -> Result<()>
where
    SF: SourceFile
{
    while let Some(fragment_line) = lines.next_line().await? {
        if let Some(captures) = FRAGMENT_REGEX.captures(&fragment_line) {
            debug!("Found inner fragment: {:?}", &captures);
            if is_matching_end(&captures, feature, qualifier) {
                break;
            }
        }
    }
    Ok(())
}

fn is_matching_end(captures: &Captures, feature: &str, qualifier: &str) -> bool {
    if let Some(inner_bracket) = captures.name("bracket") {
        if inner_bracket.as_str() == "END " {
            let inner_feature = captures.name("feature").map(|m| m.as_str().to_string()).unwrap_or("@".to_string());
            if inner_feature != feature {
                return false;
            }
            let inner_qualifier = captures.name("qualifier").map(|m| m.as_str().to_string()).unwrap_or("".to_string());
            if inner_qualifier != qualifier {
                return false;
            }
            debug!("Found end of fragment: {:?}", &captures);
            return true;
        }
    }

    false
}

async fn send_to_writer<TF: TargetFile>(line: &str, target_file: &TF) -> Result<()> {
    trace!("Send to writer: {:?}", &line);
    target_file.write_line(line).await?;
    Ok(())
}

fn interpolate<IS: InvarState>(line: &str, invar_config: &IS) -> String {
    crate::interpolate::interpolate(line, invar_config.props().as_ref()).into_owned()
}

fn void_subtree() -> (AHashMap<String, Vec<Bolt>>, AHashSet<SingleComponent>) {
    (AHashMap::new(), AHashSet::new())
}

fn get_invar_config(body: &str, config_format: ConfigFormat) -> Result<impl InvarConfig> {
    let config = invar_config::from_str(body, config_format)?;
    debug!("Invar configuration: {config:?}");
    Ok(config)
}

fn combine(cumulus_bolts: AHashMap<String, Vec<Bolt>>, invar_bolts: AHashMap<String, Vec<Bolt>>) -> AHashMap<String, (Vec<Bolt>, Vec<Bolt>)> {
    let cumulus_keys: AHashSet<String> = cumulus_bolts.iter().map(|(k, _)| k).map(ToOwned::to_owned).collect();
    let invar_keys: AHashSet<String> = invar_bolts.iter().map(|(k, _)| k).map(ToOwned::to_owned).collect();
    let keys = cumulus_keys.union(&invar_keys);
    keys.map(
        |k: &String|
            (k.to_owned(),
             (
                 cumulus_bolts.get(k).map(ToOwned::to_owned).unwrap_or_else(Vec::new),
                 invar_bolts.get(k).map(ToOwned::to_owned).unwrap_or_else(Vec::new)
             )
            )
    ).collect()
}

fn combine_bolt_lists(cumulus_bolts_list: &Vec<Bolt>, invar_bolts_list: &Vec<Bolt>) -> Vec<Bolt> {
    let mut result = invar_bolts_list.clone();
    let mut invar_fragments = AHashSet::new();
    for invar_bolt in invar_bolts_list {
        if let BoltKind::Fragment { .. } = invar_bolt.kind {
            invar_fragments.insert((invar_bolt.feature_name(), invar_bolt.qualifier()));
        }
    }
    for cumulus_bolt in cumulus_bolts_list {
        if let BoltKind::Fragment { .. } = cumulus_bolt.kind {
            if invar_fragments.contains(&(cumulus_bolt.feature_name(), cumulus_bolt.qualifier())) {
                continue;
            }
        }
        result.push(cumulus_bolt.clone());
    }
    result
}

fn captures_to_bolt(captures: Captures, source: FileLocation) -> Result<Bolt> {
    let extension = captures.name("extension").map(|m|m.as_str().to_string()).unwrap_or("".to_string());
    let feature_name = captures.name("feature").map(|m|m.as_str().to_string()).unwrap_or("@".to_string());
    let qualifier = captures.name("qualifier").map(|m|m.as_str().to_string());
    if let (Some(base_name_orig), Some(bolt_type)) = (captures.name("base"), captures.name("bolt_type")) {
        let base_name = to_base_name(base_name_orig.as_str());
        let bolt_type = bolt_type.as_str();
        let bolt =
            if bolt_type == "option" {
                Bolt{ base_name, extension, feature_name, source, kind: BoltKind::Option}
            } else if bolt_type == "fragment" {
                Bolt{ base_name, extension, feature_name, source, kind: BoltKind::Fragment { qualifier } }
            } else {
                Bolt{ base_name, extension, feature_name, source, kind: BoltKind::Unknown { qualifier } }
            };
        Ok(bolt)
    } else {
        bail!("Internal error")
    }
}
fn config_captures_to_bolt(captures: Captures, source: FileLocation) -> Result<Bolt> {
    let extension = captures.name("extension").map(|m|m.as_str().to_string()).unwrap_or("".to_string());
    let feature_name = captures.name("feature").map(|m|m.as_str().to_string()).unwrap_or("@".to_string());
    if let (Some(base_name_orig), Some(format_match)) = (captures.name("base"), captures.name("format")) {
        let base_name = to_base_name(base_name_orig.as_str());
        let format_str = format_match.as_str();
        let format =
            if format_str == "toml" { ConfigFormat::TOML }
            else if format_str == "yaml" { ConfigFormat::YAML }
            else { bail!("Unknown config file format: {:?}", format_match) }
        ;
        let config =
            Bolt{
                base_name: base_name.to_string(),
                extension: extension.to_string(),
                feature_name: feature_name.to_string(),
                source,
                kind: BoltKind::Config { format },
            }
        ;
        Ok(config)
    } else {
        bail!("Internal error")
    }
}

fn to_base_name(base_name_orig: &str) -> String {
    let base_name = base_name_orig.to_string();
    let base_name = base_name.strip_prefix("dot_")
        .map(|stripped| ".".to_string() + stripped)
        .unwrap_or(base_name);
    base_name.strip_prefix("x_").unwrap_or(&base_name).to_string()
}

fn add<K,I>(map: &mut AHashMap<K,Vec<I>>, key: &K, item: I)
where
    K: PartialEq + Eq + Hash + Clone
{
    if let Some(existing_list) = map.get_mut(key) {
        existing_list.push(item);
    } else {
        let mut new_list = Vec::new();
        new_list.push(item);
        map.insert(key.clone(), new_list);
    }
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use test_log::test;
    use crate::config_model::{project_config, NicheTriggers, ProjectConfig, PsychotropicConfig};
    use crate::file_system::ConfigFormat::TOML;
    use crate::file_system::fixture;
    use crate::path::test_utils::to_absolute_path;
    use super::*;

    #[test(tokio::test)]
    async fn test_process_complex_niche() -> Result<()> {
        // Given
        let thundercloud_toml = indoc! {r#"
            [example-thundercloud]
            "thundercloud.toml" = """
            [niche]
            name = "example"
            description = "Example thundercloud for demonstration purposes"

            [invar-defaults]
            write-mode = "Overwrite"
            interpolate = true

            [invar-defaults.props]
            milk-man = "Ronny Soak"
            alter-ego = "Lobsang"
            """

            [example-thundercloud.cumulus.workshop]
            "clock+fragment-glass-spring.yaml" = """
            ---
            spring:
              material: glass
              delicate: false
              number-of-coils: 3
            """
            "clock+option-glass.yaml" = '''
            ---
            sweeper: "{{alter-ego}}"
            raising:
              - "steam"
              - "money"
            # ==== BEGIN FRAGMENT glass-spring ====
              - "replaced-by-fragment"
            # ==== END FRAGMENT glass-spring ====
              - "to the occasion"
            '''
            "clock+config-glass.yaml.toml" = """
            write-mode = "WriteNew"
            """
        "#};
        let project_toml = indoc! {r#"
            "CargoCult.toml" = '''
            [[psychotropic.cues]]
            name = "example"
            use-thundercloud = { directory = "{{PROJECT}}/example-thundercloud", on-incoming = "Update", features = ["glass", "bash_config", "kermie"], invar-defaults = { props = { marthter = "Jeremy", buyer = "Myra LeJean", milk-man = "Kaos" }, target = "bad-schuschein" } }
            '''

            [yeth-marthter.example.invar]
            "dot_+config.toml" = """
            target = "Ankh-Morpork"
            """

            [yeth-marthter.example.invar.workshop]
            "dot_+config.toml" = """
            target = "{{marthter}}"
            """
            "clock+config-glass.yaml.toml" = """
            write-mode = "Overwrite"

            [props]
            sweeper = "Lu Tse"
            """
            "clock+fragment-glass-spring.yaml" = '''
            # ==== BEGIN FRAGMENT glass-spring ====
            ---
            spring:
              material: glass
              delicate: true
              number-of-coils: 17
            raising:
              - "expectations"
            # ==== END FRAGMENT glass-spring ====
            '''
        "#};

        // When
        let result_file_path = to_absolute_path("/Ankh-Morpork/Jeremy/clock.yaml");
        let result_body = test_process_niche(thundercloud_toml, project_toml, result_file_path).await?;

        // Then
        let expected_result = indoc! {r#"
            ---
            sweeper: "Lobsang"
            raising:
              - "steam"
              - "money"
            # ==== BEGIN FRAGMENT glass-spring ====
            ---
            spring:
              material: glass
              delicate: true
              number-of-coils: 17
            raising:
              - "expectations"
            # ==== END FRAGMENT glass-spring ====
              - "to the occasion"
        "#};
        assert_eq!(&result_body, expected_result);

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_config_without_extension() -> Result<()> {
        // Given
        let thundercloud_toml = indoc! {r#"
            [example-thundercloud]
            "thundercloud.toml" = """
            [niche]
            name = "example"
            description = "Example thundercloud for demonstration purposes"

            [invar-defaults.props]
            alter-ego = "Lobsang"
            """

            [example-thundercloud.cumulus.workshop]
            "x_x_x+option-kermie" = '''
            Miss Piggy
            Sweeper: {{sweeper}}
            Alter ego: {{alter-ego}}
            '''
        "#};
        let project_toml = indoc! {r#"
            "CargoCult.toml" = '''
            [[psychotropic.cues]]
            name = "example"
            use-thundercloud = { directory = "{{PROJECT}}/example-thundercloud", on-incoming = "Update", features = ["glass", "bash_config", "kermie"], invar-defaults = { props = { marthter = "Jeremy", buyer = "Myra LeJean", milk-man = "Kaos" } } }
            '''

            [yeth-marthter.example.invar.workshop]
            "x_x_x+config-kermie.toml" = '''
            [props]
            sweeper = "Lu Tse"
            '''
        "#};

        // When
        let result_file_path = to_absolute_path("/workshop/x_x");
        let result_body = test_process_niche(thundercloud_toml, project_toml, result_file_path).await?;

        // Then
        let expected_result = indoc! {r#"
            Miss Piggy
            Sweeper: Lu Tse
            Alter ego: Lobsang
        "#};
        assert_eq!(&result_body, expected_result);

        Ok(())
    }

    async fn test_process_niche(thundercloud_toml: &str, project_toml: &str, result_file_path: AbsolutePath) -> Result<String> {
        // Given
        let thundercloud_fs = fixture::from_toml(thundercloud_toml)?;
        let project_fs = fixture::from_toml(project_toml)?;
        let project_config = create_project_config(project_fs.clone()).await?;
        let niche_triggers = get_niche_triggers(&project_config)?;
        let default_invar_config = niche_triggers.use_thundercloud().unwrap().invar_defaults().into_owned();
        let project_root = AbsolutePath::root();
        let thundercloud_directory = to_absolute_path("/example-thundercloud");
        let invar_directory = to_absolute_path("/yeth-marthter/example/invar");
        let thunder_config = niche_triggers.use_thundercloud().unwrap().new_thunder_config(default_invar_config, thundercloud_fs.clone(), thundercloud_directory.clone(), project_fs.clone(), invar_directory.clone(), project_root.clone());
        let generation_context = GenerationContext(thunder_config);

        // When
        let result = process_niche_in_context(&generation_context).await;

        // Then
        result?;

        let fs = generation_context.0.project_file_system();

        fs.get_content(result_file_path).await
    }

    async fn create_project_config<FS: FileSystem>(fs: FS) -> Result<impl ProjectConfig> {
        let source_file = fs.open_source(to_absolute_path("/CargoCult.toml")).await?;
        let body = body(source_file).await?;
        Ok(project_config::from_str(&body, TOML)?)
    }

    fn get_niche_triggers<PC: ProjectConfig>(project_config: &PC) -> Result<impl NicheTriggers + '_> {
        let psychotropic_config = project_config.psychotropic()?;
        let niche_triggers = psychotropic_config.get("example");
        niche_triggers.map(|nt| nt.clone()).ok_or_else(|| anyhow!("Niche not found: 'example'"))
    }

    // Utilities

    async fn body<SF: SourceFile>(mut source_file: SF) -> Result<String> {
        let mut body = Vec::new();
        while let Some(line) = source_file.next_line().await? {
            body.push(line);
        }
        Ok(body.join("\n"))
    }
}
