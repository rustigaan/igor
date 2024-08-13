
pub trait GitRemoteConfig {
    fn fetch_url(&self) -> &str;
    fn revision(&self) -> &str;
}

#[cfg(test)]
mod test {
    use super::*;
    use super::super::git_remote_config_data::GitRemoteConfigData;

    #[test]
    fn getters() {
        // Given
        let fetch_url = "https://github.com/rustigaan/igor.git";
        let revision = "490656c";
        let git_remote_config_data = GitRemoteConfigData::new(
            fetch_url,
            revision
        );

        // When
        let git_remote_config: Box<dyn GitRemoteConfig> = Box::new(git_remote_config_data);

        // Then
        assert_eq!(git_remote_config.fetch_url(), fetch_url);
        assert_eq!(git_remote_config.revision(), revision);
    }
}