from tests.support import DemoTestCase

import demo


class OrderedRecordsTests(DemoTestCase):
    def test_rank_compares_by_its_derived_ord(self) -> None:
        self.demo_case("case:records.ordered.rank.should_compare_by_rust_ord")
        low = demo.Rank(1, 9)
        high = demo.Rank(2, 0)

        self.assertEqual(low.compare_to(high), -1)
        self.assertEqual(high.compare_to(low), 1)
        self.assertEqual(low.compare_to(demo.Rank(1, 9)), 0)
        self.assertTrue(low < high)
        self.assertTrue(low <= demo.Rank(1, 9))
        self.assertTrue(high > low)
        self.assertTrue(high >= demo.Rank(2, 0))
        self.assertEqual(
            sorted([high, demo.Rank(1, 10), low]),
            [low, demo.Rank(1, 10), high],
        )

    def test_version_compares_by_its_hand_written_ord(self) -> None:
        self.demo_case("case:records.ordered.semantic_version.should_compare_by_rust_ord")
        beta = demo.SemanticVersion(1, 2, 3, "beta")
        alpha = demo.SemanticVersion(1, 2, 3, "alpha")
        release = demo.SemanticVersion(1, 2, 3, None)
        next_patch = demo.SemanticVersion(1, 2, 4, "alpha")

        self.assertTrue(beta < release)
        self.assertTrue(alpha < beta)
        self.assertTrue(release < next_patch)
        self.assertEqual(release.compare_to(demo.SemanticVersion(1, 2, 3, None)), 0)
        self.assertEqual(sorted([next_patch, release, beta, alpha]), [alpha, beta, release, next_patch])
        self.assertEqual(max([beta, release, alpha]), release)

    def test_ordering_refuses_other_types(self) -> None:
        with self.assertRaises(TypeError):
            _ = demo.Rank(1, 0) < (1, 0)
        with self.assertRaises(TypeError):
            _ = demo.SemanticVersion(1, 0, 0, None) < demo.Rank(1, 0)
