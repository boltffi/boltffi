private final class BoltFFIFutureState<T>: @unchecked Sendable {
    typealias Continuation = CheckedContinuation<T, Error>

    enum Finish {
        case finished
        case withoutContinuation
        case withContinuation(Continuation)
    }

    final class ContinuationBox {
        let continuation: Continuation

        init(_ continuation: Continuation) {
            self.continuation = continuation
        }
    }

    let handle: RustFutureHandle?
    private var continuationSlot: UInt64 = 0

    init(handle: RustFutureHandle?) {
        self.handle = handle
    }

    func install(_ continuation: Continuation) -> Bool {
        let box = ContinuationBox(continuation)
        let raw = UInt64(UInt(bitPattern: Unmanaged.passRetained(box).toOpaque()))
        let prior = withUnsafeMutablePointer(to: &continuationSlot) { slot in
            boltffi_atomic_u64_exchange(slot, raw)
        }
        if prior == 0 {
            return true
        }
        Unmanaged<ContinuationBox>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(raw))!).release()
        if prior != 1 {
            withUnsafeMutablePointer(to: &continuationSlot) { slot in
                _ = boltffi_atomic_u64_exchange(slot, prior)
            }
        }
        return false
    }

    func canPoll() -> Bool {
        withUnsafeMutablePointer(to: &continuationSlot) { slot in
            boltffi_atomic_u64_load(slot)
        } != 1
    }

    func finish() -> Finish {
        let prior = withUnsafeMutablePointer(to: &continuationSlot) { slot in
            boltffi_atomic_u64_exchange(slot, 1)
        }
        if prior == 1 {
            return .finished
        }
        if prior == 0 {
            return .withoutContinuation
        }
        let box = Unmanaged<ContinuationBox>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(prior))!).takeRetainedValue()
        return .withContinuation(box.continuation)
    }
}

private final class BoltFFIAsyncPollDriver: @unchecked Sendable {
    let futureHandle: RustFutureHandle?
    let poll: (RustFutureHandle?, UInt64, (@convention(c) (UInt64, Int8) -> Void)?) -> Void
    let ready: () -> Void
    let canPoll: () -> Bool

    init(
        futureHandle: RustFutureHandle?,
        poll: @escaping (RustFutureHandle?, UInt64, (@convention(c) (UInt64, Int8) -> Void)?) -> Void,
        ready: @escaping () -> Void,
        canPoll: @escaping () -> Bool
    ) {
        self.futureHandle = futureHandle
        self.poll = poll
        self.ready = ready
        self.canPoll = canPoll
    }

    func start() {
        poll(futureHandle, UInt64(UInt(bitPattern: Unmanaged.passRetained(self).toOpaque())), boltffiAsyncPollCallback)
    }

    func handle(_ result: Int8) {
        if result == 0 {
            ready()
        } else if canPoll() {
            _Concurrency.Task { [self] in
                await _Concurrency.Task.yield()
                start()
            }
        }
    }
}

private let boltffiAsyncPollCallback: @convention(c) (UInt64, Int8) -> Void = { data, result in
    Unmanaged<BoltFFIAsyncPollDriver>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(data))!).takeRetainedValue().handle(result)
}

func boltffiAsyncCall<T>(
    futureHandle: RustFutureHandle?,
    poll: @escaping (RustFutureHandle?, UInt64, (@convention(c) (UInt64, Int8) -> Void)?) -> Void,
    cancel: @escaping (RustFutureHandle?) -> Void,
    free: @escaping (RustFutureHandle?) -> Void,
    complete: @escaping (RustFutureHandle?, UnsafeMutablePointer<FfiStatus>?) throws -> T
) async throws -> T {
    let state = BoltFFIFutureState<T>(handle: futureHandle)
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            guard state.install(continuation) else {
                continuation.resume(throwing: CancellationError())
                return
            }
            let driver = BoltFFIAsyncPollDriver(
                futureHandle: futureHandle,
                poll: poll,
                ready: {
                    let finish = state.finish()
                    if case .finished = finish {
                        return
                    }
                    var status = FfiStatus()
                    do {
                        let value = try complete(futureHandle, &status)
                        free(futureHandle)
                        switch finish {
                        case .withContinuation(let continuation):
                            if status.code == 0 {
                                continuation.resume(returning: value)
                            } else {
                                continuation.resume(throwing: FfiError(message: "FFI failed in async completion with code \(status.code)"))
                            }
                        case .withoutContinuation, .finished:
                            break
                        }
                    } catch {
                        free(futureHandle)
                        if case .withContinuation(let continuation) = finish {
                            continuation.resume(throwing: error)
                        }
                    }
                },
                canPoll: { state.canPoll() }
            )
            driver.start()
        }
    } onCancel: {
        let finish = state.finish()
        switch finish {
        case .finished:
            break
        case .withoutContinuation:
            cancel(state.handle)
            free(state.handle)
        case .withContinuation(let continuation):
            cancel(state.handle)
            free(state.handle)
            continuation.resume(throwing: CancellationError())
        }
    }
}
