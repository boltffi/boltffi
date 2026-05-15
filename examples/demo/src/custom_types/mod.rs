use boltffi::*;
use chrono::{DateTime, Utc};

/// An email address that is validated on construction.
/// You can't slap #[data] on this because the invariant
/// (must contain '@') needs to be enforced on every crossing.
pub struct Email(String);

impl Email {
    pub fn new(value: &str) -> Result<Self, String> {
        if value.contains('@') {
            Ok(Self(value.to_string()))
        } else {
            Err(format!("invalid email: {}", value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[custom_ffi]
impl CustomFfiConvertible for Email {
    type FfiRepr = String;
    type Error = String;

    fn into_ffi(&self) -> String {
        self.0.clone()
    }

    fn try_from_ffi(repr: String) -> Result<Self, String> {
        Email::new(&repr)
    }
}

// chrono::DateTime<Utc> is a type from an external crate that we
// don't own, so we can't put #[data] on it. custom_type! generates
// conversion functions without a trait impl, avoids the orphan rule.
custom_type!(
    UtcDateTime,                    // public BoltFFI type name (used in generated API/type mapping keys)
    remote = DateTime<Utc>,             // the actual Rust type being wrapped
    repr = i64,                         // what gets sent over the FFI boundry i.e i64
    into_ffi = |dt: &DateTime<Utc>| dt.timestamp_millis(),  // Rust -> forien
    try_from_ffi = |millis: i64| {                           // forien -> Rust (can fail)
        DateTime::from_timestamp_millis(millis)
            .ok_or(CustomTypeConversionError)
    },
);

#[data]
pub struct Event {
    pub name: String,
    pub timestamp: DateTime<Utc>,
}

#[demo_bench_macros::demo_case(
    "custom_types.email.should_roundtrip_value",
    description = "An email custom type crosses the wire through its string representation and returns unchanged.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover custom types."
    )
)]
#[export]
pub fn echo_email(email: Email) -> Email {
    email
}

#[demo_bench_macros::demo_case(
    "custom_types.email.should_extract_domain",
    description = "An email custom type crosses the wire and returns its domain string.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover custom types."
    )
)]
#[export]
pub fn email_domain(email: Email) -> String {
    email.as_str().split('@').nth(1).unwrap_or("").to_string()
}

#[demo_bench_macros::demo_case(
    "custom_types.datetime.should_roundtrip_millis",
    description = "A DateTime custom type crosses the wire through millisecond representation and returns unchanged.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover custom types."
    )
)]
#[export]
pub fn echo_datetime(dt: DateTime<Utc>) -> DateTime<Utc> {
    dt
}

#[demo_bench_macros::demo_case(
    "custom_types.datetime.should_convert_to_millis",
    description = "A DateTime custom type crosses the wire and returns its millisecond representation.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover custom types."
    )
)]
#[export]
pub fn datetime_to_millis(dt: DateTime<Utc>) -> i64 {
    dt.timestamp_millis()
}

#[demo_bench_macros::demo_case(
    "custom_types.datetime.should_format_rfc3339_timestamp",
    description = "A DateTime custom type crosses the wire and returns an RFC3339 timestamp string.",
    exclude(
        kotlin,
        reason = "The Kotlin custom type demo does not currently cover the standalone timestamp formatting helper."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover custom types."
    )
)]
#[export]
pub fn format_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339()
}

#[export]
pub fn echo_event(event: Event) -> Event {
    event
}

#[export]
pub fn event_timestamp(event: Event) -> i64 {
    event.timestamp.timestamp_millis()
}

#[export]
pub fn echo_emails(emails: Vec<Email>) -> Vec<Email> {
    emails
}

#[export]
pub fn echo_datetimes(dts: Vec<DateTime<Utc>>) -> Vec<DateTime<Utc>> {
    dts
}
