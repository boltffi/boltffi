from tests.support import DemoTestCase

import demo


class ComplexOptionsTests(DemoTestCase):
    def test_optional_strings(self) -> None:
        self.demo_case("case:options.complex.string.should_roundtrip_some")
        self.assertEqual(demo.echo_optional_string("hello"), "hello")
        self.demo_case("case:options.complex.string.should_roundtrip_none")
        self.assertIsNone(demo.echo_optional_string(None))
        self.demo_case("case:options.complex.string.should_report_some")
        self.assertIs(demo.is_some_string("x"), True)
        self.demo_case("case:options.complex.string.should_report_none")
        self.assertIs(demo.is_some_string(None), False)
        self.demo_case("case:options.complex.string.should_find_name_for_positive_id")
        self.assertEqual(demo.find_name(7), "Name_7")
        self.demo_case("case:options.complex.string.should_return_none_for_non_positive_id")
        self.assertIsNone(demo.find_name(0))

    def test_optional_point(self) -> None:
        point = demo.Point(1.0, 2.0)

        self.demo_case("case:options.complex.point.should_roundtrip_some")
        self.assertEqual(demo.echo_optional_point(point), point)
        self.demo_case("case:options.complex.point.should_roundtrip_none")
        self.assertIsNone(demo.echo_optional_point(None))
        self.demo_case("case:options.complex.point.should_make_some")
        self.assertEqual(demo.make_some_point(3.0, 4.0), demo.Point(3.0, 4.0))
        self.demo_case("case:options.complex.point.should_make_none")
        self.assertIsNone(demo.make_none_point())

    def test_optional_status(self) -> None:
        self.demo_case("case:options.complex.status.should_roundtrip_some")
        self.assertEqual(demo.echo_optional_status(demo.Status.ACTIVE), demo.Status.ACTIVE)
        self.demo_case("case:options.complex.status.should_roundtrip_none")
        self.assertIsNone(demo.echo_optional_status(None))

    def test_optional_vecs(self) -> None:
        self.demo_case("case:options.complex.vec.should_roundtrip_some")
        self.assertEqual(demo.echo_optional_vec([1, 2, 3]), [1, 2, 3])
        self.demo_case("case:options.complex.vec.should_roundtrip_none")
        self.assertIsNone(demo.echo_optional_vec(None))
        self.demo_case("case:options.complex.vec.should_roundtrip_empty_some")
        self.assertEqual(demo.echo_optional_vec([]), [])
        self.demo_case("case:options.complex.vec.should_report_length_for_some")
        self.assertEqual(demo.optional_vec_length([9, 8]), 2)
        self.demo_case("case:options.complex.vec.should_return_none_for_absent_length")
        self.assertIsNone(demo.optional_vec_length(None))
        self.demo_case("case:options.complex.vec.should_find_numbers_for_positive_count")
        self.assertEqual(demo.find_numbers(3), [0, 1, 2])
        self.demo_case("case:options.complex.vec.should_return_none_for_non_positive_number_count")
        self.assertIsNone(demo.find_numbers(0))
        self.demo_case("case:options.complex.vec_string.should_find_names_for_positive_count")
        self.assertEqual(demo.find_names(2), ["Name_0", "Name_1"])
        self.demo_case("case:options.complex.vec_string.should_return_none_for_non_positive_name_count")
        self.assertIsNone(demo.find_names(0))

    def test_optional_data_enum(self) -> None:
        self.demo_case("case:options.complex.api_result.should_find_success_variant")
        self.assertEqual(demo.find_api_result(0), demo.ApiResultSuccess())
        self.demo_case("case:options.complex.api_result.should_find_error_code_variant")
        self.assertEqual(demo.find_api_result(1), demo.ApiResultErrorCode(-1))
        self.demo_case("case:options.complex.api_result.should_find_error_with_data_variant")
        self.assertEqual(demo.find_api_result(2), demo.ApiResultErrorWithData(-1, -2))
        self.demo_case("case:options.complex.api_result.should_return_none_for_unknown_code")
        self.assertIsNone(demo.find_api_result(99))

    def test_vec_optional_i32(self) -> None:
        self.demo_case("case:options.complex.vec_optional_i32.should_roundtrip_mixed_presence")
        self.assertEqual(demo.echo_vec_optional_i32([1, None, 2, None, 3]), [1, None, 2, None, 3])
        self.demo_case("case:options.complex.vec_optional_i32.should_roundtrip_empty")
        self.assertEqual(demo.echo_vec_optional_i32([]), [])
        self.demo_case("case:options.complex.vec_optional_i32.should_roundtrip_all_none")
        self.assertEqual(demo.echo_vec_optional_i32([None, None, None]), [None, None, None])

    def test_optional_shape_radius(self) -> None:
        self.demo_case("case:options.complex.shape.should_return_radius_for_circle")
        self.assertEqual(demo.radius_if_circle(demo.ShapeCircle(5.0)), 5.0)
        self.demo_case("case:options.complex.shape.should_return_none_for_non_circle")
        self.assertIsNone(demo.radius_if_circle(demo.ShapeRectangle(3.0, 4.0)))
