import { assert, demo } from "../support/index.mjs";

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
}
