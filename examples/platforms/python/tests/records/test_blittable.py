import math
from tests.support import DemoTestCase

import demo


class BlittableRecordsTests(DemoTestCase):
    def assert_point(
        self,
        point: demo.Point,
        *,
        x: float,
        y: float,
        tolerance: float = 1e-12,
    ) -> None:
        self.assertIsInstance(point, demo.Point)
        self.assertTrue(math.isclose(point.x, x, rel_tol=0.0, abs_tol=tolerance))
        self.assertTrue(math.isclose(point.y, y, rel_tol=0.0, abs_tol=tolerance))

    def test_point_surface(self) -> None:
        self.assert_point(demo.Point.new(1.0, 2.0), x=1.0, y=2.0)
        self.assert_point(demo.Point.origin(), x=0.0, y=0.0)
        self.assert_point(demo.Point.from_polar(2.0, math.pi / 2.0), x=0.0, y=2.0, tolerance=1e-9)
        self.assertEqual(demo.Point.dimensions(), 2)

    def test_point_instance_methods(self) -> None:
        point = demo.Point(3.0, 4.0)

        self.assertTrue(math.isclose(point.distance(), 5.0, rel_tol=0.0, abs_tol=1e-12))
        self.assert_point(point.scale(2.0), x=6.0, y=8.0)
        self.assert_point(point.add(demo.Point(5.0, 6.0)), x=8.0, y=10.0)

    def test_point_functions(self) -> None:
        self.demo_case("case:records.blittable.point.functions")
        point = demo.Point(1.0, 2.0)

        self.assert_point(demo.echo_point(point), x=1.0, y=2.0)
        self.assert_point(demo.make_point(1.0, 2.0), x=1.0, y=2.0)
        self.assert_point(
            demo.add_points(demo.Point(3.0, 4.0), demo.Point(5.0, 6.0)),
            x=8.0,
            y=10.0,
        )

    def test_color_functions(self) -> None:
        self.demo_case("case:records.blittable.color.basic")
        color = demo.Color(1, 2, 3, 255)

        self.assertEqual(demo.echo_color(color), color)
        self.assertEqual(demo.make_color(9, 8, 7, 6), demo.Color(9, 8, 7, 6))
