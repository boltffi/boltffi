private final class BoltFFIThreadBoundPollContinuation: @unchecked Sendable {
    let continuation: CheckedContinuation<Int8, Never>

    init(_ continuation: CheckedContinuation<Int8, Never>) {
        self.continuation = continuation
    }
}

private let boltffiThreadBoundPollCallback: @convention(c) (UInt64, Int8) -> Void = { data, result in
    let box = Unmanaged<BoltFFIThreadBoundPollContinuation>
        .fromOpaque(UnsafeRawPointer(bitPattern: UInt(data))!)
        .takeRetainedValue()
    box.continuation.resume(returning: result)
}

private final class BoltFFIThreadBoundCancellation: @unchecked Sendable {
    private var phase: UInt64 = 0

    func cancel(_ action: () -> Void) {
        let acquired = withUnsafeMutablePointer(to: &phase) { phase in
            boltffi_atomic_u64_cas(phase, 0, 1)
        }
        guard acquired else {
            return
        }
        action()
        withUnsafeMutablePointer(to: &phase) { phase in
            _ = boltffi_atomic_u64_exchange(phase, 2)
        }
    }

    func beginCompletion() async {
        while true {
            let current = withUnsafeMutablePointer(to: &phase) { phase in
                boltffi_atomic_u64_load(phase)
            }
            if current == 1 {
                await _Concurrency.Task.yield()
                continue
            }
            let acquired = withUnsafeMutablePointer(to: &phase) { phase in
                boltffi_atomic_u64_cas(phase, current, 3)
            }
            if acquired {
                return
            }
        }
    }
}

@_unsafeInheritExecutor
private func boltffiThreadBoundAsyncCall<T>(
    futureHandle: RustFutureHandle?,
    poll: @escaping (RustFutureHandle?, UInt64, (@convention(c) (UInt64, Int8) -> Void)?) -> Void,
    cancel: @escaping (RustFutureHandle?) -> Void,
    free: @escaping (RustFutureHandle?) -> Void,
    complete: @escaping (RustFutureHandle?, UnsafeMutablePointer<FfiStatus>?) throws -> T
) async throws -> T {
    let cancellation = BoltFFIThreadBoundCancellation()
    return try await withTaskCancellationHandler {
        while true {
            let result = await withCheckedContinuation { continuation in
                let box = BoltFFIThreadBoundPollContinuation(continuation)
                let data = UInt64(UInt(bitPattern: Unmanaged.passRetained(box).toOpaque()))
                poll(futureHandle, data, boltffiThreadBoundPollCallback)
            }
            if result == 0 {
                break
            }
            await _Concurrency.Task.yield()
        }
        await cancellation.beginCompletion()
        defer {
            free(futureHandle)
        }
        var status = FfiStatus()
        let value = try complete(futureHandle, &status)
        guard status.code == 0 else {
            throw FfiError(message: "FFI failed in async completion with code \(status.code)")
        }
        return value
    } onCancel: {
        cancellation.cancel {
            cancel(futureHandle)
        }
    }
}
