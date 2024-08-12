
pub trait GitRemoteConfig {
    fn fetch_url(&self) -> &str;
    fn revision(&self) -> &str;
}
