import { assert, assertThrowsWithMessage, demo } from "../support/index.mjs";

export async function run() {
  const implicitDefaults = {
    name: "worker",
    retries: 3,
    region: "standard",
    endpoint: null,
    backupEndpoint: "https://default",
  };
  globalThis.demoCase("case:records.default_values.service_config.should_roundtrip_value");
  assert.deepEqual(demo.echoServiceConfig(implicitDefaults), implicitDefaults);
  globalThis.demoCase("case:records.default_values.service_config.should_describe_values");
  assert.equal(demo.ServiceConfig.describe(implicitDefaults), "worker:3:standard:none:https://default");

  const explicitConfig = {
    name: "worker",
    retries: 9,
    region: "eu-west",
    endpoint: "https://edge",
    backupEndpoint: "https://backup",
  };
  globalThis.demoCase("case:records.default_values.service_config.should_roundtrip_value");
  assert.deepEqual(demo.echoServiceConfig(explicitConfig), explicitConfig);
  globalThis.demoCase("case:records.default_values.service_config.should_describe_values");
  assert.equal(demo.ServiceConfig.describe(explicitConfig), "worker:9:eu-west:https://edge:https://backup");
  globalThis.demoCase("case:records.default_values.service_config.should_describe_with_prefix");
  assert.equal(demo.ServiceConfig.describeWithPrefix(explicitConfig, "cfg"), "cfg:worker:9:eu-west:https://edge:https://backup");

  globalThis.demoCase("case:records.default_values.service_config.from_owned_name.should_return_config");
  assert.equal(demo.ServiceConfig.describe(demo.ServiceConfig.fromOwnedName("owned")), "owned:3:standard:none:https://default");
  globalThis.demoCase("case:records.default_values.service_config.from_borrowed_name.should_return_config");
  assert.equal(demo.ServiceConfig.describe(demo.ServiceConfig.fromBorrowedName("borrowed")), "borrowed:3:standard:none:https://default");
  globalThis.demoCase("case:records.default_values.service_config.from_string_ref_name.should_return_config");
  assert.equal(demo.ServiceConfig.describe(demo.ServiceConfig.fromStringRefName("stringref")), "stringref:3:standard:none:https://default");
  globalThis.demoCase("case:records.default_values.service_config.from_non_empty_name.should_return_config_for_non_empty_values");
  const validatedConfig = demo.ServiceConfig.fromNonEmptyName("optional", "eu-west");
  assert.equal(validatedConfig.name, "optional");
  assert.equal(validatedConfig.region, "eu-west");
  globalThis.demoCase("case:records.default_values.service_config.from_non_empty_name.should_return_none_for_empty_values");
  assert.equal(demo.ServiceConfig.fromNonEmptyName("", "eu-west"), null);
  assert.equal(demo.ServiceConfig.fromNonEmptyName("optional", ""), null);
  globalThis.demoCase("case:records.default_values.service_config.try_with_retries.should_return_config");
  assert.equal(demo.ServiceConfig.tryWithRetries(4).retries, 4);
  globalThis.demoCase("case:records.default_values.service_config.try_with_retries.should_reject_negative_retries");
  assertThrowsWithMessage(
    () => demo.ServiceConfig.tryWithRetries(-1),
    "service config retries must be non-negative",
  );
  globalThis.demoCase("case:records.default_values.service_config.maybe_with_retries.should_return_some");
  assert.equal(demo.ServiceConfig.maybeWithRetries(5).retries, 5);
  globalThis.demoCase("case:records.default_values.service_config.maybe_with_retries.should_return_none");
  assert.equal(demo.ServiceConfig.maybeWithRetries(-1), null);
}
