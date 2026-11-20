import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';
import { capture, captureAsync } from './support.mjs';
import { AsyncFutureManager } from './runtime/module.js';
import { StreamPollManager, StreamSession } from './runtime/stream.js';

globalThis.self = globalThis;
let finish;
const finished = new Promise(resolve => { finish = resolve; });
globalThis.probeDone = finish;
globalThis.largeInteger = () => 9007199254740993n;
class FailureException extends Error {
  constructor() { super('failure'); this.name = 'FailureException'; this.value = { message: 'failure' }; }
}
class Counter {
  constructor(value) { this.n = value; }
  static new(value) { return new Counter(value); }
  static fromValue(value) { return new Counter(value); }
  value() { return this.n; }
  nullable(a, b) { if (a !== null) throw Error('null argument lost'); return b; }
  values() {
    const polls = new StreamPollManager();
    let next = 1;
    return new StreamSession(2, max => {
      const count = Math.min(max, 4 - next);
      return Array.from({length: count}, () => next++);
    }, () => polls.wake(2, 1), polls, () => {}, () => {});
  }
  batches() { return this.values(); }
  notifications(callback) { return this.values().consume(callback); }
}
const futures = new AsyncFutureManager();
globalThis.__boltffi_probe = {
  Counter,
  __boltffiCapture: capture,
  __boltffiCaptureAsync: captureAsync,
  echoScores: value => ({values: new Int32Array(value.values)}),
  echoValues: value => new Int32Array(value),
  echoBools: value => value,
  echoLongs: value => new BigInt64Array(value),
  mutatePoint: value => { value.x = 42; },
  mutateFallible: value => { value.x = 43; throw new FailureException(); },
  fallible: () => { throw new FailureException(); },
  failString: () => { throw new Error('failure'); },
  greeting: value => value,
  interleaved: (before, first, middle, last) => before + first + middle + last,
  defaultUrl: value => value,
  defaultUuid: value => value,
  collision: async value => value,
  echoUrl: value => value,
  echoUuid: value => value,
  echoTime: value => value,
  echoDuration: value => value,
  callAdder: adder => adder.add(3, 4),
  maybeValue: async flag => flag ? 42 : null,
  asyncFallible: async () => { throw new FailureException(); },
  pending: (callback, options) => {
    let cancelled = false;
    let first = true;
    return futures.pollAsync(1, () => {
      if (first) { first = false; callback(); }
      return cancelled ? -1 : 0;
    }, () => 'panic', () => {}, () => { cancelled = true; }, options?.signal, options?.cancelId);
  },
};
if (process.argv[2] === 'wasm') {
  const { compile } = await import('./main.mjs');
  const application = await compile(await readFile(new URL('./main.wasm', import.meta.url)));
  const instance = await application.instantiate({});
  instance.invokeMain();
} else {
  createRequire(import.meta.url)('./main.js');
}
let timer;
try {
  await Promise.race([finished, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('Dart did not complete')), 5000); })]);
} finally {
  clearTimeout(timer);
}
