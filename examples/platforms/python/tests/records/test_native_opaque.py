from tests.support import DemoTestCase

import demo


class NativeOpaqueRecordTests(DemoTestCase):
    def test_engine_snapshot_reads_fields_through_accessors(self) -> None:
        self.demo_case(
            "case:records.native_opaque.engine_snapshot.should_read_fields_through_accessors"
        )
        snapshot = demo.capture_engine_snapshot(4)
        self.assertEqual(snapshot.revision, 4)
        self.assertEqual(snapshot.label, "engine-4")
        self.assertEqual(snapshot.build_tag, "build-4")

        # The handle owns the Rust value; the host never sees a serialized
        # layout, so there is no dataclass-style constructor to call.
        with self.assertRaises(TypeError):
            demo.EngineSnapshot()

        # Releasing is idempotent, and reads after release must fail loudly
        # rather than touch a freed value.
        snapshot.close()
        snapshot.close()
        with self.assertRaises(RuntimeError):
            _ = snapshot.revision

    def test_engine_snapshot_reports_absent_optional_field(self) -> None:
        self.demo_case(
            "case:records.native_opaque.engine_snapshot.should_report_absent_optional_field"
        )
        snapshot = demo.capture_untagged_snapshot(7)
        self.assertIsNone(snapshot.build_tag)
        self.assertEqual(snapshot.revision, 7)
        self.assertEqual(snapshot.label, "engine-7")
        snapshot.close()
