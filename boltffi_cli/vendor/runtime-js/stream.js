export class StreamPollManager {
    constructor() {
        this.pending = new Map();
    }
    poll(handle, pollHandle) {
        if (handle === 0) {
            return Promise.resolve(1 /* StreamPollResult.Closed */);
        }
        if (this.pending.has(handle)) {
            return Promise.reject(new Error(`Stream ${handle} already has a pending poll`));
        }
        return new Promise((resolve, reject) => {
            this.pending.set(handle, { resolve, reject });
            try {
                pollHandle(handle);
            }
            catch (error) {
                this.pending.delete(handle);
                reject(error instanceof Error ? error : new Error(String(error)));
            }
        });
    }
    close(handle) {
        const pending = this.pending.get(handle);
        this.pending.delete(handle);
        pending?.resolve(1 /* StreamPollResult.Closed */);
    }
    wake(handle, result) {
        const pending = this.pending.get(handle);
        if (pending === undefined) {
            return;
        }
        this.pending.delete(handle);
        if (result === 0 /* StreamPollResult.Ready */ || result === 1 /* StreamPollResult.Closed */) {
            pending.resolve(result);
            return;
        }
        pending.reject(new Error(`Unknown stream poll result: ${result}`));
    }
}
export class StreamSession {
    constructor(handle, batch, pollHandle, polls, unsubscribeHandle, freeHandle) {
        this.handle = handle;
        this.batch = batch;
        this.pollHandle = pollHandle;
        this.polls = polls;
        this.unsubscribeHandle = unsubscribeHandle;
        this.freeHandle = freeHandle;
        this.unsubscribed = false;
        this.closed = handle === 0;
    }
    get isClosed() { return this.closed; }
    popBatch(maxCount = 16) {
        return this.closed || this.handle === 0 ? [] : this.batch(this.handle, maxCount);
    }
    unsubscribe() {
        if (!this.unsubscribed && this.handle !== 0) {
            this.unsubscribed = true;
            this.unsubscribeHandle(this.handle);
        }
    }
    consume(callback) {
        return new StreamCancellable(this, callback);
    }
    dispose() {
        if (this.closed) {
            return;
        }
        this.closed = true;
        if (this.handle !== 0) {
            this.polls.close(this.handle);
            try {
                this.unsubscribe();
            }
            finally {
                this.freeHandle(this.handle);
            }
        }
    }
    async *[Symbol.asyncIterator]() {
        try {
            while (!this.closed) {
                const items = this.popBatch();
                if (items.length !== 0) {
                    for (const item of items) {
                        if (this.closed)
                            return;
                        yield item;
                    }
                    continue;
                }
                const result = await this.polls.poll(this.handle, this.pollHandle);
                if (this.closed) {
                    return;
                }
                if (result === 1 /* StreamPollResult.Closed */) {
                    let remaining = this.popBatch();
                    while (remaining.length !== 0) {
                        for (const item of remaining) {
                            if (this.closed)
                                return;
                            yield item;
                        }
                        remaining = this.popBatch();
                    }
                    return;
                }
            }
        }
        finally {
            this.dispose();
        }
    }
}
export class StreamCancellable {
    pause() {
        if (this.paused !== undefined)
            return;
        this.paused = new Promise((resolve) => { this.resumePaused = resolve; });
    }
    resume() {
        const resolve = this.resumePaused;
        this.paused = undefined;
        this.resumePaused = undefined;
        resolve?.();
    }
    constructor(session, callback) {
        this.session = session;
        this.done = this.consume(callback);
    }
    cancel() {
        this.resume();
        this.session.dispose();
    }
    async consume(callback) {
        const iterator = this.session[Symbol.asyncIterator]();
        try {
            let next = await iterator.next();
            while (!next.done) {
                if (this.paused !== undefined)
                    await this.paused;
                if (this.session.isClosed)
                    break;
                callback(next.value);
                if (this.paused !== undefined)
                    await this.paused;
                next = await iterator.next();
            }
        }
        finally {
            await iterator.return?.();
        }
    }
}
//# sourceMappingURL=stream.js.map