from tests.support import DemoTestCase

import demo


class EnumBackedTests(DemoTestCase):
    def test_traffic_light_methods_see_the_variant_behind_the_handle(self) -> None:
        red = demo.TrafficLight()

        self.demo_case("case:classes.enum_backed.traffic_light.seconds_left.should_match_on_the_variant_behind_the_handle")
        self.assertEqual(red.seconds_left(), 0)
        self.assertEqual(red.turn_green(30).seconds_left(), 30)
