from enum import IntEnum
from tests.support import DemoTestCase

import demo


class CStyleEnumsTests(DemoTestCase):
    def test_status_functions(self) -> None:
        self.demo_case("case:enums.c_style.status.basic")
        self.assertEqual(demo.echo_status(demo.Status.ACTIVE), demo.Status.ACTIVE)
        self.assertEqual(demo.status_to_string(demo.Status.ACTIVE), "active")
        self.assertIs(demo.is_active(demo.Status.PENDING), False)

        self.demo_case("case:enums.c_style.status.vec")
        self.assertEqual(
            demo.echo_vec_status([demo.Status.ACTIVE, demo.Status.PENDING]),
            [demo.Status.ACTIVE, demo.Status.PENDING],
        )

    def test_direction_surface(self) -> None:
        self.demo_case("case:enums.c_style.direction.basic")
        self.assertEqual(demo.Direction.new(3), demo.Direction.WEST)
        self.assertEqual(demo.Direction.cardinal(), demo.Direction.NORTH)
        self.assertEqual(demo.Direction.from_degrees(90.0), demo.Direction.EAST)
        self.assertEqual(demo.Direction.from_degrees(225.0), demo.Direction.WEST)
        self.assertEqual(demo.Direction.NORTH.opposite(), demo.Direction.SOUTH)
        self.assertIs(demo.Direction.WEST.is_horizontal(), True)
        self.assertIs(demo.Direction.NORTH.is_horizontal(), False)
        self.assertEqual(demo.Direction.SOUTH.label(), "S")
        self.assertEqual(demo.Direction.count(), 4)
        self.assertEqual(demo.echo_direction(demo.Direction.EAST), demo.Direction.EAST)
        self.assertEqual(
            demo.opposite_direction(demo.Direction.EAST),
            demo.Direction.WEST,
        )

    def test_repr_int_enums(self) -> None:
        self.demo_case("case:enums.repr_int.priority.basic")
        self.assertEqual(demo.echo_priority(demo.Priority.HIGH), demo.Priority.HIGH)
        self.assertEqual(demo.priority_label(demo.Priority.LOW), "low")
        self.assertIs(demo.is_high_priority(demo.Priority.CRITICAL), True)
        self.assertIs(demo.is_high_priority(demo.Priority.LOW), False)

        self.demo_case("case:enums.repr_int.log_level.basic")
        self.assertEqual(demo.echo_log_level(demo.LogLevel.INFO), demo.LogLevel.INFO)
        self.assertIs(demo.should_log(demo.LogLevel.ERROR, demo.LogLevel.WARN), True)
        self.assertIs(demo.should_log(demo.LogLevel.DEBUG, demo.LogLevel.INFO), False)

        self.demo_case("case:enums.repr_int.log_level.vec")
        self.assertEqual(
            demo.echo_vec_log_level(
                [demo.LogLevel.TRACE, demo.LogLevel.INFO, demo.LogLevel.ERROR]
            ),
            [demo.LogLevel.TRACE, demo.LogLevel.INFO, demo.LogLevel.ERROR],
        )

    def test_rejects_plain_ints_for_enum_parameters(self) -> None:
        with self.assertRaises(TypeError):
            demo.echo_status(0)

        with self.assertRaises(TypeError):
            demo.echo_vec_status([0])

    def test_rejects_registration_with_wrong_enum_values(self) -> None:
        class WrongDirection(IntEnum):
            NORTH = 10
            SOUTH = 11
            EAST = 12
            WEST = 13

        try:
            with self.assertRaises(ValueError):
                demo._native._register_direction(WrongDirection)
        finally:
            demo._native._register_direction(demo.Direction)
