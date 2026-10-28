import { expect, it, vi } from 'vitest';
import { StreamPollManager, StreamSession } from '../src/stream.js';

it('settles a pending poll when the session is disposed', async () => {
  const polls = new StreamPollManager();
  const unsubscribe = vi.fn();
  const free = vi.fn();
  const session = new StreamSession(1, () => [], () => {}, polls, unsubscribe, free);
  const iterator = session[Symbol.asyncIterator]();
  const pending = iterator.next();
  session.dispose();
  expect(await pending).toEqual({ value: undefined, done: true });
  session.dispose();
  expect(unsubscribe).toHaveBeenCalledTimes(1);
  expect(free).toHaveBeenCalledTimes(1);
});

it('stops delivery and draining while paused, then resumes in order', async () => {
  let batchCount = 0;
  const session = new StreamSession(1, () => { batchCount++; return [1, 2, 3]; }, () => {}, new StreamPollManager(), () => {}, () => {});
  const values: number[] = [];
  const consumer = session.consume(value => {
    values.push(value);
    if (values.length === 1) consumer.pause();
    if (values.length === 3) consumer.cancel();
  });
  await new Promise(resolve => setTimeout(resolve, 0));
  expect(values).toEqual([1]);
  expect(batchCount).toBe(1);
  consumer.resume();
  await consumer.done;
  expect(values).toEqual([1, 2, 3]);
  expect(batchCount).toBe(1);
});

it('cancels a paused consumer without waiting for resume', async () => {
  const session = new StreamSession(1, () => [1], () => {}, new StreamPollManager(), () => {}, () => {});
  const consumer = session.consume(() => consumer.pause());
  await new Promise(resolve => setTimeout(resolve, 0));
  consumer.cancel();
  await consumer.done;
});

it('frees the handle even when unsubscribe throws', () => {
  const free = vi.fn();
  const session = new StreamSession(1, () => [], () => {}, new StreamPollManager(), () => { throw Error('unsubscribe'); }, free);
  expect(() => session.dispose()).toThrow('unsubscribe');
  expect(free).toHaveBeenCalledOnce();
});

it('does not yield buffered items after disposal', async () => {
  const session = new StreamSession(1, () => [1, 2, 3], () => {}, new StreamPollManager(), () => {}, () => {});
  const iterator = session[Symbol.asyncIterator]();
  expect(await iterator.next()).toEqual({ value: 1, done: false });
  session.dispose();
  expect(await iterator.next()).toEqual({ value: undefined, done: true });
});
