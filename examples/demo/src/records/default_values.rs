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
        "records.default_values.service_config.from_owned_name.should_return_config",
        justification = "Ensure a non-blittable record constructor accepts an owned String and returns the encoded record.",
        directions = "Call `records::default_values::ServiceConfig::from_owned_name` through the generated binding and assert it returns a ServiceConfig using the provided name."
    )]
    pub fn from_owned_name(name: String) -> Self {
        Self {
            name,
            retries: 3,
            region: "standard".to_string(),
            endpoint: None,
            backup_endpoint: Some("https://default".to_string()),
        }
    }

    #[demo_bench_macros::demo_case(
        "records.default_values.service_config.from_borrowed_name.should_return_config",
        justification = "Ensure a non-blittable record constructor accepts a borrowed string and returns the encoded record.",
        directions = "Call `records::default_values::ServiceConfig::from_borrowed_name` through the generated binding and assert it returns a ServiceConfig using the provided name."
    )]
    pub fn from_borrowed_name(name: &str) -> Self {
        Self {
            name: name.to_string(),
            retries: 3,
            region: "standard".to_string(),
            endpoint: None,
            backup_endpoint: Some("https://default".to_string()),
        }
    }

    #[demo_bench_macros::demo_case(
        "records.default_values.service_config.from_string_ref_name.should_return_config",
        justification = "Ensure a non-blittable record constructor accepts a borrowed String reference and returns the encoded record.",
        directions = "Call `records::default_values::ServiceConfig::from_string_ref_name` through the generated binding and assert it returns a ServiceConfig using the provided name."
    )]
    #[allow(clippy::ptr_arg)]
    pub fn from_string_ref_name(name: &String) -> Self {
        Self {
            name: name.clone(),
            retries: 3,
            region: "standard".to_string(),
            endpoint: None,
            backup_endpoint: Some("https://default".to_string()),
        }
    }

    #[demo_bench_macros::demo_case(
        "records.default_values.service_config.from_non_empty_name.should_return_config_for_non_empty_values",
        justification = "Ensure ServiceConfig::from_non_empty_name returns a config for non-empty borrowed name and region values.",
        directions = "Call `records::default_values::ServiceConfig::from_non_empty_name` through the generated binding with non-empty borrowed name and region values and assert it returns the expected ServiceConfig."
    )]
    #[demo_bench_macros::demo_case(
        "records.default_values.service_config.from_non_empty_name.should_return_none_for_empty_values",
        justification = "Ensure ServiceConfig::from_non_empty_name returns None when either borrowed input is empty.",
        directions = "Call `records::default_values::ServiceConfig::from_non_empty_name` through the generated binding with an empty borrowed name or region and assert it returns None."
    )]
    pub fn from_non_empty_name(name: &str, region: &str) -> Option<Self> {
        (!name.is_empty() && !region.is_empty()).then(|| Self {
            name: name.to_string(),
            retries: 3,
            region: region.to_string(),
            endpoint: None,
            backup_endpoint: Some("https://default".to_string()),
        })
    }

    #[demo_bench_macros::demo_case(
        "records.default_values.service_config.should_describe_values",
        justification = "Ensure ServiceConfig::describe formats defaulted and explicit fields into a stable string.",
        directions = "Call `records::default_values::ServiceConfig::describe` through the generated binding and assert ServiceConfig::describe formats defaulted and explicit fields into a stable string."
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
        directions = "Call `records::default_values::ServiceConfig::describe_with_prefix` through the generated binding and assert ServiceConfig::describe_with_prefix prepends a caller-provided string to the description."
    )]
    pub fn describe_with_prefix(&self, prefix: String) -> String {
        format!("{}:{}", prefix, self.describe())
    }

    #[demo_bench_macros::demo_case(
        "records.default_values.service_config.try_with_retries.should_return_config",
        justification = "Ensure a fallible ServiceConfig constructor returns a non-blittable record through Result Ok.",
        directions = "Call `records::default_values::ServiceConfig::try_with_retries` through the generated binding and assert a non-blittable ServiceConfig record returns through Result Ok.",
        exclude(
            swift,
            reason = ExclusionReason::CoverageGap,
            details = "This C# regression case is not asserted by the Swift demo suite yet. Add it when Swift demo coverage expands for fallible non-blittable record constructors."
        ),
        exclude(
            java,
            reason = ExclusionReason::CoverageGap,
            details = "This C# regression case is not asserted by the Java demo suite yet. Add it when Java demo coverage expands for fallible non-blittable record constructors."
        )
    )]
    #[demo_bench_macros::demo_case(
        "records.default_values.service_config.try_with_retries.should_reject_negative_retries",
        justification = "Ensure a fallible ServiceConfig constructor maps Result Err to the target language error path.",
        directions = "Call `records::default_values::ServiceConfig::try_with_retries` through the generated binding with negative retries and assert Result Err becomes a language-native error.",
        exclude(
            swift,
            reason = ExclusionReason::CoverageGap,
            details = "This C# regression case is not asserted by the Swift demo suite yet. Add it when Swift demo coverage expands for fallible non-blittable record constructors."
        ),
        exclude(
            java,
            reason = ExclusionReason::CoverageGap,
            details = "This C# regression case is not asserted by the Java demo suite yet. Add it when Java demo coverage expands for fallible non-blittable record constructors."
        )
    )]
    pub fn try_with_retries(retries: i32) -> Result<Self, String> {
        if retries < 0 {
            Err("service config retries must be non-negative".to_string())
        } else {
            Ok(Self {
                name: "generated".to_string(),
                retries,
                region: "standard".to_string(),
                endpoint: None,
                backup_endpoint: Some("https://default".to_string()),
            })
        }
    }

    #[demo_bench_macros::demo_case(
        "records.default_values.service_config.maybe_with_retries.should_return_some",
        justification = "Ensure an optional ServiceConfig constructor returns Some(non-blittable record) through the generated binding.",
        directions = "Call `records::default_values::ServiceConfig::maybe_with_retries` through the generated binding with non-negative retries and assert it returns Some(ServiceConfig).",
        exclude(
            swift,
            reason = ExclusionReason::CoverageGap,
            details = "This C# regression case is not asserted by the Swift demo suite yet. Add it when Swift demo coverage expands for optional non-blittable record constructors."
        ),
        exclude(
            java,
            reason = ExclusionReason::CoverageGap,
            details = "This C# regression case is not asserted by the Java demo suite yet. Add it when Java demo coverage expands for optional non-blittable record constructors."
        )
    )]
    #[demo_bench_macros::demo_case(
        "records.default_values.service_config.maybe_with_retries.should_return_none",
        justification = "Ensure an optional ServiceConfig constructor returns None through the generated binding.",
        directions = "Call `records::default_values::ServiceConfig::maybe_with_retries` through the generated binding with negative retries and assert it returns None/null.",
        exclude(
            swift,
            reason = ExclusionReason::CoverageGap,
            details = "This C# regression case is not asserted by the Swift demo suite yet. Add it when Swift demo coverage expands for optional non-blittable record constructors."
        ),
        exclude(
            java,
            reason = ExclusionReason::CoverageGap,
            details = "This C# regression case is not asserted by the Java demo suite yet. Add it when Java demo coverage expands for optional non-blittable record constructors."
        )
    )]
    pub fn maybe_with_retries(retries: i32) -> Option<Self> {
        if retries < 0 {
            None
        } else {
            Some(Self {
                name: "generated".to_string(),
                retries,
                region: "standard".to_string(),
                endpoint: None,
                backup_endpoint: Some("https://default".to_string()),
            })
        }
    }
}

#[demo_bench_macros::demo_case(
    "records.default_values.service_config.should_roundtrip_value",
    justification = "Ensure a ServiceConfig record with defaulted and explicit fields crosses the wire and returns unchanged.",
    directions = "Call `records::default_values::echo_service_config` through the generated binding and assert a ServiceConfig record with defaulted and explicit fields crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_service_config(config: ServiceConfig) -> ServiceConfig {
    config
}
