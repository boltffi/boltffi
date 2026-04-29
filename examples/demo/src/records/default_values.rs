use boltffi::*;

#[data]
#[derive(Clone, Debug, PartialEq)]
pub struct ServiceConfig {
    pub name: String,
    #[boltffi::default(3)]
    pub retries: i32,
    #[boltffi::default("standard")]
    pub region: String,
    #[boltffi::default(None)]
    pub endpoint: Option<String>,
    #[boltffi::default("https://default")]
    pub backup_endpoint: Option<String>,
}

#[data(impl)]
impl ServiceConfig {
    pub fn describe(&self) -> String {
        let endpoint = self.endpoint.as_deref().unwrap_or("none");
        let backup_endpoint = self.backup_endpoint.as_deref().unwrap_or("none");
        format!(
            "{}:{}:{}:{}:{}",
            self.name, self.retries, self.region, endpoint, backup_endpoint
        )
    }

    pub fn describe_with_prefix(&self, prefix: String) -> String {
        format!("{}:{}", prefix, self.describe())
    }
}

#[export]
pub fn echo_service_config(config: ServiceConfig) -> ServiceConfig {
    config
}
