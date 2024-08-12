pub use crate::config_model::niche_description_data::NicheDescriptionData;

pub trait NicheDescription {
    fn name(&self) -> &str;
    fn description(&self) -> &Option<String>;
}
