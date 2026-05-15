use std::time::{Duration, SystemTime, UNIX_EPOCH};

use boltffi::*;
use url::Url;
use uuid::Uuid;

/// Returns the duration unchanged.
#[demo_bench_macros::demo_case(
    "builtins.duration.should_roundtrip_value",
    description = "A Duration value crosses the wire and returns unchanged.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover built-in Duration values."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover built-in Duration values."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover built-in values."
    )
)]
#[export]
pub fn echo_duration(d: Duration) -> Duration {
    d
}

#[demo_bench_macros::demo_case(
    "builtins.duration.should_construct_from_parts",
    description = "Duration seconds and nanoseconds cross the wire and return as a Duration value.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover built-in Duration values."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover built-in Duration values."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover built-in values."
    )
)]
#[export]
pub fn make_duration(secs: u64, nanos: u32) -> Duration {
    Duration::new(secs, nanos)
}

#[demo_bench_macros::demo_case(
    "builtins.duration.should_report_milliseconds",
    description = "A Duration value crosses the wire and returns its millisecond count.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover built-in Duration values."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover built-in Duration values."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover built-in values."
    )
)]
#[export]
pub fn duration_as_millis(d: Duration) -> u64 {
    d.as_millis() as u64
}

#[demo_bench_macros::demo_case(
    "builtins.system_time.should_roundtrip_value",
    description = "A SystemTime value crosses the wire and returns unchanged.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover built-in SystemTime values."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover built-in SystemTime values."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover built-in values."
    )
)]
#[export]
pub fn echo_system_time(t: SystemTime) -> SystemTime {
    t
}

#[demo_bench_macros::demo_case(
    "builtins.system_time.should_convert_to_epoch_milliseconds",
    description = "A SystemTime value crosses the wire and returns Unix epoch milliseconds.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover built-in SystemTime values."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover built-in SystemTime values."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover built-in values."
    )
)]
#[export]
pub fn system_time_to_millis(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

#[demo_bench_macros::demo_case(
    "builtins.system_time.should_construct_from_epoch_milliseconds",
    description = "Unix epoch milliseconds cross the wire and return as a SystemTime value.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover built-in SystemTime values."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover built-in SystemTime values."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover built-in values."
    )
)]
#[export]
pub fn millis_to_system_time(millis: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(millis)
}

/// Returns the UUID unchanged.
#[demo_bench_macros::demo_case(
    "builtins.uuid.should_roundtrip_value",
    description = "A UUID value crosses the wire and returns unchanged.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover built-in UUID values."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover built-in UUID values."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover built-in values."
    )
)]
#[export]
pub fn echo_uuid(id: Uuid) -> Uuid {
    id
}

#[demo_bench_macros::demo_case(
    "builtins.uuid.should_format_canonical_string",
    description = "A UUID value crosses the wire and returns its canonical string representation.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover built-in UUID values."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover built-in UUID values."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover built-in values."
    )
)]
#[export]
pub fn uuid_to_string(id: Uuid) -> String {
    id.to_string()
}

#[demo_bench_macros::demo_case(
    "builtins.url.should_roundtrip_value",
    description = "A URL value crosses the wire and returns unchanged.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover built-in URL values."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover built-in URL values."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover built-in values."
    )
)]
#[export]
pub fn echo_url(url: Url) -> Url {
    url
}

#[demo_bench_macros::demo_case(
    "builtins.url.should_format_string",
    description = "A URL value crosses the wire and returns its string representation.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover built-in URL values."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover built-in URL values."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover built-in values."
    )
)]
#[export]
pub fn url_to_string(url: Url) -> String {
    url.to_string()
}
