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

#[export]
pub fn echo_filter(f: Filter) -> Filter {
    f
}

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

#[export]
pub fn echo_api_response(response: ApiResponse) -> ApiResponse {
    response
}

#[export]
pub fn is_success(response: ApiResponse) -> bool {
    matches!(response, ApiResponse::Success { .. })
}
