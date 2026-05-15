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
    "enums.complex_variants.filter.should_roundtrip_variants",
    description = "Complex Filter variants with strings, nested vectors, and record vectors cross the FFI boundary unchanged.",
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
    "enums.complex_variants.filter.should_describe_variants",
    description = "describe_filter renders summaries for Filter variants containing primitive, string, vector, and record payloads.",
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
    "enums.complex_variants.api_response.should_roundtrip_variants",
    description = "ApiResponse data enum variants with success, redirect, and empty payload shapes cross the FFI boundary unchanged.",
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
pub fn echo_api_response(response: ApiResponse) -> ApiResponse {
    response
}

#[demo_bench_macros::demo_case(
    "enums.complex_variants.api_response.should_identify_success",
    description = "is_success returns true only for the ApiResponse Success variant.",
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
