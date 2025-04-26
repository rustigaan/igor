use crate::config_model::invar_config::*;
use std::borrow::Cow;
use std::sync::LazyLock;
use ahash::AHashMap;
use anyhow::Result;
use log::debug;
use serde::{Deserialize, Serialize};
use toml::{Table, Value};
use crate::file_system::ConfigFormat;

macro_rules! invar_struct {
    ($v:vis $name:ident $([$($derive:ident),+])? { $($x:ident : $y:ty),* $(,)? }) => {
        #[derive(Deserialize,Serialize,Debug,Clone$($(,$derive)+)?)]
        #[serde(rename_all = "kebab-case")]
        $v struct $name {
            write_mode: Option<WriteMode>,
            executable: Option<bool>,
            interpolate: Option<bool>,
            props: Option<Table>,
            $(
            $x: $y
            ),*
        }
    };
}

invar_struct! [pub InvarConfigData[Default] {
//    target: Option<String>
}];

invar_struct! [pub InvarConfigState {}];

static EMPTY_INVAR_CONFIG_DATA: LazyLock<InvarConfigData> = LazyLock::new(|| {
    let default_invar_config_data = InvarConfigData::default();
    InvarConfigData {
        props: Some(Table::new()),
        ..default_invar_config_data
    }
});

impl InvarConfigData {
    pub fn new() -> InvarConfigData {
        EMPTY_INVAR_CONFIG_DATA.clone()
    }
}

#[cfg(test)]
mod test_invar_config_data {
    use super::*;

    #[test]
    fn create_empty_invar_config_data() {
        let empty_invar_config_data = InvarConfigData::new();
        assert_eq!(empty_invar_config_data.write_mode, None);
        assert_eq!(empty_invar_config_data.interpolate, None);
        assert_eq!(empty_invar_config_data.props, Some(Table::new()));
    }
}

impl InvarConfig for InvarConfigData {
    fn from_str(body: &str, config_format: ConfigFormat) -> Result<Self> {
        let invar_config: InvarConfigData = match config_format {
            ConfigFormat::TOML => toml::from_str(body)?,
            ConfigFormat::YAML => {
                let result = serde_yaml::from_str(body)?;

                #[cfg(test)]
                crate::test_utils::log_toml("Invar Config", &result)?;

                result
            }
        };
        Ok(invar_config)
    }

    fn with_invar_config<I: InvarConfig>(&self, invar_config: I) -> Cow<Self> {
        let dirty = false;
        let (write_mode, dirty) = merge_property(self.write_mode, invar_config.write_mode_option(), dirty);
        debug!("Write mode: {:?} -> {:?} ({:?})", self.write_mode, &write_mode, dirty);
        let (executable, dirty) = merge_property(self.executable, invar_config.executable_option(), dirty);
        debug!("Executable: {:?} -> {:?} ({:?})", self.executable, &executable, dirty);
        let (interpolate, dirty) = merge_property(self.interpolate, invar_config.interpolate_option(), dirty);
        debug!("Interpolate: {:?} -> {:?} ({:?})", self.interpolate, &interpolate, dirty);
        let (props, dirty) = merge_props(&self.props, &invar_config.props_option(), dirty);
        debug!("Props ({:?})", dirty);
        if dirty {
            Cow::Owned(InvarConfigData { write_mode, executable, interpolate, props: Some(props.into_owned()) })
        } else {
            Cow::Borrowed(self)
        }
    }

    fn with_write_mode_option(&self, write_mode: Option<WriteMode>) -> Cow<Self> {
        let invar_config = InvarConfigData { write_mode, executable: None, interpolate: None, props: None };
        self.with_invar_config(invar_config)
    }

    fn with_write_mode(&self, write_mode: WriteMode) -> Cow<Self> {
        self.with_write_mode_option(Some(write_mode))
    }

    fn write_mode(&self) -> WriteMode {
        self.write_mode.unwrap_or(WriteMode::Overwrite)
    }

    fn write_mode_option(&self) -> Option<WriteMode> {
        self.write_mode
    }

    fn with_executable_option(&self, executable: Option<bool>) -> Cow<Self> {
        let invar_config = InvarConfigData { write_mode: None, executable, interpolate: None, props: None };
        self.with_invar_config(invar_config)
    }

    fn with_executable(&self, executable: bool) -> Cow<Self> {
        self.with_executable_option(Some(executable))
    }

    fn executable(&self) -> bool {
        self.executable.unwrap_or(false)
    }

    fn executable_option(&self) -> Option<bool> {
        self.executable
    }

    fn with_interpolate_option(&self, interpolate: Option<bool>) -> Cow<Self> {
        let invar_config = InvarConfigData { write_mode: None, executable: None, interpolate, props: None };
        self.with_invar_config(invar_config)
    }

    fn with_interpolate(&self, interpolate: bool) -> Cow<Self> {
        self.with_interpolate_option(Some(interpolate))
    }

    fn interpolate(&self) -> bool {
        self.interpolate.unwrap_or(true)
    }

    fn interpolate_option(&self) -> Option<bool> {
        self.interpolate
    }

    fn with_props_option(&self, props: Option<Table>) -> Cow<Self> {
        let invar_config = InvarConfigData { write_mode: None, executable: None, interpolate: None, props };
        self.with_invar_config(invar_config)
    }

    fn with_props(&self, props: Table) -> Cow<Self> {
        self.with_props_option(Some(props))
    }

    fn props(&self) -> Cow<Table> {
        self.props.as_ref().map(Cow::Borrowed).unwrap_or(Cow::Owned(Table::new()))
    }

    fn props_option(&self) -> &Option<Table> {
        &self.props
    }

    fn string_props(&self) -> AHashMap<String,String> {
        to_string_map(self.props().as_ref())
    }
}

fn merge_property<T: Copy + Eq>(current_value_option: Option<T>, new_value_option: Option<T>, dirty: bool) -> (Option<T>, bool) {
    match (current_value_option, new_value_option) {
        (Some(current_value), Some(new_value)) =>
            if new_value == current_value {
                (current_value_option, dirty)
            } else {
                (new_value_option, true)
            },
        (None, Some(_)) => (new_value_option, true),
        (_, _) => (current_value_option, dirty)
    }
}

fn merge_props<'a>(current_props_option: &'a Option<Table>, new_props_option: &'a Option<Table>, dirty: bool) -> (Cow<'a, Table>, bool) {
    if let Some(current_props) = current_props_option {
        if let Some(new_props) = new_props_option {
            for (k, v) in new_props {
                if current_props.get(k) != Some(v) {
                    let mut result = current_props.clone();
                    let new_props = new_props.clone();
                    result.extend(new_props);
                    return (Cow::Owned(result), true)
                }
            }
            (Cow::Borrowed(current_props), dirty)
        } else {
            (Cow::Borrowed(current_props), dirty)
        }
    } else if let Some(new_props) = new_props_option {
        (Cow::Borrowed(new_props), true)
    } else {
        (Cow::Owned(Table::new()), true)
    }
}

fn to_string_map(props: &Table) -> AHashMap<String,String> {
    props.iter().map(to_strings).filter(Option::is_some).map(Option::unwrap).collect()
}

fn to_strings(entry: (&String, &Value)) -> Option<(String, String)> {
    if let (key, Value::String(val)) = entry {
        Some((key.to_owned(), val.to_owned()))
    } else {
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use super::super::serde_test_utils::insert_entry;
    use crate::config_model::WriteMode::*;
    use test_log::test;

    // Write mode

    #[test]
    fn with_write_mode_from_none_to_something() {
        let invar_config = new_invar_config();
        assert_eq!(invar_config.write_mode_option(), None);
        let updated = invar_config.with_write_mode(Overwrite);
        assert_owned(&updated);
        assert_eq!(updated.write_mode_option(), Some(Overwrite));
    }

    #[test]
    fn with_write_mode_from_none_to_some_thing() {
        let invar_config = new_invar_config();
        assert_eq!(invar_config.write_mode_option(), None);
        let updated = invar_config.with_write_mode_option(Some(Overwrite));
        assert_owned(&updated);
        assert_eq!(updated.write_mode_option(), Some(Overwrite));
    }

    #[test]
    fn with_write_mode_from_none_to_none() {
        let invar_config = new_invar_config();
        assert_eq!(invar_config.write_mode_option(), None);
        let updated = invar_config.with_write_mode_option(None);
        assert_borrowed(&updated);
        assert_eq!(updated.write_mode_option(), None);
    }

    #[test]
    fn with_write_mode_from_something_to_something_same() {
        let invar_config = new_invar_config().with_write_mode(Ignore).into_owned();
        assert_eq!(invar_config.write_mode_option(), Some(Ignore));
        let updated = invar_config.with_write_mode(Ignore);
        assert_borrowed(&updated);
        assert_eq!(updated.write_mode_option(), Some(Ignore));
    }

    #[test]
    fn with_write_mode_from_something_to_some_thing_same() {
        let invar_config = new_invar_config().with_write_mode(Ignore).into_owned();
        assert_eq!(invar_config.write_mode_option(), Some(Ignore));
        let updated = invar_config.with_write_mode_option(Some(Ignore));
        assert_borrowed(&updated);
        assert_eq!(updated.write_mode_option(), Some(Ignore));
    }

    #[test]
    fn with_write_mode_from_something_to_something_different() {
        let invar_config = new_invar_config().with_write_mode(Ignore).into_owned();
        assert_eq!(invar_config.write_mode_option(), Some(Ignore));
        let updated = invar_config.with_write_mode(Overwrite);
        assert_owned(&updated);
        assert_eq!(updated.write_mode_option(), Some(Overwrite));
    }

    #[test]
    fn with_write_mode_from_something_to_some_thing_different() {
        let invar_config = new_invar_config().with_write_mode(Ignore).into_owned();
        assert_eq!(invar_config.write_mode_option(), Some(Ignore));
        let updated = invar_config.with_write_mode_option(Some(Overwrite));
        assert_owned(&updated);
        assert_eq!(updated.write_mode_option(), Some(Overwrite));
    }

    #[test]
    fn with_write_mode_from_something_to_none() {
        let invar_config = new_invar_config().with_write_mode(Ignore).into_owned();
        assert_eq!(invar_config.write_mode_option(), Some(Ignore));
        let updated = invar_config.with_write_mode_option(None);
        assert_borrowed(&updated);
        assert_eq!(updated.write_mode_option(), Some(Ignore)); // Old value unchanged
    }

    // Interpolate

    #[test]
    fn with_interpolate_from_none_to_something() {
        let invar_config = new_invar_config();
        assert_eq!(invar_config.interpolate_option(), None);
        let updated = invar_config.with_interpolate(false);
        assert_owned(&updated);
        assert_eq!(updated.interpolate_option(), Some(false));
    }

    #[test]
    fn with_interpolate_from_none_to_some_thing() {
        let invar_config = new_invar_config();
        assert_eq!(invar_config.interpolate_option(), None);
        let updated = invar_config.with_interpolate_option(Some(false));
        assert_owned(&updated);
        assert_eq!(updated.interpolate_option(), Some(false));
    }

    #[test]
    fn with_interpolate_from_none_to_none() {
        let invar_config = new_invar_config();
        assert_eq!(invar_config.interpolate_option(), None);
        let updated = invar_config.with_interpolate_option(None);
        assert_borrowed(&updated);
        assert_eq!(updated.interpolate_option(), None);
    }

    #[test]
    fn with_interpolate_from_something_to_something_same() {
        let invar_config = new_invar_config().with_interpolate(false).into_owned();
        assert_eq!(invar_config.interpolate_option(), Some(false));
        let updated = invar_config.with_interpolate(false);
        assert_borrowed(&updated);
        assert_eq!(updated.interpolate_option(), Some(false));
    }

    #[test]
    fn with_interpolate_from_something_to_some_thing_same() {
        let invar_config = new_invar_config().with_interpolate(false).into_owned();
        assert_eq!(invar_config.interpolate_option(), Some(false));
        let updated = invar_config.with_interpolate_option(Some(false));
        assert_borrowed(&updated);
        assert_eq!(updated.interpolate_option(), Some(false));
    }

    #[test]
    fn with_interpolate_from_something_to_something_different() {
        let invar_config = new_invar_config().with_interpolate(false).into_owned();
        assert_eq!(invar_config.interpolate_option(), Some(false));
        let updated = invar_config.with_interpolate(true);
        assert_owned(&updated);
        assert_eq!(updated.interpolate_option(), Some(true));
    }

    #[test]
    fn with_interpolate_from_something_to_some_thing_different() {
        let invar_config = new_invar_config().with_interpolate(false).into_owned();
        assert_eq!(invar_config.interpolate_option(), Some(false));
        let updated = invar_config.with_interpolate_option(Some(true));
        assert_owned(&updated);
        assert_eq!(updated.interpolate_option(), Some(true));
    }

    #[test]
    fn with_interpolate_from_something_to_none() {
        let invar_config = new_invar_config().with_interpolate(false).into_owned();
        assert_eq!(invar_config.interpolate_option(), Some(false));
        let updated = invar_config.with_interpolate_option(None);
        assert_borrowed(&updated);
        assert_eq!(updated.interpolate_option(), Some(false)); // Old value unchanged
    }

    // Properties

    #[test]
    fn with_props_from_none_to_something() {
        let invar_config = empty_invar_config();
        let mut mapping = Table::new();
        insert_entry(&mut mapping, "foo", "bar");
        let updated = invar_config.with_props(mapping.clone());
        assert_owned(&updated);
        assert_eq!(updated.props_option(), &Some(mapping));
    }

    #[test]
    fn with_props_from_none_to_some_thing() {
        let invar_config = empty_invar_config();
        let mut mapping = Table::new();
        insert_entry(&mut mapping, "foo", "bar");
        let updated = invar_config.with_props_option(Some(mapping.clone()));
        assert_owned(&updated);
        assert_eq!(updated.props_option(), &Some(mapping));
    }

    #[test]
    fn with_props_from_none_to_none() {
        // Given
        let invar_config = empty_invar_config();

        // When
        let updated = invar_config.with_props_option(None);

        // Then
        assert_owned(&updated);
        assert_eq!(updated.props_option(), &Some(Table::new()));
    }

    #[test]
    fn with_props_from_something_add_same() {
        // Given
        let mut old_mapping = Table::new();
        insert_entry(&mut old_mapping, "foo", "bar");
        insert_entry(&mut old_mapping, "food", "baz");
        let old_mapping = old_mapping; // No longer mutable
        let invar_config = new_invar_config().with_props(old_mapping.clone()).into_owned();
        let mut new_mapping = Table::new();
        insert_entry(&mut new_mapping, "foo", "bar");

        // When
        let updated = invar_config.with_props(new_mapping.clone());

        // Then
        assert_borrowed(&updated);
        assert_eq!(updated.props_option(), &Some(old_mapping));
    }

    #[test]
    fn with_props_from_something_add_different() {
        // Given
        let mut old_mapping = Table::new();
        insert_entry(&mut old_mapping, "foo", "bar");
        insert_entry(&mut old_mapping, "food", "baz");
        let old_mapping = old_mapping; // No longer mutable
        let invar_config = new_invar_config().with_props(old_mapping.clone()).into_owned();
        let mut new_mapping = Table::new();
        insert_entry(&mut new_mapping, "foo", "beep");

        // When
        let updated = invar_config.with_props(new_mapping.clone());

        // Then
        let mut updated_mapping = old_mapping.clone();
        assert_owned(&updated);
        insert_entry(&mut updated_mapping, "foo", "beep");
        assert_eq!(updated.props_option(), &Some(updated_mapping));
    }

    #[test]
    fn with_props_from_something_add_new() {
        // Given
        let mut old_mapping = Table::new();
        insert_entry(&mut old_mapping, "foo", "bar");
        insert_entry(&mut old_mapping, "food", "baz");
        let old_mapping = old_mapping; // No longer mutable
        let invar_config = new_invar_config().with_props(old_mapping.clone()).into_owned();
        let mut new_mapping = Table::new();
        insert_entry(&mut new_mapping, "oh", "joy");

        // When
        let updated = invar_config.with_props(new_mapping.clone());

        // Then
        let mut updated_mapping = old_mapping.clone();
        assert_owned(&updated);
        insert_entry(&mut updated_mapping, "oh", "joy");
        assert_eq!(updated.props_option(), &Some(updated_mapping));
    }

    #[test]
    fn string_props() {
        // Given
        let mut mapping = Table::new();
        insert_entry(&mut mapping, "foo", "bar");
        insert_entry(&mut mapping, "food", "baz");
        let invar_config = new_invar_config().with_props(mapping).into_owned();

        // When
        let string_props = invar_config.string_props();

        // Then
        let mut expected = AHashMap::new();
        expected.insert("foo".to_string(), "bar".to_string());
        expected.insert("food".to_string(), "baz".to_string());
        assert_eq!(string_props, expected);
    }

    // Utility functions

    fn empty_invar_config() -> impl InvarConfig {
        InvarConfigData { write_mode: None, executable: None, interpolate: None, props: None }
    }

    fn new_invar_config() -> impl InvarConfig {
        InvarConfigData::new()
    }

    fn assert_owned(invar_config: &Cow<impl InvarConfig>) {
        if let Cow::Owned(_) = invar_config {
            return;
        } else {
            assert_eq!("borrowed", "owned")
        }
    }

    fn assert_borrowed(invar_config: &Cow<impl InvarConfig>) {
        if let Cow::Borrowed(_) = invar_config {
            return;
        } else {
            assert_eq!("owned", "borrowed")
        }
    }
}
