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
        justification = "Ensure ServiceConfig::describe formats defaulted and explicit fields into a stable string.",
        directions = "Call `records::default_values::ServiceConfig::describe` through the generated binding and assert ServiceConfig::describe formats defaulted and explicit fields into a stable string.",
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when non-blittable records are implemented for Python."
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
        justification = "Ensure ServiceConfig::describe_with_prefix prepends a caller-provided string to the description.",
        directions = "Call `records::default_values::ServiceConfig::describe_with_prefix` through the generated binding and assert ServiceConfig::describe_with_prefix prepends a caller-provided string to the description.",
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when non-blittable records are implemented for Python."
        )
    )]
    pub fn describe_with_prefix(&self, prefix: String) -> String {
        format!("{}:{}", prefix, self.describe())
    }
}

#[demo_bench_macros::demo_case(
    "records.default_values.service_config.should_roundtrip_value",
    justification = "Ensure a ServiceConfig record with defaulted and explicit fields crosses the wire and returns unchanged.",
    directions = "Call `records::default_values::echo_service_config` through the generated binding and assert a ServiceConfig record with defaulted and explicit fields crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when non-blittable records are implemented for Python."
    )
)]
#[export]
pub fn echo_service_config(config: ServiceConfig) -> ServiceConfig {
    config
}
