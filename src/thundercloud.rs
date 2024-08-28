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
use serde_yaml::Value;
use tokio_stream::StreamExt;
use crate::config_model::{invar_config, InvarConfig, NicheDescription, thundercloud_config, ThundercloudConfig, ThunderConfig, WriteMode};
use crate::path::{AbsolutePath, RelativePath, SingleComponent};
use crate::thundercloud::Thumbs::{FromBothCumulusAndInvar, FromCumulus, FromInvar};
use crate::config_model::UseThundercloudConfig;
use crate::file_system::{source_file_to_string, DirEntry, FileSystem, SourceFile, TargetFile};
use crate::thundercloud::DirectoryContext::{Project, ThunderCloud};

pub async fn process_niche<T: ThunderConfig>(thunder_config: T) -> Result<()> {
    let generation_context = GenerationContext(thunder_config);
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
    let invar_config = config.invar_defaults();
    let invar_defaults = generation_context.0.use_thundercloud().invar_defaults().into_owned();
    let invar_config = invar_config.with_invar_config(invar_defaults);
    debug!("String properties: {:?}", invar_config.string_props());
    generation_context.visit_subtree(&current_directory, FromBothCumulusAndInvar, invar_config.as_ref()).await?;
    Ok(())
}

async fn get_config<FS: FileSystem>(thundercloud_directory: &AbsolutePath, fs: FS) -> Result<impl ThundercloudConfig> {
    let mut config_path = thundercloud_directory.clone();
    config_path.push("thundercloud.yaml");
    info!("Config path: {config_path:?}");

    let source_file = fs.open_source(config_path).await?;
    let body = source_file_to_string(source_file).await?;
    let config = get_config_from_string(body)?;
    debug!("Thundercloud configuration: {config:?}");
    Ok(config)
}

fn get_config_from_string(body: String) -> Result<impl ThundercloudConfig> {
    let config = thundercloud_config::from_string(body)?;
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
    Config,
    Unknown {
        qualifier: Option<String>
    },
}

impl Bolt {
    fn kind_name(&self) -> &'static str {
        match self.kind {
            BoltKind::Option => "option",
            BoltKind::Config => "config",
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

static PLACE_HOLDER_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new("\\$\\{([^{}]*)}").unwrap()
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
    async fn visit_subtree<IC>(&self, directory: &RelativePath, thumbs: Thumbs, invar_config: &IC) -> Result<()>
    where IC: InvarConfig
    {
        let cumulus_directory_location = CumulusDirectoryLocation(self.0.thundercloud_file_system().clone());
        let (cumulus_bolts, cumulus_subdirectories) =
            self.try_visit_directory(thumbs.visit_cumulus(), &cumulus_directory_location, directory).await?;
        let invar_directory_location = InvarDirectoryLocation(self.0.project_file_system().clone());
        let (invar_bolts, invar_subdirectories) =
            self.try_visit_directory(thumbs.visit_invar(), &invar_directory_location, directory).await?;

        let bolts = combine(cumulus_bolts, invar_bolts);
        for (key, bolt_lists) in &bolts {
            debug!("Bolts entry: {:?}: {:?}", key, bolt_lists);
        }

        self.generate_files(&directory, bolts, invar_config).await?;

        self.visit_subdirectories(directory, cumulus_subdirectories, invar_subdirectories, invar_config).await?;

        Ok(())
    }

    async fn generate_files<IC>(&self, directory: &RelativePath, bolts: AHashMap<String, (Vec<Bolt>, Vec<Bolt>)>, invar_config: &IC) -> Result<()>
    where IC: InvarConfig
    {
        let mut bolts = bolts;
        let mut use_config = Cow::Borrowed(invar_config);
        if let Some(dir_bolts) = bolts.remove(".") {
            let (_, dir_bolt_list) = self.combine_and_filter_bolt_lists(&dir_bolts.0, &dir_bolts.1);
            use_config = self.update_invar_config(invar_config, &dir_bolt_list).await?;
        }
        let bolts = bolts;

        let target_directory = directory.relative_to(self.0.project_root());
        debug!("Generate files in {:?} with config {:?}", &target_directory, &use_config);
        for (name, bolt_lists) in &bolts {
            if ILLEGAL_FILE_REGEX.is_match(name) {
                warn!("Target filename is not legal: {name:?}");
                continue;
            }
            let target_file = RelativePath::from(name as &str).relative_to(&target_directory);
            let half_config = self.update_invar_config(invar_config, &bolt_lists.0).await?;
            let whole_config = self.update_invar_config(half_config.as_ref(), &bolt_lists.1).await?;
            let (option, bolts) = self.combine_and_filter_bolt_lists(&bolt_lists.0, &bolt_lists.1);
            let file_system = self.0.project_file_system();
            self.generate_file(&file_system, &target_file, option, bolts, whole_config.as_ref()).await?;
        }
        Ok(())
    }

    async fn generate_file<FS, IC>(&self, file_system: &FS, target_path: &AbsolutePath, option: Option<Bolt>, bolts: Vec<Bolt>, invar_config: &IC) -> Result<()>
    where
        FS: FileSystem,
        IC: InvarConfig
    {
        if bolts.is_empty() {
            debug!("Skip: {:?}: {:?}", target_path, &bolts);
            return Ok(())
        }
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
        if let Some(target_file) = file_system.open_target(target_path.clone(), invar_config.write_mode()).await? {
            self.generate_option(file_system, option, bolts, invar_config, &target_file).await?;
            let mut target_file_mut = target_file;
            target_file_mut.close().await?;
        } else {
            debug!("Skip (target exists): {:?}: {:?}: {:?}", target_path, &bolts, &invar_config);
        }
        Ok(())
    }

    async fn generate_option<FS, IC, TF>(&self, file_system: &FS, option: Bolt, fragments: Vec<Bolt>, invar_config: &IC, target_file: &TF) -> Result<()>
    where
        FS: FileSystem,
        IC: InvarConfig,
        TF: TargetFile
    {
        debug!("Generating option: {:?}: {:?}: {:?}", &option, &fragments, invar_config);
        let source = option.source();
        let mut source_file = file_system.open_source(source.clone()).await?;
        while let Some(line) = source_file.next_line().await? {
            if let Some(captures) = FRAGMENT_REGEX.captures(&line) {
                let feature = captures.name("feature").map(|m| m.as_str().to_string()).unwrap_or("@".to_string());
                let qualifier = captures.name("qualifier").map(|m| m.as_str().to_string()).unwrap_or("".to_string());
                debug!("Found fragment: {:?}: {:?}", &feature, &qualifier);
                if let Some(bracket) = captures.name("bracket") {
                    if bracket.as_str() == "BEGIN " {
                        skip_to_end_of_fragment(&mut source_file, &feature, &qualifier).await?;
                    }
                }
                self.find_and_include_fragment(file_system, &feature, &qualifier, target_file, &fragments, invar_config).await?;
                continue;
            }
            send_to_writer(&line, invar_config, target_file).await?;
        }
        Ok(())
    }

    async fn find_and_include_fragment<FS, IC, TF>(&self, file_system: &FS, feature: &str, qualifier: &str, target_file: &TF, fragments: &Vec<Bolt>, invar_config: &IC) -> Result<()>
    where
        FS: FileSystem,
        IC: InvarConfig,
        TF: TargetFile
    {
        for bolt in fragments {
            if let BoltKind::Fragment { qualifier: fragment_qualifier, .. } = &bolt.kind {
                let fragment_qualifier = fragment_qualifier.as_ref().map(ToOwned::to_owned).unwrap_or("".to_string());
                if bolt.feature_name == feature && fragment_qualifier == qualifier {
                    debug!("Found fragment to include: {:?}", bolt);
                    self.include_fragment(file_system, bolt, feature, qualifier, target_file, fragments, invar_config).await?;
                    break;
                }
            }
        }
        Ok(())
    }

    async fn include_fragment<FS, TF, IC>(&self, file_system: &FS, fragment: &Bolt, feature: &str, qualifier: &str, target_file: &TF, fragments: &Vec<Bolt>, invar_config: &IC) -> Result<()>
    where
        FS: FileSystem,
        TF: TargetFile,
        IC: InvarConfig
    {
        let source = fragment.source().clone();
        let mut source_file = file_system.open_source(source).await?;
        while let Some(line) = source_file.next_line().await? {
            if let Some(captures) = FRAGMENT_REGEX.captures(&line) {
                let placeholder_feature = captures.name("feature").map(|m| m.as_str().to_string()).unwrap_or("@".to_string());
                let placeholder_qualifier = captures.name("qualifier").map(|m| m.as_str().to_string()).unwrap_or("".to_string());
                debug!("Found placeholder: {:?}: {:?}", &feature, &qualifier);
                if let Some(bracket) = captures.name("bracket") {
                    if bracket.as_str() == "BEGIN " && placeholder_feature == feature && placeholder_qualifier == qualifier {
                        send_to_writer(&line, invar_config, target_file).await?;
                        self.copy_to_end_of_fragment(file_system, &mut source_file, &feature, &qualifier, target_file, fragments, invar_config).await?;
                    }
                }
                return Ok(());
            }
        }
        debug!("Include fragment: placeholder not found");
        Ok(())
    }

    async fn copy_to_end_of_fragment<FS, SF, TF, IC>(&self, file_system: &FS, lines: &mut SF, feature: &str, qualifier: &str, target_file: &TF, fragments: &Vec<Bolt>, invar_config: &IC) -> Result<()>
    where
        FS: FileSystem,
        SF: SourceFile,
        TF: TargetFile,
        IC: InvarConfig
    {
        while let Some(fragment_line) = lines.next_line().await? {
            if let Some(captures) = FRAGMENT_REGEX.captures(&fragment_line) {
                debug!("Found inner fragment: {:?}", &captures);
                if is_matching_end(&captures, feature, qualifier) {
                    send_to_writer(&fragment_line, invar_config, target_file).await?;
                    break;
                } else {
                    if let Some(bracket) = captures.name("bracket") {
                        if bracket.as_str() == "BEGIN " {
                            skip_to_end_of_fragment(lines, &feature, &qualifier).await?;
                        }
                    }
                    Box::pin(self.find_and_include_fragment(file_system, &feature, &qualifier, target_file, fragments, invar_config)).await?;
                    continue;
                }
            }
            send_to_writer(&fragment_line, invar_config, target_file).await?;
        }
        Ok(())
    }

    async fn update_invar_config<'a, IC>(&self, invar_config: &'a IC, bolts: &Vec<Bolt>) -> Result<Cow<'a, IC>>
    where
        IC: InvarConfig,
    {
        let mut use_config = Cow::Borrowed(invar_config);
        for bolt in bolts {
            debug!("Bolt kind: {:?}", bolt.kind_name());
            if let BoltKind::Config = bolt.kind {
                let thundercloud_fs = self.0.thundercloud_file_system();
                let project_fs = self.0.thundercloud_file_system();
                let bolt_invar_config_body = match bolt.context() {
                    ThunderCloud => get_invar_config_body(bolt.source(), &thundercloud_fs).await?,
                    Project => get_invar_config_body(bolt.source(), &project_fs).await?,
                };
                let bolt_invar_config = get_invar_config(bolt_invar_config_body)?;
                debug!("Apply bolt configuration: {:?}: {:?} += {:?}", bolt.target_name(), invar_config, &bolt_invar_config);
                let new_use_config = use_config.to_owned().with_invar_config(bolt_invar_config).into_owned();
                use_config = Cow::Owned(new_use_config);
            }
        }
        debug!("Updated invar config: {:?}", use_config);
        Ok(use_config)
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

    async fn visit_subdirectories<IC>(&self, directory: &RelativePath, cumulus_subdirectories: AHashSet<SingleComponent>, invar_subdirectories: AHashSet<SingleComponent>, invar_config: &IC) -> Result<()>
    where
        TC: ThunderConfig,
        IC: InvarConfig
    {
        let mut invar_subdirectories = invar_subdirectories;
        for path in cumulus_subdirectories {
            let subdirectory_thumbs = if let Some(_) = invar_subdirectories.get(&path) {
                invar_subdirectories.remove(&path);
                FromBothCumulusAndInvar
            } else {
                FromCumulus
            };
            let mut subdirectory = directory.clone();
            let path: RelativePath = path.try_into()?;
            subdirectory.push(path);
            Box::pin(self.visit_subtree(&subdirectory, subdirectory_thumbs, invar_config)).await?;
        }
        for path in invar_subdirectories {
            let mut subdirectory = directory.clone();
            let path: RelativePath = path.try_into()?;
            subdirectory.push(path);
            Box::pin(self.visit_subtree(&subdirectory, FromInvar, invar_config)).await?;
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
                if let Some(captures) = BOLT_REGEX_WITH_DOT.captures(&file_name) {
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

async fn send_to_writer<IC, TF>(line: &str, invar_config: &IC, target_file: &TF) -> Result<()>
where
    IC: InvarConfig,
    TF: TargetFile
{
    let mut line = interpolate(&line, invar_config);
    debug!("Send to writer: {:?}", line);
    line.push('\n');
    target_file.write_line(line).await?;
    Ok(())
}

fn interpolate<IC: InvarConfig>(line: &str, invar_config: &IC) -> String {
    let mut result = String::new();
    let properties = invar_config.props();
    let replacements = properties.as_ref();
    let mut pos = 0;
    for placeholder in PLACE_HOLDER_REGEX.captures_iter(line) {
        if let (Some(extent), Some(expression)) = (placeholder.get(0), placeholder.get(1)) {
            let expression = expression.as_str().to_string();
            if let Some(Value::String(replacement)) = replacements.get(&expression) {
                debug!("Found: [{:?}] -> [{:?}]", &expression, &replacement);
                result.push_str(&line[pos..extent.start()]);
                result.push_str(replacement);
            } else {
                debug!("Not found: [{:?}]", &expression);
                result.push_str(&line[pos..extent.end()])
            }
            pos = extent.end();
        } else {
            debug!("No expression: [{:?}]", &line[pos..]);
            result.push_str(&line[pos..]);
            continue;
        }
    }
    result.push_str(&line[pos..line.len()]);
    result
}

fn void_subtree() -> (AHashMap<String, Vec<Bolt>>, AHashSet<SingleComponent>) {
    (AHashMap::new(), AHashSet::new())
}

async fn get_invar_config_body<FS: FileSystem>(source: &AbsolutePath, fs: &FS) -> Result<String> {
    info!("Config path: {source:?}");

    let source_file = fs.open_source(source.clone()).await?;
    source_file_to_string(source_file).await
}

fn get_invar_config(body: String) -> Result<impl InvarConfig> {
    let config = invar_config::from_string(body)?;
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
        let base_name = base_name_orig.as_str().to_string();
        let base_name = base_name.strip_prefix("dot_")
            .map(|stripped| ".".to_string() + stripped)
            .unwrap_or(base_name);
        let base_name = base_name.strip_prefix("x_").unwrap_or(&base_name).to_string();
        let bolt_type = bolt_type.as_str();
        let bolt =
            if bolt_type == "option" {
                Bolt{ base_name, extension, feature_name, source, kind: BoltKind::Option}
            } else if bolt_type == "config" {
                create_config(base_name_orig.as_str(), &base_name, &extension, &feature_name, source)
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

fn create_config(base_name_orig: &str, base_name: &str, extension: &str, feature_name: &str, source: FileLocation) -> Bolt {
    if base_name_orig == "dot" && extension == ".yaml" {
        Bolt{
            base_name: ".".to_string(),
            extension: "".to_string(),
            feature_name: feature_name.to_string(),
            source,
            kind: BoltKind::Config,
        }
    } else {
        Bolt{
            base_name: base_name.to_string(),
            extension: extension.to_string(),
            feature_name: feature_name.to_string(),
            source,
            kind: BoltKind::Config,
        }
    }
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
    use stringreader::StringReader;
    use test_log::test;
    use crate::config_model::niche_config;
    use crate::config_model::niche_config::NicheConfig;
    use crate::file_system::fixture_file_system;
    use crate::path::test_utils::to_absolute_path;
    use super::*;

    #[test(tokio::test)]
    async fn test_process_niche() -> Result<()> {
        // Given
        let thundercloud_fs = create_thundercloud_file_system_fixture()?.read_only();
        let project_fs = create_project_file_system_fixture()?;
        let niche_configuration = create_niche_config(project_fs.clone()).await?;
        let thunder_config = create_thunder_config(&niche_configuration, thundercloud_fs.clone(), project_fs.clone()).await?;

        // When
        let result = process_niche(thunder_config).await;

        // Then
        result
    }

    async fn create_niche_config<FS: FileSystem>(fs: FS) -> Result<impl NicheConfig> {
        let source_file = fs.open_source(to_absolute_path("/yeth-mathtur/example/igor-thettingth.yaml")).await?;
        let body = body(source_file).await?;
        Ok(niche_config::test_utils::from_string(body)?)
    }

    async fn create_thunder_config<'a, NC: NicheConfig, TFS: FileSystem + 'a, PFS: FileSystem + 'a>(niche_configuration: &'a NC, thundercloud_fs: TFS, project_fs: PFS) -> Result<impl ThunderConfig + 'a> {
        let project_root = to_absolute_path("/");
        let thundercloud_directory = to_absolute_path("/example-thundercloud");
        let invar_directory = to_absolute_path("/yeth-mathtur/example/invar");
        let thunder_config = niche_configuration.new_thunder_config(thundercloud_fs, thundercloud_directory, project_fs, invar_directory, project_root);
        Ok(thunder_config)
    }

    // Utilities

    async fn body<SF: SourceFile>(mut source_file: SF) -> Result<String> {
        let mut body = Vec::new();
        while let Some(line) = source_file.next_line().await? {
            body.push(line);
        }
        Ok(body.join("\n"))
    }

    fn create_thundercloud_file_system_fixture() -> Result<impl FileSystem> {
        let yaml = indoc! {r#"
                example-thundercloud:
                    thundercloud.yaml: |
                        ---
                        niche:
                          name: example
                          description: Example thundercloud for demonstration purposes
                        invar-defaults:
                          write-mode: Overwrite
                          interpolate: true
                          props:
                            milk-man: Ronny Soak
                            alter-ego: Lobsang
                    cumulus:
                        clock+option-grlass.yaml: |
                            ---
                            sweeper: "${sweeper}"
                            raising:
                              - "steam"
                              - "money"
                            # ==== BEGIN FRAGMENT glass-spring ====
                              - "replaced-by-fragment"
                            # ==== END FRAGMENT glass-spring ====
                              - "to the occasion"
            "#};
        trace!("YAML: [{}]", &yaml);

        let yaml_source = StringReader::new(yaml);
        Ok(fixture_file_system(yaml_source)?)
    }

    fn create_project_file_system_fixture() -> Result<impl FileSystem> {
        let yaml = indoc! {r#"
                yeth-mathtur:
                    example:
                        igor-thettingth.yaml: |
                            ---
                            use-thundercloud:
                              directory: "{{PROJECT}}/example-thundercloud"
                              on-incoming: Update
                              features:
                                - glass
                                - bash_config
                                - kermie
                              invar-defaults:
                                props:
                                  mathtur: Jeremy
                                  buyer: Myra LeJean
                                  milk-man: Kaos
                        invar:
                            workshop:
                                bench:
                                    press+option-free: |
                                        #!/usr/bin/false

                                        echo 'Hello, world!'
                                clock+config-glass.yaml: |
                                    write-mode: Overwrite
                                    props:
                                      sweeper: Lu Tse
                                clock+fragment-glass-spring.yaml: |
                                    # ==== BEGIN FRAGMENT glass-spring ====
                                    ---
                                    spring:
                                      material: glass
                                      delicate: true
                                      number-of-coils: 17
                                    raising:
                                      - "expectations"
                                    # ==== END FRAGMENT glass-spring ====
                                README+fragment-@-details.md: |
                                    ## Details

                                    The details of this project.

                                    * Mathtur: ${mathtur}
                                    * Buyer: ${buyer}
                                    * Milk man: ${milk-man}
                                    * Undefined: ${undefined}
            "#};
        trace!("YAML: [{}]", &yaml);

        let yaml_source = StringReader::new(yaml);
        Ok(fixture_file_system(yaml_source)?)
    }
}
