use std::borrow::Cow;
use ahash::AHashMap;
use log::debug;
use once_cell::sync::{Lazy};
use regex::Regex;

static PLACEHOLDER_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new("[{][{]([A-Z][A-Z_]*)[}][}]").unwrap()
});

pub fn interpolate(source: &str, variables: AHashMap<String, String>) -> Cow<'_, str> {
    let mut result: Cow<str> = Cow::from(source);
    if variables.is_empty() {
        return result;
    }
    if let Some(captures) = PLACEHOLDER_REGEX.captures(result.as_ref()) {
        debug!("Interpolate: capture: {:?}", captures.get(0));
        if let (Some(match_placeholder), Some(match_name)) = (captures.get(0), captures.get(1)) {
            debug!("Interpolate: placeholder name: '{}'", match_name.as_str());
            if let Some(value) = variables.get(match_name.as_str()) {
                debug!("Interpolate: '{}' to '{}' in: {}", match_placeholder.as_str(), value, result);
                let value= value.clone();
                let range = match_placeholder.range();
                result.to_mut().replace_range(range, &value);
            }
        }
    }
    result
}