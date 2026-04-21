import math
import unittest

import demo


class PrimitiveVecsTests(unittest.TestCase):
    def test_echo_vec_i32(self) -> None:
        self.assertEqual(demo.echo_vec_i32([1, 2, 3]), [1, 2, 3])
        self.assertEqual(demo.echo_vec_i32([]), [])

    def test_sum_vec_i32(self) -> None:
        self.assertEqual(demo.sum_vec_i32([10, 20, 30]), 60)
        self.assertEqual(demo.sum_vec_i32([]), 0)

    def test_echo_vec_f64(self) -> None:
        values = demo.echo_vec_f64([1.5, 2.5])
        self.assertEqual(len(values), 2)
        self.assertTrue(math.isclose(values[0], 1.5, rel_tol=0.0, abs_tol=1e-12))
        self.assertTrue(math.isclose(values[1], 2.5, rel_tol=0.0, abs_tol=1e-12))

    def test_echo_vec_bool(self) -> None:
        self.assertEqual(demo.echo_vec_bool([True, False, True]), [True, False, True])

    def test_echo_vec_i8(self) -> None:
        self.assertEqual(demo.echo_vec_i8([-1, 0, 7]), [-1, 0, 7])

    def test_echo_vec_u8(self) -> None:
        self.assertEqual(demo.echo_vec_u8(bytes([0, 1, 2, 3])), bytes([0, 1, 2, 3]))

    def test_echo_vec_i16(self) -> None:
        self.assertEqual(demo.echo_vec_i16([-3, 0, 9]), [-3, 0, 9])

    def test_echo_vec_u16(self) -> None:
        self.assertEqual(demo.echo_vec_u16([0, 10, 20]), [0, 10, 20])

    def test_echo_vec_u32(self) -> None:
        self.assertEqual(demo.echo_vec_u32([0, 10, 20]), [0, 10, 20])

    def test_echo_vec_i64(self) -> None:
        self.assertEqual(demo.echo_vec_i64([-5, 0, 8]), [-5, 0, 8])

    def test_echo_vec_u64(self) -> None:
        self.assertEqual(demo.echo_vec_u64([0, 1, 2]), [0, 1, 2])

    def test_echo_vec_isize(self) -> None:
        self.assertEqual(demo.echo_vec_isize([-2, 0, 5]), [-2, 0, 5])

    def test_echo_vec_usize(self) -> None:
        self.assertEqual(demo.echo_vec_usize([0, 2, 4]), [0, 2, 4])

    def test_echo_vec_f32(self) -> None:
        values = demo.echo_vec_f32([1.25, -2.5])
        self.assertEqual(len(values), 2)
        self.assertTrue(math.isclose(values[0], 1.25, rel_tol=0.0, abs_tol=1e-6))
        self.assertTrue(math.isclose(values[1], -2.5, rel_tol=0.0, abs_tol=1e-6))

    def test_make_range(self) -> None:
        self.assertEqual(demo.make_range(0, 5), [0, 1, 2, 3, 4])

    def test_reverse_vec_i32(self) -> None:
        self.assertEqual(demo.reverse_vec_i32([1, 2, 3]), [3, 2, 1])
