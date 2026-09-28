export function increment(value) {
  return value + 1;
}

export function invoke(callback, value) {
  return callback(value);
}

export function startup_hook() {
  globalThis.boltffiStartupCount = (globalThis.boltffiStartupCount ?? 0) + 1;
  globalThis.boltffiStartupHook?.();
}
