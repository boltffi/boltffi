import { assert, demo } from "../support/index.mjs";

export async function run() {
  const red = demo.TrafficLight.new();
  globalThis.demoCase("case:classes.enum_backed.traffic_light.seconds_left.should_match_on_the_variant_behind_the_handle");
  assert.equal(red.secondsLeft(), 0);
  const green = red.turnGreen(30);
  assert.equal(green.secondsLeft(), 30);
  green.dispose();
  red.dispose();
}
