use boltffi::*;

use crate::records::blittable::DataPoint;

/// Errors that can happen during math operations.
#[error]
#[derive(Clone, Debug, PartialEq)]
pub enum MathError {
    DivisionByZero,
    NegativeInput,
    Overflow,
}

impl std::fmt::Display for MathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DivisionByZero => write!(f, "division by zero"),
            Self::NegativeInput => write!(f, "negative input"),
            Self::Overflow => write!(f, "overflow"),
        }
    }
}

impl std::error::Error for MathError {}

impl From<UnexpectedFfiCallbackError> for MathError {
    fn from(_: UnexpectedFfiCallbackError) -> Self {
        Self::Overflow
    }
}

#[demo_bench_macros::demo_case(
    "results.error_enums.checked_divide.should_return_quotient",
    description = "checked_divide returns the integer quotient when the divisor is non-zero.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover typed result errors."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.checked_divide.should_reject_division_by_zero",
    description = "checked_divide returns a typed MathError when the divisor is zero.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover typed result errors."
    )
)]
#[export]
pub fn checked_divide(a: i32, b: i32) -> Result<i32, MathError> {
    if b == 0 {
        Err(MathError::DivisionByZero)
    } else {
        Ok(a / b)
    }
}

#[demo_bench_macros::demo_case(
    "results.error_enums.checked_sqrt.should_return_square_root",
    description = "checked_sqrt returns the square root for non-negative floating-point input.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover typed result errors."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.checked_sqrt.should_reject_negative_input",
    description = "checked_sqrt returns a typed MathError for negative floating-point input.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover typed result errors."
    )
)]
#[export]
pub fn checked_sqrt(x: f64) -> Result<f64, MathError> {
    if x < 0.0 {
        Err(MathError::NegativeInput)
    } else {
        Ok(x.sqrt())
    }
}

#[demo_bench_macros::demo_case(
    "results.error_enums.checked_add.should_return_sum",
    description = "checked_add returns the sum when the i32 addition does not overflow.",
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover the checked_add success path."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover typed result errors."
    ),
    exclude(
        wasm,
        reason = "The WASM demo tests do not currently cover the checked_add success path."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.checked_add.should_reject_overflow",
    description = "checked_add returns a typed MathError when i32 addition overflows.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover typed result errors."
    )
)]
#[export]
pub fn checked_add(a: i32, b: i32) -> Result<i32, MathError> {
    a.checked_add(b).ok_or(MathError::Overflow)
}

#[error]
#[derive(Clone, Debug, PartialEq)]
pub struct AppError {
    pub code: i32,
    pub message: String,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for AppError {}

#[demo_bench_macros::demo_case(
    "results.error_enums.may_fail.should_return_success_when_valid",
    description = "may_fail returns an Ok success string when the input is valid.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover structured result errors."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.may_fail.should_return_app_error_when_invalid",
    description = "may_fail returns a structured AppError when the input is invalid.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover structured result errors."
    )
)]
#[export]
pub fn may_fail(valid: bool) -> Result<String, AppError> {
    if valid {
        Ok("Success!".to_string())
    } else {
        Err(AppError {
            code: 400,
            message: "Invalid input".to_string(),
        })
    }
}

#[demo_bench_macros::demo_case(
    "results.error_enums.divide_app.should_return_quotient",
    description = "divide_app returns the integer quotient when the divisor is non-zero.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover structured result errors."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.divide_app.should_return_app_error_for_division_by_zero",
    description = "divide_app returns a structured AppError when the divisor is zero.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover structured result errors."
    )
)]
#[export]
pub fn divide_app(a: i32, b: i32) -> Result<i32, AppError> {
    if b == 0 {
        Err(AppError {
            code: 500,
            message: "Division by zero".to_string(),
        })
    } else {
        Ok(a / b)
    }
}

#[error]
#[derive(Clone, Debug, PartialEq)]
#[repr(i32)]
pub enum ValidationError {
    TooShort = 1,
    TooLong = 2,
    InvalidFormat = 3,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, "too short"),
            Self::TooLong => write!(f, "too long"),
            Self::InvalidFormat => write!(f, "invalid format"),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validates a username against length and format rules.
///
/// Returns the username on success, or a typed ValidationError
/// that tells the caller exactly what went wrong.
#[demo_bench_macros::demo_case(
    "results.error_enums.validate_username.should_accept_valid_name",
    description = "validate_username returns the provided name when it satisfies all validation rules.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover repr-int result errors."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.validate_username.should_reject_too_short_name",
    description = "validate_username returns the TooShort typed error when the name has fewer than three characters.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover repr-int result errors."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.validate_username.should_reject_too_long_name",
    description = "validate_username returns the TooLong typed error when the name has more than twenty characters.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover repr-int result errors."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.validate_username.should_reject_invalid_format",
    description = "validate_username returns the InvalidFormat typed error when the name contains spaces.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover repr-int result errors."
    )
)]
#[export]
pub fn validate_username(name: String) -> Result<String, ValidationError> {
    if name.len() < 3 {
        Err(ValidationError::TooShort)
    } else if name.len() > 20 {
        Err(ValidationError::TooLong)
    } else if name.contains(' ') {
        Err(ValidationError::InvalidFormat)
    } else {
        Ok(name)
    }
}

#[data]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(i32)]
pub enum ApiResult {
    Success = 0,
    ErrorCode(i32) = 1,
    ErrorWithData { code: i32, detail: i32 } = 2,
}

#[error]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(i32)]
pub enum ComputeError {
    InvalidInput(i32) = 0,
    Overflow { value: i32, limit: i32 } = 1,
}

impl std::fmt::Display for ComputeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(value) => write!(f, "invalid input: {}", value),
            Self::Overflow { value, limit } => {
                write!(f, "overflow: value {} exceeds limit {}", value, limit)
            }
        }
    }
}

impl std::error::Error for ComputeError {}

#[data]
#[derive(Clone, Debug, PartialEq)]
pub struct BenchmarkResponse {
    pub request_id: i64,
    pub result: Result<DataPoint, ComputeError>,
}

#[demo_bench_macros::demo_case(
    "results.error_enums.process_value.should_return_success_variant",
    description = "process_value returns the Success data enum variant for positive input.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover ApiResult data enum results."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.process_value.should_return_error_code_variant",
    description = "process_value returns the ErrorCode data enum variant for zero input.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover ApiResult data enum results."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.process_value.should_return_error_with_data_variant",
    description = "process_value returns the ErrorWithData data enum variant for negative input.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        wasm,
        reason = "The WASM demo tests do not currently cover the ErrorWithData process_value branch."
    )
)]
#[export]
pub fn process_value(value: i32) -> ApiResult {
    if value > 0 {
        ApiResult::Success
    } else if value == 0 {
        ApiResult::ErrorCode(-1)
    } else {
        ApiResult::ErrorWithData {
            code: value,
            detail: value * 2,
        }
    }
}

#[demo_bench_macros::demo_case(
    "results.error_enums.api_result_is_success.should_report_success_variant",
    description = "api_result_is_success returns true for the Success data enum variant.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover ApiResult data enum results."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.api_result_is_success.should_report_error_variant",
    description = "api_result_is_success returns false for non-success data enum variants.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover ApiResult data enum results."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover ApiResult data enum results."
    )
)]
#[export]
pub fn api_result_is_success(result: ApiResult) -> bool {
    matches!(result, ApiResult::Success)
}

#[demo_bench_macros::demo_case(
    "results.error_enums.try_compute.should_return_doubled_value",
    description = "try_compute returns an Ok value containing positive input doubled.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover ComputeError data enum results."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover ComputeError data enum results."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover ComputeError data enum results."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover ComputeError data enum results."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.try_compute.should_return_overflow_error",
    description = "try_compute returns the Overflow typed error for negative input.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover ComputeError data enum results."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover ComputeError data enum results."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover ComputeError data enum results."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover ComputeError data enum results."
    )
)]
#[export]
pub fn try_compute(value: i32) -> Result<i32, ComputeError> {
    if value > 0 {
        Ok(value * 2)
    } else if value == 0 {
        Err(ComputeError::InvalidInput(-999))
    } else {
        Err(ComputeError::Overflow { value, limit: 0 })
    }
}

#[demo_bench_macros::demo_case(
    "results.error_enums.benchmark_response.should_make_success_response",
    description = "create_success_response returns a BenchmarkResponse carrying an Ok DataPoint result.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover BenchmarkResponse result fields."
    )
)]
#[export]
pub fn create_success_response(request_id: i64, point: DataPoint) -> BenchmarkResponse {
    BenchmarkResponse {
        request_id,
        result: Ok(point),
    }
}

#[demo_bench_macros::demo_case(
    "results.error_enums.benchmark_response.should_make_error_response",
    description = "create_error_response returns or surfaces a BenchmarkResponse carrying an Err ComputeError result.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover BenchmarkResponse result fields."
    )
)]
#[export]
pub fn create_error_response(request_id: i64, error: ComputeError) -> BenchmarkResponse {
    BenchmarkResponse {
        request_id,
        result: Err(error),
    }
}

#[demo_bench_macros::demo_case(
    "results.error_enums.benchmark_response.should_report_success_response",
    description = "is_response_success returns true for a BenchmarkResponse carrying an Ok result.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover BenchmarkResponse result fields."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.benchmark_response.should_report_error_response",
    description = "is_response_success returns false for a BenchmarkResponse carrying an Err result.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover BenchmarkResponse result fields."
    )
)]
#[export]
pub fn is_response_success(response: BenchmarkResponse) -> bool {
    response.result.is_ok()
}

#[demo_bench_macros::demo_case(
    "results.error_enums.benchmark_response.should_return_value_for_success_response",
    description = "get_response_value returns Some(DataPoint) for a BenchmarkResponse carrying an Ok result.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover BenchmarkResponse result fields."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.benchmark_response.should_return_none_for_error_response",
    description = "get_response_value returns None for a BenchmarkResponse carrying an Err result.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover BenchmarkResponse result fields."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover BenchmarkResponse result fields."
    )
)]
#[export]
pub fn get_response_value(response: BenchmarkResponse) -> Option<DataPoint> {
    response.result.ok()
}
