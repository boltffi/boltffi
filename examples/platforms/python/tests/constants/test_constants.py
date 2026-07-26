from tests.support import DemoTestCase

import demo


class ConstantsTests(DemoTestCase):
    def test_associated_constants_stay_on_their_owner_types(self) -> None:
        self.demo_case(
            "case:constants.associated.should_expose_values_on_exported_types"
        )
        self.assertEqual(demo.DemoMode.PREFERRED, demo.DemoMode.SAFE)
        self.assertEqual(demo.DemoMode.FALLBACK, demo.DemoMode.SAFE)
        self.assertEqual(demo.DemoMode.VARIANT_COUNT, 2)
        self.assertIsInstance(demo.DemoState.INITIAL, demo.DemoStateIdle)
        self.assertEqual(demo.Point.ZERO, demo.Point(x=0.0, y=0.0))
        self.assertEqual(demo.MathUtils.DEFAULT_PRECISION, 2)
