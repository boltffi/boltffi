{%- if section.declaration() -%}
{%- if let Some(owner) = stream.owner() %}
extension {{ owner }} {
{{ stream.documentation() }}    public func {{ stream.name() }}{{ stream.signature() }} {
{{ stream.body("        ") }}
    }
}
{%- else %}
{{ stream.documentation() }}public func {{ stream.name() }}{{ stream.signature() }} {
{{ stream.body("    ") }}
}
{%- endif %}
{%- if let Some(subscription) = stream.batch_subscription() %}

public final class {{ subscription }} {
    private let handle: UInt64
    private let readBatch: (UInt64, UInt) -> [{{ stream.item_type() }}]
    private let wait: (UInt64, UInt32) -> Int32
    private let unsubscribeCall: (UInt64) -> Void
    private let free: (UInt64) -> Void
{%- if stream.fallible() %}
    private let takeFailure: (UInt64) -> Swift.Error?
    private var failure: Swift.Error?
{%- endif %}
    private var closed = false

    @usableFromInline init(
        handle: UInt64,
        readBatch: @escaping (UInt64, UInt) -> [{{ stream.item_type() }}],
        wait: @escaping (UInt64, UInt32) -> Int32,
        unsubscribe: @escaping (UInt64) -> Void,
        free: @escaping (UInt64) -> Void{% if stream.fallible() %},
        takeFailure: @escaping (UInt64) -> Swift.Error?{% endif %}
    ) {
        self.handle = handle
        self.readBatch = readBatch
        self.wait = wait
        self.unsubscribeCall = unsubscribe
        self.free = free
{%- if stream.fallible() %}
        self.takeFailure = takeFailure
{%- endif %}
    }

    deinit {
        if handle != 0 {
            free(handle)
        }
    }

{% if stream.fallible() %}    public func popBatch(maxCount: UInt = 16) throws -> [{{ stream.item_type() }}] {
        if handle == 0 {
            return []
        }
        // closed before this pop, so nothing can arrive after it: an empty pop is final
        let ended = failure != nil || wait(handle, 0) < 0
        let items = readBatch(handle, maxCount)
        if items.isEmpty && ended {
            if failure == nil {
                failure = takeFailure(handle)
            }
            if let failure {
                throw failure
            }
        }
        return items
    }{% else %}    public func popBatch(maxCount: UInt = 16) -> [{{ stream.item_type() }}] {
        if handle == 0 {
            return []
        }
        return readBatch(handle, maxCount)
    }{% endif %}

    public func wait(timeout: UInt32) -> Int32 {
        if handle == 0 {
            return -1
        }
        return wait(handle, timeout)
    }

    public func unsubscribe() {
        if handle == 0 || closed {
            return
        }
        closed = true
        unsubscribeCall(handle)
    }
}
{%- endif %}
{%- if let Some(cancellable) = stream.callback_cancellable() %}

public final class {{ cancellable }} {
    private let cancelAction: () -> Void
    private var cancelled = false

    @usableFromInline init(_ cancelAction: @escaping () -> Void = {}) {
        self.cancelAction = cancelAction
    }

    deinit {
        cancel()
    }

    public func cancel() {
        if cancelled {
            return
        }
        cancelled = true
        cancelAction()
    }
}
{%- endif %}
{%- endif -%}
{%- if section.body() -%}
{%- if stream.async_delivery() -%}
{{ indent }}_Concurrency.Async{% if stream.fallible() %}Throwing{% endif %}Stream<{{ stream.item_type() }}{% if stream.fallible() %}, Swift.Error{% endif %}>(bufferingPolicy: .unbounded) { continuation in
{{ inner_indent }}let {{ stream.subscription_binding() }} = {{ stream.subscribe_call() }}
{{ inner_indent }}guard {{ stream.subscription_binding() }} != 0 else {
{{ block_indent }}continuation.finish()
{{ block_indent }}return
{{ inner_indent }}}
{{ inner_indent }}let context = BoltFFIStreamContext<{{ stream.item_type() }}>(
{{ argument_indent }}subscription: {{ stream.subscription_binding() }},
{{ argument_indent }}batchSize: 16,
{{ argument_indent }}readBatch: { subscription, batchSize in
{{ stream.read_batch(read_batch_indent) }}
{{ argument_indent }}},
{{ argument_indent }}poll: {{ stream.poll() }},
{{ argument_indent }}unsubscribe: {{ stream.unsubscribe() }},
{{ argument_indent }}free: {{ stream.free() }},
{{ argument_indent }}atomicCompareExchange: boltffi_atomic_u8_cas,
{{ argument_indent }}yieldItem: { {{ stream.yielded_item_binding() }} in _ = continuation.yield({{ stream.yielded_item_binding() }}) },
{{ argument_indent }}finish: { continuation.finish() }{% if stream.fallible() %},
{{ argument_indent }}fail: { failure in continuation.finish(throwing: failure) },
{{ argument_indent }}takeFailure: {{ stream.take_failure(argument_indent) }}{% endif %}
{{ inner_indent }})
{{ inner_indent }}continuation.onTermination = { @Sendable _ in context.requestTermination() }
{{ inner_indent }}context.start()
{{ indent }}}
{%- else -%}
{%- if let Some(subscription) = stream.batch_subscription() -%}
{{ indent }}let {{ stream.subscription_binding() }} = {{ stream.subscribe_call() }}
{{ indent }}return {{ subscription }}(
{{ inner_indent }}handle: {{ stream.subscription_binding() }},
{{ inner_indent }}readBatch: { subscription, batchSize in
{{ stream.read_batch(block_indent) }}
{{ inner_indent }}},
{{ inner_indent }}wait: {{ stream.wait() }},
{{ inner_indent }}unsubscribe: {{ stream.unsubscribe() }},
{{ inner_indent }}free: {{ stream.free() }}{% if stream.fallible() %},
{{ inner_indent }}takeFailure: {{ stream.take_failure(inner_indent) }}{% endif %}
{{ indent }})
{%- endif -%}
{%- if let Some(cancellable) = stream.callback_cancellable() -%}
{{ indent }}let {{ stream.subscription_binding() }} = {{ stream.subscribe_call() }}
{{ indent }}if {{ stream.subscription_binding() }} == 0 {
{{ inner_indent }}return {{ cancellable }} {}
{{ indent }}}
{{ indent }}let context = BoltFFIStreamContext<{{ stream.item_type() }}>(
{{ inner_indent }}subscription: {{ stream.subscription_binding() }},
{{ inner_indent }}batchSize: 16,
{{ inner_indent }}readBatch: { subscription, batchSize in
{{ stream.read_batch(block_indent) }}
{{ inner_indent }}},
{{ inner_indent }}poll: {{ stream.poll() }},
{{ inner_indent }}unsubscribe: {{ stream.unsubscribe() }},
{{ inner_indent }}free: {{ stream.free() }},
{{ inner_indent }}atomicCompareExchange: boltffi_atomic_u8_cas,
{{ inner_indent }}yieldItem: callback,
{{ inner_indent }}finish: {}{% if stream.fallible() %},
{{ inner_indent }}fail: onError,
{{ inner_indent }}takeFailure: {{ stream.take_failure(inner_indent) }}{% endif %}
{{ indent }})
{{ indent }}context.start()
{{ indent }}return {{ cancellable }} { context.requestTermination() }
{%- endif -%}
{%- endif -%}
{%- endif -%}
{%- if section.read_batch() -%}
{%- if let Some(batch) = item.direct_batch() -%}
{{ indent }}return boltffiReadDirectStreamBatch(
{{ body_indent }}subscription: subscription,
{{ body_indent }}batchSize: batchSize,
{{ body_indent }}popBatch: {{ pop_batch }}
{{ indent }}) { (rawItems: UnsafeBufferPointer<{{ batch.element() }}>) in
{{ body_indent }}var {{ item.batch_binding() }} = [{{ item.ty() }}]()
{{ body_indent }}{{ item.batch_binding() }}.reserveCapacity(rawItems.count)
{{ body_indent }}for rawItem in rawItems {
{{ item_indent }}{{ item.batch_binding() }}.append({{ batch.expression() }})
{{ body_indent }}}
{{ body_indent }}return {{ item.batch_binding() }}
{{ indent }}}
{%- endif -%}
{%- if let Some(batch) = item.encoded_batch() -%}
{{ indent }}return boltffiReadWireStreamBatch(
{{ body_indent }}subscription: subscription,
{{ body_indent }}batchSize: batchSize,
{{ body_indent }}popBatch: {{ pop_batch }},
{{ body_indent }}freeBuf: {{ free_buffer }}
{{ indent }}) { reader in
{{ body_indent }}let {{ item.batch_count_binding() }} = Int(reader.readU32())
{{ body_indent }}var {{ item.batch_binding() }} = [{{ item.ty() }}]()
{{ body_indent }}{{ item.batch_binding() }}.reserveCapacity({{ item.batch_count_binding() }})
{{ body_indent }}for _ in 0..<{{ item.batch_count_binding() }} {
{{ item_indent }}{{ item.batch_binding() }}.append({{ batch.expression() }})
{{ body_indent }}}
{{ body_indent }}return {{ item.batch_binding() }}
{{ indent }}}
{%- endif -%}
{%- endif -%}
{%- if section.runtime() -%}
private enum BoltFFIStreamPollResult: Int8 {
    case ready = 0
    case closed = 1
}

@inline(__always)
private func boltffiReadDirectStreamBatch<Element, Item>(
    subscription: UInt64,
    batchSize: UInt,
    popBatch: (UInt64, UnsafeMutablePointer<Element>?, UInt) -> UInt,
    mapItems: (UnsafeBufferPointer<Element>) -> [Item]
) -> [Item] {
    if batchSize == 0 {
        return []
    }
    let items = UnsafeMutablePointer<Element>.allocate(capacity: Int(batchSize))
    defer { items.deallocate() }
    let count = popBatch(subscription, items, batchSize)
    if count == 0 {
        return []
    }
    return mapItems(UnsafeBufferPointer(start: items, count: Int(count)))
}

private class BoltFFIStreamPollContext: @unchecked Sendable {
    func handlePoll(_ result: Int8) {
        preconditionFailure("invalid BoltFFI stream poll context")
    }
}

private let boltffiStreamPollCallback: @convention(c) (UInt64, Int8) -> Void = { data, result in
    Unmanaged<BoltFFIStreamPollContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(data))!).takeRetainedValue().handlePoll(result)
}

private final class BoltFFIStreamContext<Item>: BoltFFIStreamPollContext, @unchecked Sendable {
    private let subscription: UInt64
    private let batchSize: UInt
    private let readBatch: (UInt64, UInt) -> [Item]
    private let poll: (UInt64, UInt64, StreamContinuationCallback?) -> Void
    private let unsubscribe: (UInt64) -> Void
    private let free: (UInt64) -> Void
    private let atomicCompareExchange: (UnsafeMutablePointer<UInt8>?, UInt8, UInt8) -> Bool
    private let yieldItem: (Item) -> Void
    private let finish: () -> Void
    private let fail: ((Swift.Error) -> Void)?
    private let takeFailure: ((UInt64) -> Swift.Error?)?
    private var failure: Swift.Error?
    private var lifecycle = UInt8(0)
    private var processing = UInt8(0)

    init(
        subscription: UInt64,
        batchSize: UInt,
        readBatch: @escaping (UInt64, UInt) -> [Item],
        poll: @escaping (UInt64, UInt64, StreamContinuationCallback?) -> Void,
        unsubscribe: @escaping (UInt64) -> Void,
        free: @escaping (UInt64) -> Void,
        atomicCompareExchange: @escaping (UnsafeMutablePointer<UInt8>?, UInt8, UInt8) -> Bool,
        yieldItem: @escaping (Item) -> Void,
        finish: @escaping () -> Void,
        fail: ((Swift.Error) -> Void)? = nil,
        takeFailure: ((UInt64) -> Swift.Error?)? = nil
    ) {
        self.subscription = subscription
        self.batchSize = batchSize
        self.readBatch = readBatch
        self.poll = poll
        self.unsubscribe = unsubscribe
        self.free = free
        self.atomicCompareExchange = atomicCompareExchange
        self.yieldItem = yieldItem
        self.finish = finish
        self.fail = fail
        self.takeFailure = takeFailure
    }

    func start() {
        registerPoll()
    }

    func requestTermination() {
        let started = withUnsafeMutablePointer(to: &lifecycle) {
            atomicCompareExchange($0, 0, 1)
        }
        if started {
            unsubscribe(subscription)
            _ = withUnsafeMutablePointer(to: &lifecycle) {
                atomicCompareExchange($0, 1, 2)
            }
        }
        finalizeIfIdle()
    }

    private func registerPoll() {
        guard withUnsafeMutablePointer(to: &lifecycle, { atomicCompareExchange($0, 0, 0) }) else {
            finalizeIfIdle()
            return
        }
        let data = UInt64(UInt(bitPattern: Unmanaged.passRetained(self).toOpaque()))
        poll(subscription, data, boltffiStreamPollCallback)
    }

    override func handlePoll(_ result: Int8) {
        guard withUnsafeMutablePointer(to: &processing, { atomicCompareExchange($0, 0, 1) }) else {
            finalizeIfIdle()
            return
        }
        let reschedule = processPoll(result)
        _ = withUnsafeMutablePointer(to: &processing) { atomicCompareExchange($0, 1, 0) }
        finalizeIfIdle()
        if reschedule {
            schedulePoll()
        }
    }

    private func processPoll(_ result: Int8) -> Bool {
        guard withUnsafeMutablePointer(to: &lifecycle, { atomicCompareExchange($0, 0, 0) }) else {
            return false
        }
        drain()
        if result == BoltFFIStreamPollResult.closed.rawValue {
            // still processing, so the handle cannot be freed under us
            failure = takeFailure?(subscription)
            requestTermination()
            return false
        }
        return withUnsafeMutablePointer(to: &lifecycle) { atomicCompareExchange($0, 0, 0) }
    }

    private func drain() {
        while true {
            let items = readBatch(subscription, batchSize)
            if items.isEmpty {
                return
            }
            for item in items {
                yieldItem(item)
            }
        }
    }

    private func schedulePoll() {
        _Concurrency.Task { [self] in
            await _Concurrency.Task.yield()
            registerPoll()
        }
    }

    private func finalizeIfIdle() {
        guard withUnsafeMutablePointer(to: &processing, { atomicCompareExchange($0, 0, 0) }) else {
            return
        }
        guard withUnsafeMutablePointer(to: &lifecycle, { atomicCompareExchange($0, 2, 3) }) else {
            return
        }
        free(subscription)
        if let failure, let fail {
            fail(failure)
        } else {
            finish()
        }
    }
}
{%- endif -%}
{%- if section.wire() -%}
@inline(__always)
private func boltffiReadWireStreamBatch<Item>(
    subscription: UInt64,
    batchSize: UInt,
    popBatch: (UInt64, UInt) -> FfiBuf_u8,
    freeBuf: (FfiBuf_u8) -> Void,
    decodeItems: (inout WireReader) -> [Item]
) -> [Item] {
    let buffer = popBatch(subscription, batchSize)
    defer { freeBuf(buffer) }
    guard buffer.len > 0, let pointer = buffer.ptr else {
        return []
    }
    var reader = WireReader(ptr: pointer, len: Int(buffer.len))
    return decodeItems(&reader)
}
{%- endif -%}
