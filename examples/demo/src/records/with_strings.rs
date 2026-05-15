use boltffi::*;
use demo_bench_macros::benchmark_candidate;

/// Represents a person with a name and age.
#[data]
#[benchmark_candidate(record, uniffi)]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Person {
    pub name: String,
    pub age: u32,
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.with_strings.person.should_roundtrip_value",
    description = "A Person record with string and integer fields crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with string fields."
    )
)]
pub fn echo_person(p: Person) -> Person {
    p
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.with_strings.person.should_make_from_fields",
    description = "make_person returns a Person containing the provided name and age fields.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with string fields."
    )
)]
pub fn make_person(name: String, age: u32) -> Person {
    Person { name, age }
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.with_strings.person.should_format_greeting",
    description = "greet_person formats a greeting from a Person record received over FFI.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with string fields."
    )
)]
pub fn greet_person(p: Person) -> String {
    format!("Hello, {}! You are {} years old.", p.name, p.age)
}

#[data]
#[benchmark_candidate(record, uniffi)]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Address {
    pub street: String,
    pub city: String,
    pub zip: String,
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.with_strings.address.should_roundtrip_value",
    description = "An Address record with multiple string fields crosses the wire and returns unchanged.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover Address records."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with string fields."
    )
)]
pub fn echo_address(a: Address) -> Address {
    a
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.with_strings.address.should_format_value",
    description = "format_address receives an Address record and returns a formatted string.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover Address records."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with string fields."
    )
)]
pub fn format_address(a: Address) -> String {
    format!("{}, {}, {}", a.street, a.city, a.zip)
}
