pub use crate::config_model::niche_description_data::NicheDescriptionData;

pub trait NicheDescription {
    fn name(&self) -> &str;
    fn description(&self) -> Option<&String>;
}

#[cfg(test)]
mod test {
    use super::*;
    use super::super::niche_description_data::NicheDescriptionData;

    #[test]
    fn getters() {
        // Given
        let name = "workshop";
        let description = Some("Place to work");
        let niche_description_data = NicheDescriptionData::new(
            name,
            description
        );

        // When
        let niche_description: Box<dyn NicheDescription> = Box::new(niche_description_data);

        // Then
        assert_eq!(niche_description.name(), name);
        assert_eq!(niche_description.description().map(std::string::String::as_ref), description);
    }
}