# ADR: Stream Delivery Contract. One Buffer, Declared in Rust

## Status
Proposed

## Authors
- [Henrik](https://github.com/akesson)

## Date
2026-11-27

---

## 1. Background and Problem

### 1.1 One declaration, a different stream per target

```rust
#[ffi_stream(item = Chunk)]
pub fn chunks(&self) -> Arc<EventSubscription<Chunk>> {
    self.producer.subscribe()          // StreamProducer::new(256)
}
```

The Rust code sizes the ring at 256 items. What a consumer gets from `chunks()`
in async mode depends on the target:

| target | host type | a slow consumer | subscribes |
|---|---|---|---|
| Swift | `AsyncStream`, `.unbounded` | host buffer grows without limit | at the call |
| Dart | `StreamController`, no `onPause` | host buffer grows without limit | at the call |
| Kotlin | `callbackFlow`, 64-slot channel | channel fills, then the ring, new items dropped | some time after `collect` |
| C# | `IAsyncEnumerable` pull loop | ring fills, new items dropped | in the first `MoveNextAsync` |
| TypeScript | `AsyncIterable` pull loop | ring fills, new items dropped | at the call |

Swift and Dart drain the ring into a second, unbounded buffer, so the declared
256 never applies. Before #703 Kotlin did the opposite: a hidden 64-slot channel
dropped items while the ring still had room, and a 200-item burst delivered 65.
Since #703 `send` suspends instead, but the channel's 64 slots still add to the
ring's 256.

The ring itself rounds the capacity up to a power of two, so
`StreamProducer::new(100)` buffers 128.

No document says which of these a stream promises. A consumer cannot write
correct code against it, and a new backend has nothing to implement against, so
it adds one more row.

### 1.2 Places where items or subscriptions are lost today

Found by reading main at `2f93f706`. The Kotlin cases were also reproduced in
July on 0.27.5. The last one has not been reproduced yet.

- **Before registration.** `callbackFlow` runs its block in a coroutine of its
  own, so a Kotlin consumer cannot tell when the subscription exists. An item
  pushed after `collect` is called can still be lost. The Kotlin demo tests
  work around it with `delay(100)`.
- **Ring full.** `push_event` returns `false` and `StreamProducer::push` drops
  that result. Neither the producer nor the consumer can see the loss.
- **Subscriber slots full.** The 33rd `subscribe()` on a `StreamProducer`
  returns a subscription that never receives an item. The only signal is an
  `eprintln!`.
- **Producer dropped.** `StreamProducer` holds weak references and has no
  `Drop`. Its subscribers are never told the stream ended and wait forever.
- **Closed with items buffered, in C#.** The pull loop ends as soon as `wait`
  reports the close and does not read the ring again. Items pushed just before
  the close are freed undelivered. The other targets drain first.
- **Owner freed before registration, in C#.** A method stream passes only
  `self.Handle` to the enumerable and subscribes in the first `MoveNextAsync`.
  If the owner is finalized in between, subscribe reads a freed handle.
- **Consumer abandoned.** A collection that is never cancelled keeps its
  subscription registered, and every later push clones into its ring. A Kotlin
  `Flow` whose scope is leaked never reaches `awaitClose`, and while it waits
  for items its continuation sits in a global map, so the garbage collector
  cannot release it either.
- **Two threads pushing.** `StreamProducer` is `Sync` and `push` takes `&self`,
  but the ring is single-producer and takes no lock. Two concurrent pushes can
  write the same slot.

## 2. Decision

The subscription ring is the only buffer. Its size and overflow policy are
declared in Rust, where the ring is built. Every target delivers from that ring
at the consumer's pace and follows one lifecycle.

### 2.1 The ring is the only buffer

A host must not put a lossy or unbounded buffer of its own between the ring and
the consumer. Delivery is a pull loop or a suspending push.

C#, TypeScript and Java's callback mode already deliver this way. Kotlin
replaces `callbackFlow` with a `flow { }` pull loop, which has no channel:

```kotlin
fun EventBus.events(): Flow<Event> = flow {
    val subscription = Native.subscribe(boltffiHandle())
    try {
        var closed = false
        while (true) {
            val items = popBatch(subscription, 16)
            items.forEach { emit(it) }
            if (items.isNotEmpty()) continue
            if (closed) break
            closed = awaitPoll(subscription) == CLOSED
        }
    } finally {
        unsubscribe(subscription)
        free(subscription)
    }
}
```

On close the loop drains the ring before it ends. Swift replaces the unbounded
`AsyncStream` with a generated `AsyncSequence` whose iterator pops a batch and
awaits the existing poll when the ring is empty. Dart stops draining while its
listener is paused.

### 2.2 Capacity and overflow are declared in Rust

The function body builds the subscription, so the attribute cannot size it. The
policy goes where the capacity already is, on `EventSubscription`.
`StreamProducer` takes the same calls and applies them to each subscription it
creates.

```rust
EventSubscription::new(256)                                 // DropNewest, as today
EventSubscription::new(4096).overflow(Overflow::DropOldest)
EventSubscription::unbounded()
```

| policy | a push into a full ring | consumer observes |
|---|---|---|
| `DropNewest`, the default | the new item is discarded | a gap, counted |
| `DropOldest` | the oldest unread item is discarded | a gap, counted |

`unbounded()` is a capacity, not a policy, and takes no `overflow`. Kotlin's
`Channel.UNLIMITED` and Swift's `.unbounded` are spelled the same way. It is
what a stream that relied on Swift's unbounded buffer switches to.

A stream that must not have gaps fails instead. Only a fallible stream (#933)
can do that, and the Rust code supplies the error:

```rust
EventSubscription::<Chunk, BodyError>::fallible(256)
    .fail_on_overflow(|| BodyError::ConsumerTooSlow)
```

The consumer receives every item pushed before the overflow and then
`BodyError::ConsumerTooSlow`, through the `fail` that #933 adds. A stream
without `error = E` has no `fail_on_overflow`, so it cannot be configured to
fail in a way the host would show as a normal end.

The ring holds exactly the declared number of items. It may still allocate a
power of two, but a push into a ring holding `capacity` items overflows.

`push` returns how many subscribers took the item and how many dropped it. Each
`EventSubscription` counts `delivered` and `dropped` on the Rust side. Async
host types (`Flow`, `IAsyncEnumerable`, `Stream`, `AsyncIterable`) have no
place to show them, so a crate that wants consumers to see loss exports the
counters itself or uses `fail_on_overflow`. `push` never waits for a consumer,
and it is safe to call from several threads.

### 2.3 Lifecycle

These rules go into `streaming.mdx` as the contract.

1. **Registration.** A subscription has one consumer. On a hot host type
   (Swift, Dart, TypeScript, Java, and batch mode everywhere) it is registered
   when the generated stream function is called. On a cold host type (Kotlin
   `Flow`, C# `IAsyncEnumerable`) each collection is its own subscription, and
   it is registered inside the call that starts the collection (`collect`, the
   first `MoveNextAsync`), before that call first suspends. A cold stream
   keeps its owning object alive until it has registered. An item pushed
   after registration is delivered or counted as dropped.
2. **Order.** Items arrive in push order, per subscription.
3. **Completeness.** An unbounded stream and a stream with `fail_on_overflow`
   have no gaps. `DropNewest` and `DropOldest` streams may have gaps, and
   `dropped` counts them.
4. **Termination.** A subscription ends exactly once: *ended* (the producer
   closed it or was dropped), *failed*, or *cancelled* by the consumer. Buffered
   items are delivered before *ended* and *failed*.
5. **Slot limit.** `subscribe()` on a producer with no free slot returns a
   subscription that has already ended. `try_subscribe()` returns `Err`, so the
   crate can report it another way.
6. **Abandonment.** A stream object that becomes unreachable, never iterated
   or dropped between pulls, still releases its subscription: by `deinit` in
   Swift, a `Cleaner` on the JVM, a finalizer in C#, a `NativeFinalizer` in
   Dart, a `FinalizationRegistry` in TypeScript. A collection suspended while
   it waits for items is reachable through its pending poll. It is live, and
   only cancellation ends it.

Rule 1 keeps a Kotlin `Flow` cold, so `repeatOnLifecycle`, `retry` and
`shareIn` keep working. A consumer that needs "collect, then emit" without a
race starts the collector with `CoroutineStart.UNDISPATCHED`.

Returning the same `Arc<EventSubscription>` from two calls gives two consumers
one subscription, and the first to cancel ends it for both. The `Sensor`
example in `streaming.mdx` does this today and changes to a `StreamProducer`.

### 2.4 Conformance

Each rule becomes a `demo_case` on the demo `EventBus`. The `demo-tests audit`
job already makes every target either pass each case or record an
`ImplementationGap`. A target calls streams stable when these cases pass.

## 3. Rollout

1. Loss that is already a bug: the counters and the `push` result, exact ring
   capacity, ending subscribers when the producer drops, `try_subscribe` and
   the ended subscription on a full producer, draining on close and keeping
   the owner alive until registration in C#, and concurrent `push`. The
   contract text lands here with a table of which targets meet which rule, and
   each later step updates the table.
2. Kotlin moves from `callbackFlow` to the `flow { }` pull loop, so it
   registers inside `collect` and holds no channel. The demo tests drop
   `delay(100)`.
3. Swift and Dart deliver at the consumer's pace, and `unbounded()` lands in the
   same PR. This breaks Swift source: the return type is no longer
   `AsyncStream<T>`, and a stream that relied on the unbounded host buffer must
   declare `unbounded()`.
4. `DropOldest`, and `fail_on_overflow` after #933.
5. Release of abandoned subscriptions, per target.

Each step is one PR with the conformance cases for its rules.

## 4. Alternatives considered

### 4.1 Declare the policy on `#[ffi_stream]` or in `boltffi.toml`

```rust
#[ffi_stream(item = Chunk, capacity = 4096, overflow = "drop_oldest")]
pub fn chunks(&self) -> Arc<EventSubscription<Chunk>> { self.producer.subscribe() }
```

The body has already built the ring with its own capacity, so the two
declarations can disagree. Hosts do not need the values either, because under
2.1 they hold no buffer.

### 4.2 Register at the call on every target

One rule for every target is simpler to state. But a Kotlin `Flow` and a C#
`IAsyncEnumerable` would then be collectable once, and this fails on the first
background and foreground cycle:

```kotlin
val events = bus.events()
repeatOnLifecycle(STARTED) { events.collect { } }
```

A flow that is created and never collected would also hold a subscriber slot
until it is garbage collected.

### 4.3 A built-in overflow error on every stream

`Overflow::Fail` could carry an error type that BoltFFI defines. Every stream
would then need a failing host type, such as a throwing sequence in Swift, even
if the crate never chooses to fail. With `fail_on_overflow` the consumer sees
one error type, the one the crate declared.

### 4.4 Let each host choose its buffering

This is the current approach. The declared capacity means something different
on each target, and a library built on streams, such as an HTTP body stream,
cannot promise more than the weakest target does.

### 4.5 Block the producer when the ring is full

`push` is called from synchronous Rust, often from a callback that must not
block. Node's `write()`, Kotlin's `tryEmit` and Swift's `yield` all return a
result and do not suspend the caller. A hook that wakes the producer when space
is free can follow if a use case needs it.

## 5. Consequences

- A stream delivers the same items on every target, and the Rust code that
  builds it says which items can be lost.
- Loss is counted where it happens. The producer sees it in the `push` result
  and the consumer in `dropped`.
- A new backend implements six rules and passes the conformance cases.
- Swift and Dart consumers that fall behind now lose items under the default
  policy, where before they used more memory. The crate declares
  `unbounded()` to keep the old behavior.
- The Swift return type of an async stream changes, which breaks code that
  names `AsyncStream<T>`.
- `StreamProducer::push` returns counts instead of `()`, which breaks Rust code
  that uses it as the tail expression of a function returning `()`.
- The ring needs new work. Today it is fixed-size, single-producer, and only the
  consumer moves the read index. `DropOldest` needs the producer to evict,
  `unbounded()` needs a queue that grows, and concurrent `push` needs more than
  one producer. None of them may make `push` wait for a consumer.
- `fail_on_overflow` depends on #933.
