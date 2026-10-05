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

  close(handle: number): void {
    const pending = this.pending.get(handle);
    this.pending.delete(handle);
    pending?.resolve(StreamPollResult.Closed);
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

  get isClosed(): boolean { return this.closed; }

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
      this.polls.close(this.handle);
      try {
        this.unsubscribe();
      } finally {
        this.freeHandle(this.handle);
      }
    }
  }

  async *[Symbol.asyncIterator](): AsyncIterator<T> {
    try {
      while (!this.closed) {
        const items = this.popBatch();
        if (items.length !== 0) {
          for (const item of items) {
            if (this.closed) return;
            yield item;
          }
          continue;
        }
        const result = await this.polls.poll(this.handle, this.pollHandle);
        if (this.closed) {
          return;
        }
        if (result === StreamPollResult.Closed) {
          let remaining = this.popBatch();
          while (remaining.length !== 0) {
            for (const item of remaining) {
              if (this.closed) return;
              yield item;
            }
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
  private paused?: Promise<void>;
  private resumePaused?: () => void;

  pause(): void {
    if (this.paused !== undefined) return;
    this.paused = new Promise((resolve) => { this.resumePaused = resolve; });
  }

  resume(): void {
    const resolve = this.resumePaused;
    this.paused = undefined;
    this.resumePaused = undefined;
    resolve?.();
  }

  constructor(
    private readonly session: StreamSession<T>,
    callback: (item: T) => void
  ) {
    this.done = this.consume(callback);
  }

  cancel(): void {
    this.resume();
    this.session.dispose();
  }

  private async consume(callback: (item: T) => void): Promise<void> {
    const iterator = this.session[Symbol.asyncIterator]();
    try {
      let next = await iterator.next();
      while (!next.done) {
        if (this.paused !== undefined) await this.paused;
        if (this.session.isClosed) break;
        callback(next.value);
        if (this.paused !== undefined) await this.paused;
        next = await iterator.next();
      }
    } finally {
      await iterator.return?.();
    }
  }
}
