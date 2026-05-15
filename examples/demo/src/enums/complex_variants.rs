use boltffi::*;

use crate::records::blittable::Point;

#[data]
#[derive(Clone, Debug, PartialEq)]
pub enum Filter {
    None,
    ByName { name: String },
    ByRange { min: f64, max: f64 },
    ByTags { tags: Vec<String> },
    ByGroups { groups: Vec<Vec<String>> },
    ByPoints { anchors: Vec<Point> },
}

#[demo_bench_macros::demo_case(
    "enums.complex_variants.filter.none.should_roundtrip_unit_variant",
    justification = "Ensure the unit Filter::None variant crosses the FFI boundary unchanged.",
    directions = "Call `enums::complex_variants::echo_filter` through the generated binding and assert the unit Filter::None variant crosses the FFI boundary unchanged.",
    exclude(
        csharp,
        reason = "The C# complex collection tests do not currently round-trip the Filter::None variant."
    ),
    exclude(
        java,
        reason = "The Java complex enum tests do not currently round-trip the Filter::None variant."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover complex data-enum variants."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.complex_variants.filter.by_name.should_roundtrip_string_payload",
    justification = "Ensure the Filter::ByName variant preserves its string payload when round-tripped.",
    directions = "Call `enums::complex_variants::echo_filter` through the generated binding and assert the Filter::ByName variant preserves its string payload when round-tripped.",
    exclude(
        csharp,
        reason = "The C# complex collection tests do not currently round-trip the Filter::ByName variant."
    ),
    exclude(
        java,
        reason = "The Java complex enum tests do not currently round-trip the Filter::ByName variant."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover complex data-enum variants."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.complex_variants.filter.by_tags.should_roundtrip_string_vector_payload",
    justification = "Ensure the Filter::ByTags variant preserves a vector of UTF-8 strings when round-tripped.",
    directions = "Call `enums::complex_variants::echo_filter` through the generated binding and assert the Filter::ByTags variant preserves a vector of UTF-8 strings when round-tripped.",
    exclude(
        apple,
        reason = "The Apple complex enum tests do not currently round-trip the Filter::ByTags variant."
    ),
    exclude(
        java,
        reason = "The Java complex enum tests do not currently round-trip the Filter::ByTags variant."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin complex enum tests do not currently round-trip the Filter::ByTags variant."
    ),
    exclude(
        wasm,
        reason = "The Wasm complex enum tests do not currently round-trip the Filter::ByTags variant."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover complex data-enum variants."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.complex_variants.filter.by_groups.should_roundtrip_nested_string_vectors",
    justification = "Ensure the Filter::ByGroups variant preserves nested UTF-8 string vectors when round-tripped.",
    directions = "Call `enums::complex_variants::echo_filter` through the generated binding and assert the Filter::ByGroups variant preserves nested UTF-8 string vectors when round-tripped.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover complex data-enum variants."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.complex_variants.filter.by_points.should_roundtrip_record_vector_payload",
    justification = "Ensure the Filter::ByPoints variant preserves a vector of Point records when round-tripped.",
    directions = "Call `enums::complex_variants::echo_filter` through the generated binding and assert the Filter::ByPoints variant preserves a vector of Point records when round-tripped.",
    exclude(
        apple,
        reason = "The Apple complex enum tests do not currently round-trip the Filter::ByPoints variant."
    ),
    exclude(
        java,
        reason = "The Java complex enum tests do not currently round-trip the Filter::ByPoints variant."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin complex enum tests do not currently round-trip the Filter::ByPoints variant."
    ),
    exclude(
        wasm,
        reason = "The Wasm complex enum tests do not currently round-trip the Filter::ByPoints variant."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover complex data-enum variants."
    )
)]
#[export]
pub fn echo_filter(f: Filter) -> Filter {
    f
}

#[demo_bench_macros::demo_case(
    "enums.complex_variants.filter.by_name.should_describe_string_payload",
    justification = "Ensure describe_filter renders a ByName string payload in the summary.",
    directions = "Call `enums::complex_variants::describe_filter` through the generated binding and assert describe_filter renders a ByName string payload in the summary.",
    exclude(
        csharp,
        reason = "The C# complex collection tests do not currently describe the Filter::ByName variant."
    ),
    exclude(
        java,
        reason = "The Java complex enum tests do not currently describe the Filter::ByName variant."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover complex data-enum variants."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.complex_variants.filter.by_range.should_describe_numeric_bounds",
    justification = "Ensure describe_filter renders a ByRange numeric lower and upper bound.",
    directions = "Call `enums::complex_variants::describe_filter` through the generated binding and assert describe_filter renders a ByRange numeric lower and upper bound.",
    exclude(
        csharp,
        reason = "The C# complex collection tests do not currently describe the Filter::ByRange variant."
    ),
    exclude(
        java,
        reason = "The Java complex enum tests do not currently describe the Filter::ByRange variant."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover complex data-enum variants."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.complex_variants.filter.by_tags.should_describe_string_vector_payload",
    justification = "Ensure describe_filter counts the UTF-8 strings in a ByTags vector payload.",
    directions = "Call `enums::complex_variants::describe_filter` through the generated binding and assert describe_filter counts the UTF-8 strings in a ByTags vector payload.",
    exclude(
        java,
        reason = "The Java complex enum tests do not currently describe the Filter::ByTags variant."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover complex data-enum variants."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.complex_variants.filter.by_groups.should_describe_nested_string_vectors",
    justification = "Ensure describe_filter counts the outer vector in a ByGroups nested vector payload.",
    directions = "Call `enums::complex_variants::describe_filter` through the generated binding and assert describe_filter counts the outer vector in a ByGroups nested vector payload.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover complex data-enum variants."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.complex_variants.filter.by_points.should_describe_record_vector_payload",
    justification = "Ensure describe_filter counts Point records in a ByPoints vector payload.",
    directions = "Call `enums::complex_variants::describe_filter` through the generated binding and assert describe_filter counts Point records in a ByPoints vector payload.",
    exclude(
        java,
        reason = "The Java complex enum tests do not currently describe the Filter::ByPoints variant."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover complex data-enum variants."
    )
)]
#[export]
pub fn describe_filter(f: Filter) -> String {
    match f {
        Filter::None => "no filter".to_string(),
        Filter::ByName { name } => format!("filter by name: {}", name),
        Filter::ByRange { min, max } => format!("filter by range: {}..{}", min, max),
        Filter::ByTags { tags } => format!("filter by {} tags", tags.len()),
        Filter::ByGroups { groups } => format!("filter by {} groups", groups.len()),
        Filter::ByPoints { anchors } => format!("filter by {} anchor points", anchors.len()),
    }
}

#[data]
#[derive(Clone, Debug, PartialEq)]
pub enum ApiResponse {
    Success { data: String },
    Error { code: i32, message: String },
    Redirect { url: String },
    Empty,
}

#[demo_bench_macros::demo_case(
    "enums.complex_variants.api_response.success.should_roundtrip_string_payload",
    justification = "Ensure the ApiResponse::Success variant preserves its string payload when round-tripped.",
    directions = "Call `enums::complex_variants::echo_api_response` through the generated binding and assert the ApiResponse::Success variant preserves its string payload when round-tripped.",
    exclude(
        csharp,
        reason = "The C# demo currently covers Filter complex variants but not ApiResponse helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover complex data-enum variants."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.complex_variants.api_response.redirect.should_roundtrip_url_payload",
    justification = "Ensure the ApiResponse::Redirect variant preserves its URL payload when round-tripped.",
    directions = "Call `enums::complex_variants::echo_api_response` through the generated binding and assert the ApiResponse::Redirect variant preserves its URL payload when round-tripped.",
    exclude(
        csharp,
        reason = "The C# demo currently covers Filter complex variants but not ApiResponse helpers."
    ),
    exclude(
        java,
        reason = "The Java complex enum tests do not currently round-trip the ApiResponse::Redirect variant."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover complex data-enum variants."
    )
)]
#[export]
pub fn echo_api_response(response: ApiResponse) -> ApiResponse {
    response
}

#[demo_bench_macros::demo_case(
    "enums.complex_variants.api_response.success.should_identify_success",
    justification = "Ensure is_success returns true for the ApiResponse::Success variant.",
    directions = "Call `enums::complex_variants::is_success` through the generated binding and assert is_success returns true for the ApiResponse::Success variant.",
    exclude(
        csharp,
        reason = "The C# demo currently covers Filter complex variants but not ApiResponse helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover complex data-enum variants."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.complex_variants.api_response.empty.should_not_identify_as_success",
    justification = "Ensure is_success returns false for the ApiResponse::Empty variant.",
    directions = "Call `enums::complex_variants::is_success` through the generated binding and assert is_success returns false for the ApiResponse::Empty variant.",
    exclude(
        csharp,
        reason = "The C# demo currently covers Filter complex variants but not ApiResponse helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover complex data-enum variants."
    )
)]
#[export]
pub fn is_success(response: ApiResponse) -> bool {
    matches!(response, ApiResponse::Success { .. })
}
