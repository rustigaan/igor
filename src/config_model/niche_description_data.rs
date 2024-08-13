use serde::Deserialize;
use crate::config_model::niche_description::*;

#[derive(Deserialize,Debug)]
pub struct NicheDescriptionData {
    name: String,
    description: Option<String>,
}

impl NicheDescriptionData {
    pub fn new(name: impl Into<String>, description: Option<impl Into<String>>) -> Self {
        let name = name.into();
        let description = description.map(Into::into);
        NicheDescriptionData {
            name,
            description,
        }
    }
}

impl NicheDescription for NicheDescriptionData {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&String> {
        self.description.as_ref()
    }
}
