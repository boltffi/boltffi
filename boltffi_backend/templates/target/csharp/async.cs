    internal static class BoltFFIAsync
    {
        private const sbyte PollReady = 0;
        private const int StatusOk = 0;
        private const int StatusCancelled = 4;

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        internal delegate void RustFutureContinuationCallback(ulong callbackData, sbyte pollResult);

        private static readonly RustFutureContinuationCallback Continuation = OnFutureContinuation;
        private static readonly global::System.Collections.Concurrent.ConcurrentDictionary<ulong, global::System.Threading.Tasks.TaskCompletionSource<sbyte>> Continuations = new();
        private static long nextContinuationId;

        internal static async global::System.Threading.Tasks.Task<T> CallAsync<T>(
            global::System.Func<nint> start,
            global::System.Action<nint, ulong, RustFutureContinuationCallback> poll,
            global::System.Func<nint, T> complete,
            global::System.Action<nint> cancel,
            global::System.Action<nint> free,
            global::System.Threading.CancellationToken cancellationToken)
        {
            var future = new RustFutureOwner(start(), cancel, free);
            global::System.Threading.CancellationTokenRegistration cancellationRegistration = cancellationToken.Register(
                static state => ((RustFutureOwner)state!).Cancel(), future);
            try
            {
                cancellationToken.ThrowIfCancellationRequested();
                while (true)
                {
                    sbyte pollResult = await PollOnceAsync(future, poll, cancellationToken).ConfigureAwait(false);
                    cancellationToken.ThrowIfCancellationRequested();
                    if (pollResult == PollReady) break;
                    await RepollOnThreadPoolAsync().ConfigureAwait(false);
                }
                cancellationRegistration.Dispose();
                return complete(future.Handle);
            }
            catch (global::System.OperationCanceledException)
            {
                future.Cancel();
                throw;
            }
            finally
            {
                cancellationRegistration.Dispose();
                future.Release();
            }
        }

        internal static global::System.Threading.Tasks.Task CallAsyncVoid(
            global::System.Func<nint> start,
            global::System.Action<nint, ulong, RustFutureContinuationCallback> poll,
            global::System.Action<nint> complete,
            global::System.Action<nint> cancel,
            global::System.Action<nint> free,
            global::System.Threading.CancellationToken cancellationToken) =>
            CallAsync<VoidResult>(
                start,
                poll,
                future => { complete(future); return default; },
                cancel,
                free,
                cancellationToken);

        internal static void ThrowIfStatus(FfiStatus status, global::System.Threading.CancellationToken cancellationToken)
        {
            switch (status.code)
            {
                case StatusOk: return;
                case StatusCancelled: throw new global::System.OperationCanceledException(cancellationToken);
                default: throw new global::System.InvalidOperationException($"FFI async completion failed with status code {status.code}");
            }
        }

        private static global::System.Threading.Tasks.Task<sbyte> PollOnceAsync(
            RustFutureOwner future,
            global::System.Action<nint, ulong, RustFutureContinuationCallback> poll,
            global::System.Threading.CancellationToken cancellationToken)
        {
            var completion = new global::System.Threading.Tasks.TaskCompletionSource<sbyte>(
                global::System.Threading.Tasks.TaskCreationOptions.RunContinuationsAsynchronously);
            ulong callbackData = RegisterContinuation(completion);
            try
            {
                poll(future.Handle, callbackData, Continuation);
            }
            catch
            {
                Continuations.TryRemove(callbackData, out _);
                throw;
            }
            var state = new PollState(callbackData, cancellationToken);
            global::System.Threading.CancellationTokenRegistration registration = cancellationToken.Register(static value =>
            {
                var current = (PollState)value!;
                if (Continuations.TryRemove(current.CallbackData, out var pending))
                    pending.TrySetCanceled(current.CancellationToken);
            }, state);
            return AwaitPollAsync(completion.Task, registration);
        }

        private static async global::System.Threading.Tasks.Task<sbyte> AwaitPollAsync(
            global::System.Threading.Tasks.Task<sbyte> pollTask,
            global::System.Threading.CancellationTokenRegistration registration)
        {
            try { return await pollTask.ConfigureAwait(false); }
            finally { registration.Dispose(); }
        }

        private static global::System.Threading.Tasks.Task RepollOnThreadPoolAsync()
        {
            var completion = new global::System.Threading.Tasks.TaskCompletionSource<object?>(
                global::System.Threading.Tasks.TaskCreationOptions.RunContinuationsAsynchronously);
            global::System.Threading.ThreadPool.QueueUserWorkItem(
                static state => ((global::System.Threading.Tasks.TaskCompletionSource<object?>)state!).TrySetResult(null),
                completion);
            return completion.Task;
        }

        private static ulong RegisterContinuation(global::System.Threading.Tasks.TaskCompletionSource<sbyte> completion)
        {
            while (true)
            {
                ulong callbackData = unchecked((ulong)global::System.Threading.Interlocked.Increment(ref nextContinuationId));
                if (callbackData != 0 && Continuations.TryAdd(callbackData, completion)) return callbackData;
            }
        }

        private static void OnFutureContinuation(ulong callbackData, sbyte pollResult)
        {
            if (Continuations.TryRemove(callbackData, out var completion))
                completion.TrySetResult(pollResult);
        }

        private sealed class PollState
        {
            internal PollState(ulong callbackData, global::System.Threading.CancellationToken cancellationToken)
            {
                CallbackData = callbackData;
                CancellationToken = cancellationToken;
            }

            internal ulong CallbackData { get; }
            internal global::System.Threading.CancellationToken CancellationToken { get; }
        }

        private sealed class RustFutureOwner
        {
            private readonly global::System.Action<nint> cancel;
            private readonly global::System.Action<nint> free;
            private readonly object gate = new object();
            private nint handle;
            private bool cancelled;
            private bool released;

            internal RustFutureOwner(nint handle, global::System.Action<nint> cancel, global::System.Action<nint> free)
            {
                this.handle = handle;
                this.cancel = cancel;
                this.free = free;
            }

            internal nint Handle
            {
                get
                {
                    lock (gate)
                    {
                        if (released || handle == 0) throw new global::System.ObjectDisposedException(nameof(RustFutureOwner));
                        return handle;
                    }
                }
            }

            internal void Cancel()
            {
                lock (gate)
                {
                    if (cancelled || released || handle == 0) return;
                    cancelled = true;
                    cancel(handle);
                }
            }

            internal void Release()
            {
                lock (gate)
                {
                    if (released || handle == 0) return;
                    released = true;
                    nint releasedHandle = handle;
                    handle = 0;
                    free(releasedHandle);
                }
            }
        }

        private readonly struct VoidResult { }
    }
