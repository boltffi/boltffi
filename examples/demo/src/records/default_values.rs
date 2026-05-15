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
    #[demo_bench_macros::demo_case(
        "records.default_values.service_config.should_describe_values",
        description = "ServiceConfig::describe formats defaulted and explicit fields into a stable string.",
        exclude(
            python,
            reason = "The Python demo tests do not currently cover ServiceConfig records."
        )
    )]
    pub fn describe(&self) -> String {
        let endpoint = self.endpoint.as_deref().unwrap_or("none");
        let backup_endpoint = self.backup_endpoint.as_deref().unwrap_or("none");
        format!(
            "{}:{}:{}:{}:{}",
            self.name, self.retries, self.region, endpoint, backup_endpoint
        )
    }

    #[demo_bench_macros::demo_case(
        "records.default_values.service_config.should_describe_with_prefix",
        description = "ServiceConfig::describe_with_prefix prepends a caller-provided string to the description.",
        exclude(
            python,
            reason = "The Python demo tests do not currently cover ServiceConfig records."
        )
    )]
    pub fn describe_with_prefix(&self, prefix: String) -> String {
        format!("{}:{}", prefix, self.describe())
    }
}

#[demo_bench_macros::demo_case(
    "records.default_values.service_config.should_roundtrip_value",
    description = "A ServiceConfig record with defaulted and explicit fields crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover ServiceConfig records."
    )
)]
#[export]
pub fn echo_service_config(config: ServiceConfig) -> ServiceConfig {
    config
}
