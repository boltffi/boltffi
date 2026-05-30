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
    justification = "Ensure checked_divide returns the integer quotient when the divisor is non-zero.",
    directions = "Call `results::error_enums::checked_divide` through the generated binding and assert checked_divide returns the integer quotient when the divisor is non-zero.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.checked_divide.should_reject_division_by_zero",
    justification = "Ensure checked_divide returns a typed MathError when the divisor is zero.",
    directions = "Call `results::error_enums::checked_divide` through the generated binding and assert checked_divide returns a typed MathError when the divisor is zero.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
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
    justification = "Ensure checked_sqrt returns the square root for non-negative floating-point input.",
    directions = "Call `results::error_enums::checked_sqrt` through the generated binding and assert checked_sqrt returns the square root for non-negative floating-point input.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.checked_sqrt.should_reject_negative_input",
    justification = "Ensure checked_sqrt returns a typed MathError for negative floating-point input.",
    directions = "Call `results::error_enums::checked_sqrt` through the generated binding and assert checked_sqrt returns a typed MathError for negative floating-point input.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
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
    justification = "Ensure checked_add returns the sum when the i32 addition does not overflow.",
    directions = "Call `results::error_enums::checked_add` through the generated binding and assert checked_add returns the sum when the i32 addition does not overflow.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.checked_add.should_reject_overflow",
    justification = "Ensure checked_add returns a typed MathError when i32 addition overflows.",
    directions = "Call `results::error_enums::checked_add` through the generated binding and assert checked_add returns a typed MathError when i32 addition overflows.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
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
    justification = "Ensure may_fail returns an Ok success string when the input is valid.",
    directions = "Call `results::error_enums::may_fail` through the generated binding and assert may_fail returns an Ok success string when the input is valid.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.may_fail.should_return_app_error_when_invalid",
    justification = "Ensure may_fail returns a structured AppError when the input is invalid.",
    directions = "Call `results::error_enums::may_fail` through the generated binding and assert may_fail returns a structured AppError when the input is invalid.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
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
    justification = "Ensure divide_app returns the integer quotient when the divisor is non-zero.",
    directions = "Call `results::error_enums::divide_app` through the generated binding and assert divide_app returns the integer quotient when the divisor is non-zero.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.divide_app.should_return_app_error_for_division_by_zero",
    justification = "Ensure divide_app returns a structured AppError when the divisor is zero.",
    directions = "Call `results::error_enums::divide_app` through the generated binding and assert divide_app returns a structured AppError when the divisor is zero.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
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
    justification = "Ensure validate_username returns the provided name when it satisfies all validation rules.",
    directions = "Call `results::error_enums::validate_username` through the generated binding and assert validate_username returns the provided name when it satisfies all validation rules.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.validate_username.should_reject_too_short_name",
    justification = "Ensure validate_username returns the TooShort typed error when the name has fewer than three characters.",
    directions = "Call `results::error_enums::validate_username` through the generated binding and assert validate_username returns the TooShort typed error when the name has fewer than three characters.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.validate_username.should_reject_too_long_name",
    justification = "Ensure validate_username returns the TooLong typed error when the name has more than twenty characters.",
    directions = "Call `results::error_enums::validate_username` through the generated binding and assert validate_username returns the TooLong typed error when the name has more than twenty characters.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.validate_username.should_reject_invalid_format",
    justification = "Ensure validate_username returns the InvalidFormat typed error when the name contains spaces.",
    directions = "Call `results::error_enums::validate_username` through the generated binding and assert validate_username returns the InvalidFormat typed error when the name contains spaces.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
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
    justification = "Ensure process_value returns the Success data enum variant for positive input.",
    directions = "Call `results::error_enums::process_value` through the generated binding and assert process_value returns the Success data enum variant for positive input.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.process_value.should_return_error_code_variant",
    justification = "Ensure process_value returns the ErrorCode data enum variant for zero input.",
    directions = "Call `results::error_enums::process_value` through the generated binding and assert process_value returns the ErrorCode data enum variant for zero input.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.process_value.should_return_error_with_data_variant",
    justification = "Ensure process_value returns the ErrorWithData data enum variant for negative input.",
    directions = "Call `results::error_enums::process_value` through the generated binding and assert process_value returns the ErrorWithData data enum variant for negative input.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
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
    justification = "Ensure api_result_is_success returns true for the Success data enum variant.",
    directions = "Call `results::error_enums::api_result_is_success` through the generated binding and assert api_result_is_success returns true for the Success data enum variant.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.api_result_is_success.should_report_error_variant",
    justification = "Ensure api_result_is_success returns false for non-success data enum variants.",
    directions = "Call `results::error_enums::api_result_is_success` through the generated binding and assert api_result_is_success returns false for non-success data enum variants.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[export]
pub fn api_result_is_success(result: ApiResult) -> bool {
    matches!(result, ApiResult::Success)
}

#[demo_bench_macros::demo_case(
    "results.error_enums.try_compute.should_return_doubled_value",
    justification = "Ensure try_compute returns an Ok value containing positive input doubled.",
    directions = "Call `results::error_enums::try_compute` through the generated binding and assert try_compute returns an Ok value containing positive input doubled.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.try_compute.should_return_overflow_error",
    justification = "Ensure try_compute returns the Overflow typed error for negative input.",
    directions = "Call `results::error_enums::try_compute` through the generated binding and assert try_compute returns the Overflow typed error for negative input.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
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
    justification = "Ensure create_success_response returns a BenchmarkResponse carrying an Ok DataPoint result.",
    directions = "Call `results::error_enums::create_success_response` through the generated binding and assert create_success_response returns a BenchmarkResponse carrying an Ok DataPoint result.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
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
    justification = "Ensure create_error_response returns or surfaces a BenchmarkResponse carrying an Err ComputeError result.",
    directions = "Call `results::error_enums::create_error_response` through the generated binding and assert create_error_response returns or surfaces a BenchmarkResponse carrying an Err ComputeError result.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
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
    justification = "Ensure is_response_success returns true for a BenchmarkResponse carrying an Ok result.",
    directions = "Call `results::error_enums::is_response_success` through the generated binding and assert is_response_success returns true for a BenchmarkResponse carrying an Ok result.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.benchmark_response.should_report_error_response",
    justification = "Ensure is_response_success returns false for a BenchmarkResponse carrying an Err result.",
    directions = "Call `results::error_enums::is_response_success` through the generated binding and assert is_response_success returns false for a BenchmarkResponse carrying an Err result.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[export]
pub fn is_response_success(response: BenchmarkResponse) -> bool {
    response.result.is_ok()
}

#[demo_bench_macros::demo_case(
    "results.error_enums.benchmark_response.should_return_value_for_success_response",
    justification = "Ensure get_response_value returns Some(DataPoint) for a BenchmarkResponse carrying an Ok result.",
    directions = "Call `results::error_enums::get_response_value` through the generated binding and assert get_response_value returns Some(DataPoint) for a BenchmarkResponse carrying an Ok result.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[demo_bench_macros::demo_case(
    "results.error_enums.benchmark_response.should_return_none_for_error_response",
    justification = "Ensure get_response_value returns None for a BenchmarkResponse carrying an Err result.",
    directions = "Call `results::error_enums::get_response_value` through the generated binding and assert get_response_value returns None for a BenchmarkResponse carrying an Err result.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer does not currently emit Result returns or structured error payloads. Include this case when typed Result support is implemented for Python."
    )
)]
#[export]
pub fn get_response_value(response: BenchmarkResponse) -> Option<DataPoint> {
    response.result.ok()
}
