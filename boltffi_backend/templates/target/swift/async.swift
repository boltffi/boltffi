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
    private let free: (RustFutureHandle?) -> Void
    private var continuationSlot: UInt64 = 0
    // When cancellation finishes the call first, two parties still use the handle: the
    // poll driver, whose poll can be running or queued, and the cancellation handler,
    // which calls cancel. Each releases the handle once, and the last release frees it.
    private var cancelledHandleUsers: UInt64 = 2

    init(handle: RustFutureHandle?, free: @escaping (RustFutureHandle?) -> Void) {
        self.handle = handle
        self.free = free
    }

    func install(_ continuation: Continuation) -> Bool {
        let box = ContinuationBox(continuation)
        let raw = UInt64(UInt(bitPattern: Unmanaged.passRetained(box).toOpaque()))
        let installed = withUnsafeMutablePointer(to: &continuationSlot) { slot in
            boltffi_atomic_u64_cas(slot, 0, raw)
        }
        if installed {
            return true
        }
        Unmanaged<ContinuationBox>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(raw))!).release()
        return false
    }

    func releaseCancelledHandle() {
        while true {
            let users = withUnsafeMutablePointer(to: &cancelledHandleUsers) { count in
                boltffi_atomic_u64_load(count)
            }
            let released = withUnsafeMutablePointer(to: &cancelledHandleUsers) { count in
                boltffi_atomic_u64_cas(count, users, users - 1)
            }
            if released {
                if users == 1 {
                    free(handle)
                }
                return
            }
        }
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
    let poll: (RustFutureHandle?, UInt64, (@convention(c) (UInt64, Int8) -> Void)?) -> Int8
    let ready: () -> Void
    let canPoll: () -> Bool
    let releaseCancelledHandle: () -> Void

    init(
        futureHandle: RustFutureHandle?,
        poll: @escaping (RustFutureHandle?, UInt64, (@convention(c) (UInt64, Int8) -> Void)?) -> Int8,
        ready: @escaping () -> Void,
        canPoll: @escaping () -> Bool,
        releaseCancelledHandle: @escaping () -> Void
    ) {
        self.futureHandle = futureHandle
        self.poll = poll
        self.ready = ready
        self.canPoll = canPoll
        self.releaseCancelledHandle = releaseCancelledHandle
    }

    func start() {
        _ = poll(
            futureHandle,
            UInt64(UInt(bitPattern: Unmanaged.passRetained(self).toOpaque())),
            boltffiAsyncPollCallback
        )
    }

    func handle(_ result: Int8) {
        if result == 0 {
            ready()
        } else if canPoll() {
            _Concurrency.Task { [self] in
                await _Concurrency.Task.yield()
                start()
            }
        } else {
            releaseCancelledHandle()
        }
    }
}

private let boltffiAsyncPollCallback: @convention(c) (UInt64, Int8) -> Void = { data, result in
    Unmanaged<BoltFFIAsyncPollDriver>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(data))!).takeRetainedValue().handle(result)
}

func boltffiAsyncCall<T>(
    futureHandle: RustFutureHandle?,
    poll: @escaping (RustFutureHandle?, UInt64, (@convention(c) (UInt64, Int8) -> Void)?) -> Int8,
    cancel: @escaping (RustFutureHandle?) -> Void,
    free: @escaping (RustFutureHandle?) -> Void,
    complete: @escaping (RustFutureHandle?, UnsafeMutablePointer<FfiStatus>?) throws -> T
) async throws -> T {
    let state = BoltFFIFutureState<T>(handle: futureHandle, free: free)
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            guard state.install(continuation) else {
                // Cancellation finished the call before the first poll, so no poll will use the handle.
                state.releaseCancelledHandle()
                continuation.resume(throwing: CancellationError())
                return
            }
            let driver = BoltFFIAsyncPollDriver(
                futureHandle: futureHandle,
                poll: poll,
                ready: {
                    let finish = state.finish()
                    if case .finished = finish {
                        state.releaseCancelledHandle()
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
                canPoll: { state.canPoll() },
                releaseCancelledHandle: { state.releaseCancelledHandle() }
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
            state.releaseCancelledHandle()
        case .withContinuation(let continuation):
            cancel(state.handle)
            state.releaseCancelledHandle()
            continuation.resume(throwing: CancellationError())
        }
    }
}
