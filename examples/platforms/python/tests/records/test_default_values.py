from tests.support import DemoTestCase

import demo


class DefaultValueRecordTests(DemoTestCase):
    def test_roundtrip_and_describe(self) -> None:
        implicit_defaults = demo.ServiceConfig("worker")

        self.demo_case("case:records.default_values.service_config.should_roundtrip_value")
        self.assertEqual(demo.echo_service_config(implicit_defaults), implicit_defaults)
        self.demo_case("case:records.default_values.service_config.should_describe_values")
        self.assertEqual(implicit_defaults.describe(), "worker:3:standard:none:https://default")

        explicit_config = demo.ServiceConfig("worker", 9, "eu-west", "https://edge", "https://backup")
        self.demo_case("case:records.default_values.service_config.should_roundtrip_value")
        self.assertEqual(demo.echo_service_config(explicit_config), explicit_config)
        self.demo_case("case:records.default_values.service_config.should_describe_values")
        self.assertEqual(explicit_config.describe(), "worker:9:eu-west:https://edge:https://backup")
        self.demo_case("case:records.default_values.service_config.should_describe_with_prefix")
        self.assertEqual(explicit_config.describe_with_prefix("cfg"), "cfg:worker:9:eu-west:https://edge:https://backup")

    def test_default_constructors(self) -> None:
        self.demo_case("case:records.default_values.service_config.from_owned_name.should_return_config")
        self.assertEqual(demo.ServiceConfig.from_owned_name("owned").describe(), "owned:3:standard:none:https://default")
        self.demo_case("case:records.default_values.service_config.from_borrowed_name.should_return_config")
        self.assertEqual(demo.ServiceConfig.from_borrowed_name("borrowed").describe(), "borrowed:3:standard:none:https://default")
        self.demo_case("case:records.default_values.service_config.from_string_ref_name.should_return_config")
        self.assertEqual(demo.ServiceConfig.from_string_ref_name("stringref").describe(), "stringref:3:standard:none:https://default")
        self.demo_case("case:records.default_values.service_config.from_non_empty_name.should_return_config_for_non_empty_values")
        validated_config = demo.ServiceConfig.from_non_empty_name("optional", "eu-west")
        self.assertEqual(validated_config.name, "optional")
        self.assertEqual(validated_config.region, "eu-west")
        self.demo_case("case:records.default_values.service_config.from_non_empty_name.should_return_none_for_empty_values")
        self.assertIsNone(demo.ServiceConfig.from_non_empty_name("", "eu-west"))
        self.assertIsNone(demo.ServiceConfig.from_non_empty_name("optional", ""))

    def test_result_and_optional_constructors(self) -> None:
        self.demo_case("case:records.default_values.service_config.try_with_retries.should_return_config")
        self.assertEqual(demo.ServiceConfig.try_with_retries(8).describe(), "generated:8:standard:none:https://default")
        self.demo_case("case:records.default_values.service_config.try_with_retries.should_reject_negative_retries")
        with self.assertRaises(Exception) as error:
            demo.ServiceConfig.try_with_retries(-1)
        self.assertIn("service config retries must be non-negative", str(error.exception))
        self.demo_case("case:records.default_values.service_config.maybe_with_retries.should_return_some")
        self.assertEqual(demo.ServiceConfig.maybe_with_retries(5).describe(), "generated:5:standard:none:https://default")
        self.demo_case("case:records.default_values.service_config.maybe_with_retries.should_return_none")
        self.assertIsNone(demo.ServiceConfig.maybe_with_retries(-1))
