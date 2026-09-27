from tests.support import DemoTestCase

import demo


class ErrorEnumResultTests(DemoTestCase):
    def test_error_messages(self) -> None:
        self.demo_case("case:results.error_enums.message.should_preserve_text")
        message = "service failed 東京\u0000🦀"
        self.assert_typed_error_value(
            demo.ServiceErrorException,
            demo.ServiceErrorFailed(message),
            lambda: demo.fail_with_message(message),
        )

        self.demo_case("case:results.error_enums.message.should_preserve_optional_text")
        self.assert_typed_error_value(
            demo.ServiceErrorException,
            demo.ServiceErrorOptional(message),
            lambda: demo.fail_with_optional_message(message),
        )
        self.assert_typed_error_value(
            demo.ServiceErrorException,
            demo.ServiceErrorOptional(None),
            lambda: demo.fail_with_optional_message(None),
        )
        self.assert_typed_error_value(
            demo.ServiceErrorException,
            demo.ServiceErrorOptional(""),
            lambda: demo.fail_with_optional_message(""),
        )

    def test_typed_result_returns(self) -> None:
        self.demo_case("case:results.error_enums.checked_divide.should_return_quotient")
        self.assertEqual(demo.checked_divide(10, 2), 5)
        self.demo_case("case:results.error_enums.checked_divide.should_reject_division_by_zero")
        self.assert_typed_error_value(
            demo.MathErrorException,
            demo.MathError.DIVISION_BY_ZERO,
            lambda: demo.checked_divide(10, 0),
        )
        self.demo_case("case:results.error_enums.checked_sqrt.should_return_square_root")
        self.assertEqual(demo.checked_sqrt(9.0), 3.0)
        self.demo_case("case:results.error_enums.checked_sqrt.should_reject_negative_input")
        self.assert_typed_error_value(
            demo.MathErrorException,
            demo.MathError.NEGATIVE_INPUT,
            lambda: demo.checked_sqrt(-1.0),
        )
        self.demo_case("case:results.error_enums.checked_add.should_return_sum")
        self.assertEqual(demo.checked_add(20, 22), 42)
        self.demo_case("case:results.error_enums.checked_add.should_reject_overflow")
        self.assert_typed_error_value(
            demo.MathErrorException,
            demo.MathError.OVERFLOW,
            lambda: demo.checked_add(2_147_483_647, 1),
        )
        self.demo_case("case:results.error_enums.may_fail.should_return_success_when_valid")
        self.assertEqual(demo.may_fail(True), "Success!")
        self.demo_case("case:results.error_enums.may_fail.should_return_app_error_when_invalid")
        self.assert_typed_error_value(
            demo.AppErrorException,
            demo.AppError(400, "Invalid input"),
            lambda: demo.may_fail(False),
        )
        self.demo_case("case:results.error_enums.divide_app.should_return_quotient")
        self.assertEqual(demo.divide_app(12, 3), 4)
        self.demo_case("case:results.error_enums.divide_app.should_return_app_error_for_division_by_zero")
        self.assert_typed_error_value(
            demo.AppErrorException,
            demo.AppError(500, "Division by zero"),
            lambda: demo.divide_app(12, 0),
        )
        self.demo_case("case:results.error_enums.validate_username.should_accept_valid_name")
        self.assertEqual(demo.validate_username("valid_name"), "valid_name")
        self.demo_case("case:results.error_enums.validate_username.should_reject_too_short_name")
        self.assert_typed_error_value(
            demo.ValidationErrorException,
            demo.ValidationError.TOO_SHORT,
            lambda: demo.validate_username("ab"),
        )
        self.demo_case("case:results.error_enums.validate_username.should_reject_too_long_name")
        self.assert_typed_error_value(
            demo.ValidationErrorException,
            demo.ValidationError.TOO_LONG,
            lambda: demo.validate_username("a" * 21),
        )
        self.demo_case("case:results.error_enums.validate_username.should_reject_invalid_format")
        self.assert_typed_error_value(
            demo.ValidationErrorException,
            demo.ValidationError.INVALID_FORMAT,
            lambda: demo.validate_username("bad name"),
        )
        self.demo_case("case:results.error_enums.try_compute.should_return_doubled_value")
        self.assertEqual(demo.try_compute(4), 8)
        self.demo_case("case:results.error_enums.try_compute.should_return_overflow_error")
        self.assert_typed_error_value(
            demo.ComputeErrorException,
            demo.ComputeErrorOverflow(-1, 0),
            lambda: demo.try_compute(-1),
        )

    def test_data_enum_returns(self) -> None:
        self.demo_case("case:results.error_enums.process_value.should_return_success_variant")
        self.assertEqual(demo.process_value(3), demo.ApiResultSuccess())
        self.demo_case("case:results.error_enums.process_value.should_return_error_code_variant")
        self.assertEqual(demo.process_value(0), demo.ApiResultErrorCode(-1))
        self.demo_case("case:results.error_enums.process_value.should_return_error_with_data_variant")
        self.assertEqual(demo.process_value(-3), demo.ApiResultErrorWithData(-3, -6))
        self.demo_case("case:results.error_enums.api_result_is_success.should_report_success_variant")
        self.assertIs(demo.api_result_is_success(demo.ApiResultSuccess()), True)
        self.demo_case("case:results.error_enums.api_result_is_success.should_report_error_variant")
        self.assertIs(demo.api_result_is_success(demo.ApiResultErrorCode(-1)), False)

    def test_success_response(self) -> None:
        point = demo.DataPoint(1.0, 2.0, 3)

        self.demo_case("case:results.error_enums.benchmark_response.should_make_success_response")
        self.assertEqual(demo.create_success_response(7, point), demo.BenchmarkResponse(7, (True, point)))
        self.demo_case("case:results.error_enums.benchmark_response.should_make_error_response")
        error = demo.ComputeErrorOverflow(-3, 0)
        error_envelope = demo.create_error_response(12, error)
        self.assertEqual(error_envelope, demo.BenchmarkResponse(12, (False, error)))
        success_envelope = demo.BenchmarkResponse(11, (True, demo.DataPoint(4.0, 5.0, 6)))
        self.demo_case("case:results.error_enums.benchmark_response.should_report_success_response")
        self.assertIs(demo.is_response_success(success_envelope), True)
        self.demo_case("case:results.error_enums.benchmark_response.should_report_error_response")
        self.assertIs(demo.is_response_success(error_envelope), False)
        self.demo_case("case:results.error_enums.benchmark_response.should_return_value_for_success_response")
        self.assertEqual(demo.get_response_value(success_envelope), demo.DataPoint(4.0, 5.0, 6))
        self.demo_case("case:results.error_enums.benchmark_response.should_return_none_for_error_response")
        self.assertIsNone(demo.get_response_value(error_envelope))
