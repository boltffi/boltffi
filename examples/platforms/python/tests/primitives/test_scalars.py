import math
from tests.support import DemoTestCase

import demo


class ScalarsTests(DemoTestCase):
    def test_echo_bool(self) -> None:
        self.assertIs(demo.echo_bool(True), True, "case:primitives.scalars.bool.should_roundtrip_true")

    def test_negate_bool(self) -> None:
        self.assertIs(demo.negate_bool(False), True, "case:primitives.scalars.bool.should_negate_false_to_true")

    def test_echo_i8(self) -> None:
        self.assertEqual(demo.echo_i8(-7), -7, "case:primitives.scalars.i8.should_roundtrip_negative_value")

    def test_echo_u8(self) -> None:
        self.assertEqual(demo.echo_u8(255), 255, "case:primitives.scalars.u8.should_roundtrip_max_value")

    def test_echo_i16(self) -> None:
        self.assertEqual(demo.echo_i16(-1234), -1234, "case:primitives.scalars.i16.should_roundtrip_negative_value")

    def test_echo_u16(self) -> None:
        self.assertEqual(demo.echo_u16(55_000), 55_000, "case:primitives.scalars.u16.should_roundtrip_large_value")

    def test_echo_i32(self) -> None:
        self.assertEqual(demo.echo_i32(-42), -42, "case:primitives.scalars.i32.should_roundtrip_negative_value")

    def test_add_i32(self) -> None:
        self.assertEqual(demo.add_i32(10, 20), 30, "case:primitives.scalars.i32.should_add_two_values")

    def test_echo_u32(self) -> None:
        self.assertEqual(demo.echo_u32(4_000_000_000), 4_000_000_000, "case:primitives.scalars.u32.should_roundtrip_large_value")

    def test_echo_i64(self) -> None:
        self.assertEqual(demo.echo_i64(-9_999_999_999), -9_999_999_999, "case:primitives.scalars.i64.should_roundtrip_large_negative_value")

    def test_echo_u64(self) -> None:
        self.assertEqual(demo.echo_u64(9_999_999_999), 9_999_999_999, "case:primitives.scalars.u64.should_roundtrip_large_value")

    def test_echo_f32(self) -> None:
        self.assertTrue(
            math.isclose(demo.echo_f32(3.5), 3.5, rel_tol=0.0, abs_tol=1e-6),
            "case:primitives.scalars.f32.should_roundtrip_value_with_tolerance",
        )

    def test_add_f32(self) -> None:
        self.assertTrue(
            math.isclose(demo.add_f32(1.5, 2.5), 4.0, rel_tol=0.0, abs_tol=1e-6),
            "case:primitives.scalars.f32.should_add_two_values_with_tolerance",
        )

    def test_echo_f64(self) -> None:
        self.assertTrue(
            math.isclose(
                demo.echo_f64(3.14159265359),
                3.14159265359,
                rel_tol=0.0,
                abs_tol=1e-12,
            ),
            "case:primitives.scalars.f64.should_roundtrip_pi_with_tolerance",
        )

    def test_add_f64(self) -> None:
        self.assertTrue(
            math.isclose(demo.add_f64(1.5, 2.5), 4.0, rel_tol=0.0, abs_tol=1e-12),
            "case:primitives.scalars.f64.should_add_two_values_with_tolerance",
        )

    def test_echo_usize(self) -> None:
        self.assertEqual(demo.echo_usize(123), 123, "case:primitives.scalars.usize.should_roundtrip_value")

    def test_echo_isize(self) -> None:
        self.assertEqual(demo.echo_isize(-123), -123, "case:primitives.scalars.isize.should_roundtrip_negative_value")
