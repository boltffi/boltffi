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
    justification = "Ensure a Person record with string and integer fields crosses the wire and returns unchanged.",
    directions = "Call `records::with_strings::echo_person` through the generated binding and assert a Person record with string and integer fields crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with string fields are implemented for Python."
    )
)]
pub fn echo_person(p: Person) -> Person {
    p
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.with_strings.person.should_make_from_fields",
    justification = "Ensure make_person returns a Person containing the provided name and age fields.",
    directions = "Call `records::with_strings::make_person` through the generated binding and assert make_person returns a Person containing the provided name and age fields.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with string fields are implemented for Python."
    )
)]
pub fn make_person(name: String, age: u32) -> Person {
    Person { name, age }
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.with_strings.person.should_format_greeting",
    justification = "Ensure greet_person formats a greeting from a Person record received over FFI.",
    directions = "Call `records::with_strings::greet_person` through the generated binding and assert greet_person formats a greeting from a Person record received over FFI.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with string fields are implemented for Python."
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
    justification = "Ensure an Address record with multiple string fields crosses the wire and returns unchanged.",
    directions = "Call `records::with_strings::echo_address` through the generated binding and assert an Address record with multiple string fields crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with string fields are implemented for Python."
    )
)]
pub fn echo_address(a: Address) -> Address {
    a
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.with_strings.address.should_format_value",
    justification = "Ensure format_address receives an Address record and returns a formatted string.",
    directions = "Call `records::with_strings::format_address` through the generated binding and assert format_address receives an Address record and returns a formatted string.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with string fields are implemented for Python."
    )
)]
pub fn format_address(a: Address) -> String {
    format!("{}, {}, {}", a.street, a.city, a.zip)
}
