use boltffi::*;

use crate::records::blittable::Point;

#[demo_bench_macros::demo_case(
    "results.basic.safe_divide.should_return_quotient",
    description = "safe_divide returns the integer quotient when the divisor is non-zero.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover string-error Result exports."
    )
)]
#[demo_bench_macros::demo_case(
    "results.basic.safe_divide.should_reject_division_by_zero",
    description = "safe_divide returns a language-native error when the divisor is zero.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover string-error Result exports."
    )
)]
#[export]
pub fn safe_divide(a: i32, b: i32) -> Result<i32, String> {
    if b == 0 {
        Err("division by zero".to_string())
    } else {
        Ok(a / b)
    }
}

#[demo_bench_macros::demo_case(
    "results.basic.safe_sqrt.should_return_square_root",
    description = "safe_sqrt returns the square root for non-negative floating-point input.",
    exclude(csharp, reason = "The C# demo tests do not currently cover safe_sqrt."),
    exclude(java, reason = "The Java demo tests do not currently cover safe_sqrt."),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover string-error Result exports."
    )
)]
#[demo_bench_macros::demo_case(
    "results.basic.safe_sqrt.should_reject_negative_input",
    description = "safe_sqrt returns a language-native error for negative floating-point input.",
    exclude(csharp, reason = "The C# demo tests do not currently cover safe_sqrt."),
    exclude(java, reason = "The Java demo tests do not currently cover safe_sqrt."),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover string-error Result exports."
    )
)]
#[export]
pub fn safe_sqrt(x: f64) -> Result<f64, String> {
    if x < 0.0 {
        Err("negative input".to_string())
    } else {
        Ok(x.sqrt())
    }
}

#[demo_bench_macros::demo_case(
    "results.basic.parse_point.should_parse_coordinates",
    description = "parse_point parses a comma-separated coordinate string into a Point record.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover string-error Result exports."
    )
)]
#[demo_bench_macros::demo_case(
    "results.basic.parse_point.should_reject_malformed_input",
    description = "parse_point returns a language-native error when the input is not a coordinate pair.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover string-error Result exports."
    )
)]
#[export]
pub fn parse_point(s: String) -> Result<Point, String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 2 {
        return Err("expected format: x,y".to_string());
    }
    let x = parts[0]
        .trim()
        .parse::<f64>()
        .map_err(|_| "invalid x coordinate".to_string())?;
    let y = parts[1]
        .trim()
        .parse::<f64>()
        .map_err(|_| "invalid y coordinate".to_string())?;
    Ok(Point { x, y })
}

#[demo_bench_macros::demo_case(
    "results.basic.always_ok.should_return_doubled_value",
    description = "always_ok returns an Ok value containing the input doubled.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover string-error Result exports."
    )
)]
#[export]
pub fn always_ok(v: i32) -> Result<i32, String> {
    Ok(v * 2)
}

#[demo_bench_macros::demo_case(
    "results.basic.always_err.should_return_message_error",
    description = "always_err returns an error containing the caller-provided message.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover string-error Result exports."
    )
)]
#[export]
pub fn always_err(msg: String) -> Result<i32, String> {
    Err(msg)
}

#[demo_bench_macros::demo_case(
    "results.basic.result_to_string.should_render_ok",
    description = "result_to_string receives an Ok Result value over FFI and renders its success payload.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover Result parameters."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover Result parameters."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Result parameters."
    )
)]
#[demo_bench_macros::demo_case(
    "results.basic.result_to_string.should_render_err",
    description = "result_to_string receives an Err Result value over FFI and renders its error payload.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover Result parameters."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover Result parameters."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover Result parameters."
    )
)]
#[export]
pub fn result_to_string(v: Result<i32, String>) -> String {
    match v {
        Ok(val) => format!("ok: {}", val),
        Err(err) => format!("err: {}", err),
    }
}

#[demo_bench_macros::demo_case(
    "results.basic.divide.should_return_quotient",
    description = "divide returns the integer quotient when the divisor is non-zero.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover the divide alias."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover the divide alias."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover the divide alias."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover string-error Result exports."
    )
)]
#[demo_bench_macros::demo_case(
    "results.basic.divide.should_reject_division_by_zero",
    description = "divide returns a language-native error when the divisor is zero.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover the divide alias."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover the divide alias."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover the divide alias."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover string-error Result exports."
    )
)]
#[export]
pub fn divide(a: i32, b: i32) -> Result<i32, String> {
    safe_divide(a, b)
}

#[demo_bench_macros::demo_case(
    "results.basic.parse_int.should_parse_integer",
    description = "parse_int parses a decimal string into an i32 value.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover the top-level parse_int export."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover the top-level parse_int export."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests cover MathUtils.parseInt rather than the top-level parse_int export."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover string-error Result exports."
    )
)]
#[demo_bench_macros::demo_case(
    "results.basic.parse_int.should_reject_invalid_integer",
    description = "parse_int returns a language-native error when the string is not a valid i32.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover the top-level parse_int export."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover the top-level parse_int export."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests cover MathUtils.parseInt rather than the top-level parse_int export."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover string-error Result exports."
    )
)]
#[export]
pub fn parse_int(input: String) -> Result<i32, String> {
    input
        .parse::<i32>()
        .map_err(|_| "invalid integer".to_string())
}

#[demo_bench_macros::demo_case(
    "results.basic.validate_name.should_greet_valid_name",
    description = "validate_name returns a greeting for a non-empty name within the length limit.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover validate_name."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover validate_name."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover validate_name."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover string-error Result exports."
    )
)]
#[demo_bench_macros::demo_case(
    "results.basic.validate_name.should_reject_empty_name",
    description = "validate_name returns a language-native error when the provided name is empty.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover validate_name."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover validate_name."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover validate_name."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover string-error Result exports."
    )
)]
#[export]
pub fn validate_name(name: String) -> Result<String, String> {
    if name.is_empty() {
        Err("name cannot be empty".to_string())
    } else if name.len() > 100 {
        Err("name too long".to_string())
    } else {
        Ok(format!("Hello, {}!", name))
    }
}

#[cfg(test)]
mod tests {
    use boltffi::__private::wire;

    #[test]
    fn exported_result_string_parameter_round_trips() {
        let input_bytes = wire::encode(&Err::<i32, String>("bad".to_owned()));
        let output_buffer =
            unsafe { super::boltffi_result_to_string(input_bytes.as_ptr(), input_bytes.len()) };
        let output_bytes = unsafe { output_buffer.as_byte_slice() }.to_vec();
        drop(output_buffer);

        let output_string: String =
            wire::decode(&output_bytes).expect("exported result_to_string should decode");

        assert_eq!(output_string, "err: bad");
    }
}
