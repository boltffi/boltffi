use boltffi::*;

use crate::records::blittable::Point;

#[demo_bench_macros::demo_case(
    "results.basic.safe_divide.should_return_quotient",
    justification = "Ensure safe_divide returns the integer quotient when the divisor is non-zero.",
    directions = "Call `results::basic::safe_divide` through the generated binding and assert safe_divide returns the integer quotient when the divisor is non-zero."
)]
#[demo_bench_macros::demo_case(
    "results.basic.safe_divide.should_reject_division_by_zero",
    justification = "Ensure safe_divide returns a language-native error when the divisor is zero.",
    directions = "Call `results::basic::safe_divide` through the generated binding and assert safe_divide returns a language-native error when the divisor is zero."
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
    justification = "Ensure safe_sqrt returns the square root for non-negative floating-point input.",
    directions = "Call `results::basic::safe_sqrt` through the generated binding and assert safe_sqrt returns the square root for non-negative floating-point input."
)]
#[demo_bench_macros::demo_case(
    "results.basic.safe_sqrt.should_reject_negative_input",
    justification = "Ensure safe_sqrt returns a language-native error for negative floating-point input.",
    directions = "Call `results::basic::safe_sqrt` through the generated binding and assert safe_sqrt returns a language-native error for negative floating-point input."
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
    justification = "Ensure parse_point parses a comma-separated coordinate string into a Point record.",
    directions = "Call `results::basic::parse_point` through the generated binding and assert parse_point parses a comma-separated coordinate string into a Point record."
)]
#[demo_bench_macros::demo_case(
    "results.basic.parse_point.should_reject_malformed_input",
    justification = "Ensure parse_point returns a language-native error when the input is not a coordinate pair.",
    directions = "Call `results::basic::parse_point` through the generated binding and assert parse_point returns a language-native error when the input is not a coordinate pair."
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
    justification = "Ensure always_ok returns an Ok value containing the input doubled.",
    directions = "Call `results::basic::always_ok` through the generated binding and assert always_ok returns an Ok value containing the input doubled."
)]
#[export]
pub fn always_ok(v: i32) -> Result<i32, String> {
    Ok(v * 2)
}

#[demo_bench_macros::demo_case(
    "results.basic.always_err.should_return_message_error",
    justification = "Ensure always_err returns an error containing the caller-provided message.",
    directions = "Call `results::basic::always_err` through the generated binding and assert always_err returns an error containing the caller-provided message."
)]
#[export]
pub fn always_err(msg: String) -> Result<i32, String> {
    Err(msg)
}

#[demo_bench_macros::demo_case(
    "results.basic.result_to_string.should_render_ok",
    justification = "Ensure result_to_string receives an Ok Result value over FFI and renders its success payload.",
    directions = "Call `results::basic::result_to_string` through the generated binding and assert result_to_string receives an Ok Result value over FFI and renders its success payload."
)]
#[demo_bench_macros::demo_case(
    "results.basic.result_to_string.should_render_err",
    justification = "Ensure result_to_string receives an Err Result value over FFI and renders its error payload.",
    directions = "Call `results::basic::result_to_string` through the generated binding and assert result_to_string receives an Err Result value over FFI and renders its error payload."
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
    justification = "Ensure divide returns the integer quotient when the divisor is non-zero.",
    directions = "Call `results::basic::divide` through the generated binding and assert divide returns the integer quotient when the divisor is non-zero."
)]
#[demo_bench_macros::demo_case(
    "results.basic.divide.should_reject_division_by_zero",
    justification = "Ensure divide returns a language-native error when the divisor is zero.",
    directions = "Call `results::basic::divide` through the generated binding and assert divide returns a language-native error when the divisor is zero."
)]
#[export]
pub fn divide(a: i32, b: i32) -> Result<i32, String> {
    safe_divide(a, b)
}

#[demo_bench_macros::demo_case(
    "results.basic.parse_int.should_parse_integer",
    justification = "Ensure parse_int parses a decimal string into an i32 value.",
    directions = "Call `results::basic::parse_int` through the generated binding and assert parse_int parses a decimal string into an i32 value."
)]
#[demo_bench_macros::demo_case(
    "results.basic.parse_int.should_reject_invalid_integer",
    justification = "Ensure parse_int returns a language-native error when the string is not a valid i32.",
    directions = "Call `results::basic::parse_int` through the generated binding and assert parse_int returns a language-native error when the string is not a valid i32."
)]
#[export]
pub fn parse_int(input: String) -> Result<i32, String> {
    input
        .parse::<i32>()
        .map_err(|_| "invalid integer".to_string())
}

#[demo_bench_macros::demo_case(
    "results.basic.is_even.should_return_parity",
    justification = "Ensure is_even delivers both bool payloads through the Result wire envelope, since a fallible bool return crosses the FFI as an encoded buffer rather than a primitive bool.",
    directions = "Call `results::basic::is_even` through the generated binding and assert it returns true for an even input and false for an odd input."
)]
#[demo_bench_macros::demo_case(
    "results.basic.is_even.should_reject_negative_input",
    justification = "Ensure is_even returns a language-native error when the input is negative.",
    directions = "Call `results::basic::is_even` through the generated binding and assert is_even returns a language-native error when the input is negative."
)]
#[export]
pub fn is_even(value: i32) -> Result<bool, String> {
    if value < 0 {
        Err("negative input".to_string())
    } else {
        Ok(value % 2 == 0)
    }
}

#[demo_bench_macros::demo_case(
    "results.basic.validate_name.should_greet_valid_name",
    justification = "Ensure validate_name returns a greeting for a non-empty name within the length limit.",
    directions = "Call `results::basic::validate_name` through the generated binding and assert validate_name returns a greeting for a non-empty name within the length limit."
)]
#[demo_bench_macros::demo_case(
    "results.basic.validate_name.should_reject_empty_name",
    justification = "Ensure validate_name returns a language-native error when the provided name is empty.",
    directions = "Call `results::basic::validate_name` through the generated binding and assert validate_name returns a language-native error when the provided name is empty."
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
