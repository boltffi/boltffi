export type StreamBatch<T> = (handle: number, maxCount: number) => T[];
export type StreamLifecycle = (handle: number) => void;
export type StreamPoll = (handle: number) => void;

export const enum StreamPollResult {
  Ready = 0,
  Closed = 1,
}

interface PendingStreamPoll {
  resolve: (result: StreamPollResult) => void;
  reject: (error: Error) => void;
}

export class StreamPollManager {
  private pending = new Map<number, PendingStreamPoll>();

  poll(handle: number, pollHandle: StreamPoll): Promise<StreamPollResult> {
    if (handle === 0) {
      return Promise.resolve(StreamPollResult.Closed);
    }
    if (this.pending.has(handle)) {
      return Promise.reject(new Error(`Stream ${handle} already has a pending poll`));
    }
    return new Promise((resolve, reject) => {
      this.pending.set(handle, { resolve, reject });
      try {
        pollHandle(handle);
      } catch (error) {
        this.pending.delete(handle);
        reject(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }

  wake(handle: number, result: number): void {
    const pending = this.pending.get(handle);
    if (pending === undefined) {
      return;
    }
    this.pending.delete(handle);
    if (result === StreamPollResult.Ready || result === StreamPollResult.Closed) {
      pending.resolve(result);
      return;
    }
    pending.reject(new Error(`Unknown stream poll result: ${result}`));
  }
}

export class StreamSession<T> implements AsyncIterable<T> {
  private closed: boolean;
  private unsubscribed = false;

  constructor(
    private readonly handle: number,
    private readonly batch: StreamBatch<T>,
    private readonly pollHandle: StreamPoll,
    private readonly polls: StreamPollManager,
    private readonly unsubscribeHandle: StreamLifecycle,
    private readonly freeHandle: StreamLifecycle
  ) {
    this.closed = handle === 0;
  }

  popBatch(maxCount = 16): T[] {
    return this.closed || this.handle === 0 ? [] : this.batch(this.handle, maxCount);
  }

  unsubscribe(): void {
    if (!this.unsubscribed && this.handle !== 0) {
      this.unsubscribed = true;
      this.unsubscribeHandle(this.handle);
    }
  }

  consume(callback: (item: T) => void): StreamCancellable<T> {
    return new StreamCancellable(this, callback);
  }

  dispose(): void {
    if (this.closed) {
      return;
    }
    this.closed = true;
    if (this.handle !== 0) {
      this.unsubscribe();
      this.freeHandle(this.handle);
    }
  }

  async *[Symbol.asyncIterator](): AsyncIterator<T> {
    try {
      while (!this.closed) {
        const items = this.popBatch();
        if (items.length !== 0) {
          yield* items;
          continue;
        }
        const result = await this.polls.poll(this.handle, this.pollHandle);
        if (this.closed) {
          return;
        }
        if (result === StreamPollResult.Closed) {
          let remaining = this.popBatch();
          while (remaining.length !== 0) {
            yield* remaining;
            remaining = this.popBatch();
          }
          return;
        }
      }
    } finally {
      this.dispose();
    }
  }
}

export class StreamCancellable<T> {
  readonly done: Promise<void>;

  constructor(
    private readonly session: StreamSession<T>,
    callback: (item: T) => void
  ) {
    this.done = this.consume(callback);
  }

  cancel(): void {
    this.session.dispose();
  }

  private async consume(callback: (item: T) => void): Promise<void> {
    const iterator = this.session[Symbol.asyncIterator]();
    try {
      let next = await iterator.next();
      while (!next.done) {
        callback(next.value);
        next = await iterator.next();
      }
    } finally {
      await iterator.return?.();
    }
  }
}
