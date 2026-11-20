from tests.support import DemoTestCase

import demo


class TransparentVariantTests(DemoTestCase):
    def test_payload_records_are_the_variants(self) -> None:
        self.demo_case("case:enums.transparent.waypoint.should_roundtrip_a_blittable_payload")
        self.assertEqual(demo.echo_waypoint(demo.Point(3.0, 4.0)), demo.Point(3.0, 4.0))
        self.demo_case("case:enums.transparent.waypoint.should_roundtrip_an_encoded_payload")
        self.assertEqual(demo.echo_waypoint(demo.Label("north")), demo.Label("north"))

    def test_wrapped_and_unit_variants_still_roundtrip(self) -> None:
        self.demo_case("case:enums.transparent.waypoint.should_roundtrip_the_wrapped_and_unit_variants")
        self.assertEqual(demo.echo_waypoint(demo.WaypointNote("here")), demo.WaypointNote("here"))
        self.assertEqual(demo.echo_waypoint(demo.WaypointUnset()), demo.WaypointUnset())

    def test_one_payload_crosses_under_each_enums_own_tag(self) -> None:
        self.demo_case("case:enums.transparent.anchor.should_carry_the_shared_payload_under_its_own_tag")
        point = demo.Point(1.0, 2.0)
        # the same value, two enums, two tags — recovered from the payload type
        self.assertEqual(demo.echo_waypoint(point), point)
        self.assertEqual(demo.echo_anchor(point), point)
        # and it belongs to both, so a caller can narrow on the enum
        self.assertIsInstance(point, demo.Waypoint)
        self.assertIsInstance(point, demo.Anchor)
        self.assertNotIsInstance(demo.Label("north"), demo.Anchor)
        self.assertEqual(demo.echo_anchor(demo.AnchorOrigin()), demo.AnchorOrigin())

    def test_c_style_enum_payloads_are_the_variants(self) -> None:
        self.demo_case("case:enums.transparent.course.should_roundtrip_a_c_style_enum_payload")
        self.assertIs(demo.echo_course(demo.Heading.WEST), demo.Heading.WEST)
        self.assertIsInstance(demo.Heading.WEST, demo.Course)
        # a wrapped variant reusing the enum keeps its own class
        self.assertEqual(demo.echo_course(demo.CourseBearing(demo.Heading.EAST)), demo.CourseBearing(demo.Heading.EAST))
