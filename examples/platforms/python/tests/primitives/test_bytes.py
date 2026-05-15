from tests.support import DemoTestCase

import demo


class BytesTests(DemoTestCase):
    def test_echo_bytes(self) -> None:
        self.demo_case("case:bytes.bytes.should_roundtrip_values")
        self.assertEqual(demo.echo_bytes(bytes([1, 2, 3, 4])), bytes([1, 2, 3, 4]))

    def test_bytes_length(self) -> None:
        self.demo_case("case:bytes.bytes.should_report_length")
        self.assertEqual(demo.bytes_length(bytes([10, 20, 30])), 3)

    def test_bytes_sum(self) -> None:
        self.demo_case("case:bytes.bytes.should_sum_values")
        self.assertEqual(demo.bytes_sum(bytes([1, 2, 3, 4])), 10)

    def test_make_bytes(self) -> None:
        self.demo_case("case:bytes.bytes.should_make_sequential_values")
        self.assertEqual(demo.make_bytes(5), bytes([0, 1, 2, 3, 4]))

    def test_reverse_bytes(self) -> None:
        self.demo_case("case:bytes.bytes.should_reverse_values")
        self.assertEqual(demo.reverse_bytes(bytes([5, 6, 7])), bytes([7, 6, 5]))
