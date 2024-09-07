use serde::{Deserialize, Serialize};
use crate::config_model::niche_description::*;

#[derive(Deserialize,Serialize,Debug)]
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

    fn description(&self) -> Option<&str> {
        self.description.as_ref().map(String::as_ref)
    }
}
