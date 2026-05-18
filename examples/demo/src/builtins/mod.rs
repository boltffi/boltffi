use std::time::{Duration, SystemTime, UNIX_EPOCH};

use boltffi::*;
use url::Url;
use uuid::Uuid;

/// Returns the duration unchanged.
#[demo_bench_macros::demo_case(
    "builtins.duration.should_roundtrip_value",
    justification = "Ensure a Duration value crosses the wire and returns unchanged.",
    directions = "Call `builtins::echo_duration` through the generated binding and assert a Duration value crosses the wire and returns unchanged.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for built-in Duration values in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "#320: Java bindgen does not currently emit functions whose signatures use built-in value types (Duration, SystemTime, URL, UUID). Include this case when the relevant Java built-in binding is implemented."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently map BoltFFI built-in value types. Include this case when the relevant built-in Python binding is implemented."
    )
)]
#[export]
pub fn echo_duration(d: Duration) -> Duration {
    d
}

#[demo_bench_macros::demo_case(
    "builtins.duration.should_construct_from_parts",
    justification = "Ensure Duration seconds and nanoseconds cross the wire and return as a Duration value.",
    directions = "Call `builtins::make_duration` through the generated binding and assert Duration seconds and nanoseconds cross the wire and return as a Duration value.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for built-in Duration values in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "#320: Java bindgen does not currently emit functions whose signatures use built-in value types (Duration, SystemTime, URL, UUID). Include this case when the relevant Java built-in binding is implemented."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently map BoltFFI built-in value types. Include this case when the relevant built-in Python binding is implemented."
    )
)]
#[export]
pub fn make_duration(secs: u64, nanos: u32) -> Duration {
    Duration::new(secs, nanos)
}

#[demo_bench_macros::demo_case(
    "builtins.duration.should_report_milliseconds",
    justification = "Ensure a Duration value crosses the wire and returns its millisecond count.",
    directions = "Call `builtins::duration_as_millis` through the generated binding and assert a Duration value crosses the wire and returns its millisecond count.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for built-in Duration values in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "#320: Java bindgen does not currently emit functions whose signatures use built-in value types (Duration, SystemTime, URL, UUID). Include this case when the relevant Java built-in binding is implemented."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently map BoltFFI built-in value types. Include this case when the relevant built-in Python binding is implemented."
    )
)]
#[export]
pub fn duration_as_millis(d: Duration) -> u64 {
    d.as_millis() as u64
}

#[demo_bench_macros::demo_case(
    "builtins.system_time.should_roundtrip_value",
    justification = "Ensure a SystemTime value crosses the wire and returns unchanged.",
    directions = "Call `builtins::echo_system_time` through the generated binding and assert a SystemTime value crosses the wire and returns unchanged.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for built-in SystemTime values in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "#320: Java bindgen does not currently emit functions whose signatures use built-in value types (Duration, SystemTime, URL, UUID). Include this case when the relevant Java built-in binding is implemented."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently map BoltFFI built-in value types. Include this case when the relevant built-in Python binding is implemented."
    )
)]
#[export]
pub fn echo_system_time(t: SystemTime) -> SystemTime {
    t
}

#[demo_bench_macros::demo_case(
    "builtins.system_time.should_convert_to_epoch_milliseconds",
    justification = "Ensure a SystemTime value crosses the wire and returns Unix epoch milliseconds.",
    directions = "Call `builtins::system_time_to_millis` through the generated binding and assert a SystemTime value crosses the wire and returns Unix epoch milliseconds.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for built-in SystemTime values in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "#320: Java bindgen does not currently emit functions whose signatures use built-in value types (Duration, SystemTime, URL, UUID). Include this case when the relevant Java built-in binding is implemented."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently map BoltFFI built-in value types. Include this case when the relevant built-in Python binding is implemented."
    )
)]
#[export]
pub fn system_time_to_millis(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

#[demo_bench_macros::demo_case(
    "builtins.system_time.should_construct_from_epoch_milliseconds",
    justification = "Ensure Unix epoch milliseconds cross the wire and return as a SystemTime value.",
    directions = "Call `builtins::millis_to_system_time` through the generated binding and assert Unix epoch milliseconds cross the wire and return as a SystemTime value.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for built-in SystemTime values in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "#320: Java bindgen does not currently emit functions whose signatures use built-in value types (Duration, SystemTime, URL, UUID). Include this case when the relevant Java built-in binding is implemented."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently map BoltFFI built-in value types. Include this case when the relevant built-in Python binding is implemented."
    )
)]
#[export]
pub fn millis_to_system_time(millis: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(millis)
}

/// Returns the UUID unchanged.
#[demo_bench_macros::demo_case(
    "builtins.uuid.should_roundtrip_value",
    justification = "Ensure a UUID value crosses the wire and returns unchanged.",
    directions = "Call `builtins::echo_uuid` through the generated binding and assert a UUID value crosses the wire and returns unchanged.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for built-in UUID values in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "#320: Java bindgen does not currently emit functions whose signatures use built-in value types (Duration, SystemTime, URL, UUID). Include this case when the relevant Java built-in binding is implemented."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently map BoltFFI built-in value types. Include this case when the relevant built-in Python binding is implemented."
    )
)]
#[export]
pub fn echo_uuid(id: Uuid) -> Uuid {
    id
}

#[demo_bench_macros::demo_case(
    "builtins.uuid.should_format_canonical_string",
    justification = "Ensure a UUID value crosses the wire and returns its canonical string representation.",
    directions = "Call `builtins::uuid_to_string` through the generated binding and assert a UUID value crosses the wire and returns its canonical string representation.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for built-in UUID values in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "#320: Java bindgen does not currently emit functions whose signatures use built-in value types (Duration, SystemTime, URL, UUID). Include this case when the relevant Java built-in binding is implemented."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently map BoltFFI built-in value types. Include this case when the relevant built-in Python binding is implemented."
    )
)]
#[export]
pub fn uuid_to_string(id: Uuid) -> String {
    id.to_string()
}

#[demo_bench_macros::demo_case(
    "builtins.url.should_roundtrip_value",
    justification = "Ensure a URL value crosses the wire and returns unchanged.",
    directions = "Call `builtins::echo_url` through the generated binding and assert a URL value crosses the wire and returns unchanged.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for built-in URL values in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "#320: Java bindgen does not currently emit functions whose signatures use built-in value types (Duration, SystemTime, URL, UUID). Include this case when the relevant Java built-in binding is implemented."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently map BoltFFI built-in value types. Include this case when the relevant built-in Python binding is implemented."
    )
)]
#[export]
pub fn echo_url(url: Url) -> Url {
    url
}

#[demo_bench_macros::demo_case(
    "builtins.url.should_format_string",
    justification = "Ensure a URL value crosses the wire and returns its string representation.",
    directions = "Call `builtins::url_to_string` through the generated binding and assert a URL value crosses the wire and returns its string representation.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for built-in URL values in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "#320: Java bindgen does not currently emit functions whose signatures use built-in value types (Duration, SystemTime, URL, UUID). Include this case when the relevant Java built-in binding is implemented."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently map BoltFFI built-in value types. Include this case when the relevant built-in Python binding is implemented."
    )
)]
#[export]
pub fn url_to_string(url: Url) -> String {
    url.to_string()
}
