#include <string.h>

#include "test.h"

bool test_results(void) {
    DemoSafeDivideResult quotient = demo_safe_divide(6, 2);
    CHECK(quotient.ok, "case:results.basic.safe_divide.should_return_quotient");
    CHECK(quotient.data.value == 3, "case:results.basic.safe_divide.should_return_quotient");
    demo_safe_divide_result_free(&quotient);

    DemoSafeDivideResult division_by_zero = demo_safe_divide(1, 0);
    CHECK(!division_by_zero.ok, "case:results.basic.safe_divide.should_reject_division_by_zero");
    const char expected_error[] = "division by zero";
    CHECK(division_by_zero.data.error.len == sizeof(expected_error) - 1,
          "case:results.basic.safe_divide.should_reject_division_by_zero");
    CHECK(memcmp(division_by_zero.data.error.ptr, expected_error, sizeof(expected_error) - 1) == 0,
          "case:results.basic.safe_divide.should_reject_division_by_zero");
    demo_safe_divide_result_free(&division_by_zero);

    DemoResultOfVecResult values = demo_result_of_vec(3);
    CHECK(values.ok && values.data.value.len == 3,
          "case:results.nested_results.vec.should_return_values_for_non_negative_count");
    const int32_t expected_values[] = {0, 1, 2};
    CHECK(memcmp(values.data.value.ptr, expected_values, sizeof(expected_values)) == 0,
          "case:results.nested_results.vec.should_return_values_for_non_negative_count");
    demo_result_of_vec_result_free(&values);

    const char valid_name[] = "ali";
    DemoValidateUsernameResult username = demo_validate_username(demo_string_view(valid_name, sizeof(valid_name) - 1));
    CHECK(username.ok && username.data.value.len == sizeof(valid_name) - 1,
          "case:results.error_enums.validate_username.should_accept_valid_name");
    CHECK(memcmp(username.data.value.ptr, valid_name, sizeof(valid_name) - 1) == 0,
          "case:results.error_enums.validate_username.should_accept_valid_name");
    demo_validate_username_result_free(&username);

    DemoValidateUsernameResult too_short = demo_validate_username(demo_string_view("ab", 2));
    CHECK(!too_short.ok && too_short.data.error == DEMO_VALIDATION_ERROR_TOO_SHORT,
          "case:results.error_enums.validate_username.should_reject_too_short_name");
    demo_validate_username_result_free(&too_short);

    const char long_name[] = "abcdefghijklmnopqrstuvwxyz";
    DemoValidateUsernameResult too_long = demo_validate_username(demo_string_view(long_name, sizeof(long_name) - 1));
    CHECK(!too_long.ok && too_long.data.error == DEMO_VALIDATION_ERROR_TOO_LONG,
          "case:results.error_enums.validate_username.should_reject_too_long_name");
    demo_validate_username_result_free(&too_long);

    DemoValidateUsernameResult invalid_format = demo_validate_username(demo_string_view("a b", 3));
    CHECK(!invalid_format.ok && invalid_format.data.error == DEMO_VALIDATION_ERROR_INVALID_FORMAT,
          "case:results.error_enums.validate_username.should_reject_invalid_format");
    demo_validate_username_result_free(&invalid_format);
    return true;
}

bool test_result_values(void) {
    DemoSafeSqrtResult root = demo_safe_sqrt(9.0);
    CHECK(root.ok && root.data.value == 3.0, "case:results.basic.safe_sqrt.should_return_square_root");
    demo_safe_sqrt_result_free(&root);
    root = demo_safe_sqrt(-1.0);
    CHECK(!root.ok && root.data.error.len == 14 && memcmp(root.data.error.ptr, "negative input", 14) == 0, "case:results.basic.safe_sqrt.should_reject_negative_input");
    demo_safe_sqrt_result_free(&root);
    DemoParsePointResult point = demo_parse_point((DemoStringView){"1.5,-2.5", 8});
    CHECK(point.ok && point.data.value.x == 1.5 && point.data.value.y == -2.5, "case:results.basic.parse_point.should_parse_coordinates");
    demo_parse_point_result_free(&point);
    point = demo_parse_point((DemoStringView){"bad", 3});
    CHECK(!point.ok && point.data.error.len == 20 && memcmp(point.data.error.ptr, "expected format: x,y", 20) == 0, "case:results.basic.parse_point.should_reject_malformed_input");
    demo_parse_point_result_free(&point);
    DemoAlwaysOkResult doubled = demo_always_ok(21);
    CHECK(doubled.ok && doubled.data.value == 42, "case:results.basic.always_ok.should_return_doubled_value");
    demo_always_ok_result_free(&doubled);
    DemoAlwaysErrResult failed = demo_always_err((DemoStringView){"failed", 6});
    CHECK(!failed.ok && failed.data.error.len == 6 && memcmp(failed.data.error.ptr, "failed", 6) == 0, "case:results.basic.always_err.should_return_message_error");
    demo_always_err_result_free(&failed);
    CHECK_STRING(demo_result_to_string((DemoResultI32OrStringView){.ok = true, .data.value = 42}), "ok: 42", "case:results.basic.result_to_string.should_render_ok");
    CHECK_STRING(demo_result_to_string((DemoResultI32OrStringView){.ok = false, .data.error = {"failed", 6}}), "err: failed", "case:results.basic.result_to_string.should_render_err");
    DemoDivideResult divided = demo_divide(12, 3);
    CHECK(divided.ok && divided.data.value == 4, "case:results.basic.divide.should_return_quotient");
    demo_divide_result_free(&divided);
    divided = demo_divide(12, 0);
    CHECK(!divided.ok && divided.data.error.len == 16 && memcmp(divided.data.error.ptr, "division by zero", 16) == 0, "case:results.basic.divide.should_reject_division_by_zero");
    demo_divide_result_free(&divided);
    DemoParseIntResult parsed = demo_parse_int((DemoStringView){"-42", 3});
    CHECK(parsed.ok && parsed.data.value == -42, "case:results.basic.parse_int.should_parse_integer");
    demo_parse_int_result_free(&parsed);
    parsed = demo_parse_int((DemoStringView){"not a number", 12});
    CHECK(!parsed.ok && parsed.data.error.len == 15 && memcmp(parsed.data.error.ptr, "invalid integer", 15) == 0, "case:results.basic.parse_int.should_reject_invalid_integer");
    demo_parse_int_result_free(&parsed);
    DemoIsEvenResult even = demo_is_even(42);
    DemoIsEvenResult odd = demo_is_even(43);
    CHECK(even.ok && even.data.value && odd.ok && !odd.data.value, "case:results.basic.is_even.should_return_parity");
    demo_is_even_result_free(&even);
    demo_is_even_result_free(&odd);
    even = demo_is_even(-1);
    CHECK(!even.ok && even.data.error.len == 14 && memcmp(even.data.error.ptr, "negative input", 14) == 0, "case:results.basic.is_even.should_reject_negative_input");
    demo_is_even_result_free(&even);
    DemoValidateNameResult name = demo_validate_name((DemoStringView){"Ali", 3});
    CHECK(name.ok && name.data.value.len == 11 && memcmp(name.data.value.ptr, "Hello, Ali!", 11) == 0, "case:results.basic.validate_name.should_greet_valid_name");
    demo_validate_name_result_free(&name);
    name = demo_validate_name((DemoStringView){NULL, 0});
    CHECK(!name.ok && name.data.error.len == 20 && memcmp(name.data.error.ptr, "name cannot be empty", 20) == 0, "case:results.basic.validate_name.should_reject_empty_name");
    demo_validate_name_result_free(&name);
    DemoResultOfOptionResult optional = demo_result_of_option(21);
    CHECK(optional.ok && optional.data.value.has_value && optional.data.value.value == 42, "case:results.nested_results.option.should_return_some_for_positive_key");
    demo_result_of_option_result_free(&optional);
    optional = demo_result_of_option(0);
    CHECK(optional.ok && !optional.data.value.has_value, "case:results.nested_results.option.should_return_none_for_zero_key");
    demo_result_of_option_result_free(&optional);
    optional = demo_result_of_option(-1);
    CHECK(!optional.ok && optional.data.error.len == 11 && memcmp(optional.data.error.ptr, "invalid key", 11) == 0, "case:results.nested_results.option.should_reject_negative_key");
    demo_result_of_option_result_free(&optional);
    DemoResultOfVecResult vector = demo_result_of_vec(-1);
    CHECK(!vector.ok && vector.data.error.len == 14 && memcmp(vector.data.error.ptr, "negative count", 14) == 0, "case:results.nested_results.vec.should_reject_negative_count");
    demo_result_of_vec_result_free(&vector);
    DemoResultOfStringResult text = demo_result_of_string(42);
    CHECK(text.ok && text.data.value.len == 7 && memcmp(text.data.value.ptr, "item_42", 7) == 0, "case:results.nested_results.string.should_return_value_for_non_negative_key");
    demo_result_of_string_result_free(&text);
    text = demo_result_of_string(-1);
    CHECK(!text.ok && text.data.error.len == 11 && memcmp(text.data.error.ptr, "invalid key", 11) == 0, "case:results.nested_results.string.should_reject_negative_key");
    demo_result_of_string_result_free(&text);
    return true;
}

bool test_error_values(void) {
    const char message[] = "service failed 東京\0🦀";
    DemoStringView message_view = demo_string_view(message, sizeof(message) - 1);
    DemoFailWithMessageResult message_error = demo_fail_with_message(message_view);
    CHECK(!message_error.ok && message_error.data.error.tag == DEMO_SERVICE_ERROR_FAILED,
          "case:results.error_enums.message.should_preserve_text");
    DemoString returned_message = message_error.data.error.data.failed.message;
    CHECK(returned_message.len == message_view.len && memcmp(returned_message.ptr, message, message_view.len) == 0,
          "case:results.error_enums.message.should_preserve_text");
    demo_fail_with_message_result_free(&message_error);

    DemoFailWithOptionalMessageResult optional_error = demo_fail_with_optional_message(
        (DemoOptionStringView){.has_value = true, .value = message_view});
    CHECK(!optional_error.ok && optional_error.data.error.tag == DEMO_SERVICE_ERROR_OPTIONAL,
          "case:results.error_enums.message.should_preserve_optional_text");
    DemoOptionString returned_optional = optional_error.data.error.data.optional.message;
    CHECK(returned_optional.has_value && returned_optional.value.len == message_view.len
          && memcmp(returned_optional.value.ptr, message, message_view.len) == 0,
          "case:results.error_enums.message.should_preserve_optional_text");
    demo_fail_with_optional_message_result_free(&optional_error);
    optional_error = demo_fail_with_optional_message((DemoOptionStringView){0});
    CHECK(!optional_error.ok && optional_error.data.error.tag == DEMO_SERVICE_ERROR_OPTIONAL
          && !optional_error.data.error.data.optional.message.has_value,
          "case:results.error_enums.message.should_preserve_optional_text");
    demo_fail_with_optional_message_result_free(&optional_error);
    optional_error = demo_fail_with_optional_message(
        (DemoOptionStringView){.has_value = true, .value = {"", 0}});
    CHECK(!optional_error.ok && optional_error.data.error.tag == DEMO_SERVICE_ERROR_OPTIONAL
          && optional_error.data.error.data.optional.message.has_value
          && optional_error.data.error.data.optional.message.value.len == 0,
          "case:results.error_enums.message.should_preserve_optional_text");
    demo_fail_with_optional_message_result_free(&optional_error);

    DemoCheckedDivideResult divided = demo_checked_divide(12, 3);
    CHECK(divided.ok && divided.data.value == 4, "case:results.error_enums.checked_divide.should_return_quotient");
    demo_checked_divide_result_free(&divided);
    divided = demo_checked_divide(12, 0);
    CHECK(!divided.ok && divided.data.error == DEMO_MATH_ERROR_DIVISION_BY_ZERO, "case:results.error_enums.checked_divide.should_reject_division_by_zero");
    demo_checked_divide_result_free(&divided);
    DemoCheckedSqrtResult root = demo_checked_sqrt(16.0);
    CHECK(root.ok && root.data.value == 4.0, "case:results.error_enums.checked_sqrt.should_return_square_root");
    demo_checked_sqrt_result_free(&root);
    root = demo_checked_sqrt(-1.0);
    CHECK(!root.ok && root.data.error == DEMO_MATH_ERROR_NEGATIVE_INPUT, "case:results.error_enums.checked_sqrt.should_reject_negative_input");
    demo_checked_sqrt_result_free(&root);
    DemoCheckedAddResult added = demo_checked_add(20, 22);
    CHECK(added.ok && added.data.value == 42, "case:results.error_enums.checked_add.should_return_sum");
    demo_checked_add_result_free(&added);
    added = demo_checked_add(INT32_MAX, 1);
    CHECK(!added.ok && added.data.error == DEMO_MATH_ERROR_OVERFLOW, "case:results.error_enums.checked_add.should_reject_overflow");
    demo_checked_add_result_free(&added);
    DemoMayFailResult status = demo_may_fail(true);
    CHECK(status.ok && status.data.value.len == 8 && memcmp(status.data.value.ptr, "Success!", 8) == 0, "case:results.error_enums.may_fail.should_return_success_when_valid");
    demo_may_fail_result_free(&status);
    status = demo_may_fail(false);
    CHECK(!status.ok && status.data.error.code == 400 && status.data.error.message.len == 13 && memcmp(status.data.error.message.ptr, "Invalid input", 13) == 0, "case:results.error_enums.may_fail.should_return_app_error_when_invalid");
    demo_may_fail_result_free(&status);
    DemoDivideAppResult application = demo_divide_app(12, 3);
    CHECK(application.ok && application.data.value == 4, "case:results.error_enums.divide_app.should_return_quotient");
    demo_divide_app_result_free(&application);
    application = demo_divide_app(12, 0);
    CHECK(!application.ok && application.data.error.code == 500 && application.data.error.message.len == 16 && memcmp(application.data.error.message.ptr, "Division by zero", 16) == 0, "case:results.error_enums.divide_app.should_return_app_error_for_division_by_zero");
    demo_divide_app_result_free(&application);
    DemoApiResult result = demo_process_value(1);
    CHECK(result.tag == DEMO_API_RESULT_SUCCESS, "case:results.error_enums.process_value.should_return_success_variant");
    CHECK(demo_api_result_is_success(result), "case:results.error_enums.api_result_is_success.should_report_success_variant");
    result = demo_process_value(0);
    CHECK(result.tag == DEMO_API_RESULT_ERROR_CODE && result.data.error_code.field_0 == -1, "case:results.error_enums.process_value.should_return_error_code_variant");
    CHECK(!demo_api_result_is_success(result), "case:results.error_enums.api_result_is_success.should_report_error_variant");
    result = demo_process_value(-2);
    CHECK(result.tag == DEMO_API_RESULT_ERROR_WITH_DATA && result.data.error_with_data.code == -2 && result.data.error_with_data.detail == -4, "case:results.error_enums.process_value.should_return_error_with_data_variant");
    DemoTryComputeResult computed = demo_try_compute(21);
    CHECK(computed.ok && computed.data.value == 42, "case:results.error_enums.try_compute.should_return_doubled_value");
    demo_try_compute_result_free(&computed);
    computed = demo_try_compute(-42);
    CHECK(!computed.ok && computed.data.error.tag == DEMO_COMPUTE_ERROR_OVERFLOW && computed.data.error.data.overflow.value == -42 && computed.data.error.data.overflow.limit == 0, "case:results.error_enums.try_compute.should_return_overflow_error");
    demo_try_compute_result_free(&computed);
    const DemoDataPoint point = {42, 3.5, 123456};
    DemoBenchmarkResponse success = demo_create_success_response(7, point);
    CHECK(success.request_id == 7 && success.result.ok && success.result.data.value.x == 42 && success.result.data.value.y == 3.5 && success.result.data.value.timestamp == 123456, "case:results.error_enums.benchmark_response.should_make_success_response");
    const DemoComputeError error = {.tag = DEMO_COMPUTE_ERROR_OVERFLOW, .data.overflow = {-42, 0}};
    DemoBenchmarkResponse failure = demo_create_error_response(8, error);
    CHECK(failure.request_id == 8 && !failure.result.ok && failure.result.data.error.tag == DEMO_COMPUTE_ERROR_OVERFLOW && failure.result.data.error.data.overflow.value == -42, "case:results.error_enums.benchmark_response.should_make_error_response");
    CHECK(demo_is_response_success(success), "case:results.error_enums.benchmark_response.should_report_success_response");
    CHECK(!demo_is_response_success(failure), "case:results.error_enums.benchmark_response.should_report_error_response");
    DemoOptionDataPoint value = demo_get_response_value(success);
    CHECK(value.has_value && value.value.x == 42 && value.value.y == 3.5 && value.value.timestamp == 123456, "case:results.error_enums.benchmark_response.should_return_value_for_success_response");
    CHECK(!demo_get_response_value(failure).has_value, "case:results.error_enums.benchmark_response.should_return_none_for_error_response");
    return true;
}
