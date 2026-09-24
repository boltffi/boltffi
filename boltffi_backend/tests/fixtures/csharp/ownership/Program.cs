using System;
using System.Threading;
using System.Threading.Tasks;
using Ownership;
using static Ownership.Demo;

internal static class Program
{
    private static async Task Main()
    {
        CheckOwned(message => Check(Consume(message) == 5, "sync result"));
        CheckOwned(Discard);
        CheckOwned(message => Sink.FromMessage(message).Dispose());
        CheckOwned(message => Check(ConsumeOptional(message) == 5, "optional result"));
        CheckOwned(message => Check(ConsumeFallible(message) == 5, "fallible success out parameter"));
        CheckOwned(message => Check(ConsumeEncoded(["a", "b"], message) == "ahellob", "encoded result"));
        Check(ConsumeOptional(null) == 0, "nullable null is accepted");

        var error = new Message("");
        uint before = DroppedMessages();
        Expect<BoltException>(() => ConsumeFallible(error));
        CheckMoved(error);
        Check(DroppedMessages() - before == 1, "Rust error drops its argument");

        var original = new Message("echo");
        before = DroppedMessages();
        var returned = Echo(original);
        CheckMoved(original);
        Check(DroppedMessages() == before, "returned ownership stays alive");
        Check(returned.Length() == 4, "returned handle is usable");
        returned.Dispose();
        Check(DroppedMessages() - before == 1, "returned ownership drops once");

        var borrowed = new Message("borrowed");
        ulong borrowedHandle = borrowed.Handle;
        Check(Inspect(borrowed) == 8, "shared borrow result");
        Modify(borrowed);
        Check(borrowed.Handle == borrowedHandle && borrowed.Length() == 9, "borrows preserve ownership");
        borrowed.Dispose();

        var first = new Message("first");
        before = DroppedMessages();
        Expect<NullReferenceException>(() => ConsumePair(first, null!));
        CheckMoved(first);
        Check(DroppedMessages() - before == 1, "partial preparation releases the first handle");

        first = new Message("first");
        var disposed = new Message("disposed");
        disposed.Dispose();
        before = DroppedMessages();
        Expect<ObjectDisposedException>(() => ConsumePair(first, disposed));
        CheckMoved(first);
        Check(DroppedMessages() - before == 1, "disposed later argument cleans up the first");
        Expect<ObjectDisposedException>(() => ConsumeOptional(disposed));

        var duplicate = new Message("duplicate");
        before = DroppedMessages();
        Expect<ObjectDisposedException>(() => ConsumePair(duplicate, duplicate));
        CheckMoved(duplicate);
        Check(DroppedMessages() - before == 1, "aliased owned arguments drop once");

        var unprepared = new Message("unprepared");
        before = DroppedMessages();
        Expect<ArgumentNullException>(() => ConsumeEncoded([null!], unprepared));
        Check(unprepared.Handle != 0, "serialization failure leaves ownership with the caller");
        unprepared.Dispose();
        Check(DroppedMessages() - before == 1, "unpassed value remains disposable");

        var missing = new Message("missing");
        before = DroppedMessages();
        Expect<EntryPointNotFoundException>(() => MissingEntry(missing));
        CheckMoved(missing);
        Check(DroppedMessages() - before == 1, "P/Invoke entry failure releases detached ownership");

        var later = new Message("later");
        before = DroppedMessages();
        Check(NativeMethods.NativeConsumePair(0, later.TakeHandle()) == 0, "native null first argument is rejected");
        CheckMoved(later);
        Check(DroppedMessages() - before == 1, "native rejection releases later owned arguments");

        later = new Message("later");
        before = DroppedMessages();
        var failure = NativeMethods.NativeConsumeEncoded([], 0, later.TakeHandle());
        NativeMethods.FreeBuf(failure);
        CheckMoved(later);
        Check(DroppedMessages() - before == 1, "native decode failure releases owned arguments");

        later = new Message("later");
        before = DroppedMessages();
        Check(NativeMethods.NativeMessageCombine(0, later.TakeHandle()) == 0, "native null receiver is rejected");
        CheckMoved(later);
        Check(DroppedMessages() - before == 1, "receiver rejection releases owned arguments");

        later = new Message("later");
        before = DroppedMessages();
        nint rejected = NativeMethods.NativeSinkSet(0, later.TakeHandle(), [], 0);
        NativeMethods.NativeSinkSetFree(rejected);
        CheckMoved(later);
        Check(DroppedMessages() - before == 1, "async receiver rejection releases owned arguments");

        var aliasedReceiver = new Message("alias");
        before = DroppedMessages();
        Check(aliasedReceiver.Combine(aliasedReceiver) == 0, "moved receiver alias is rejected");
        CheckMoved(aliasedReceiver);
        Check(DroppedMessages() - before == 1, "receiver alias drops once");

        using (var sink = new Sink())
        {
            var asynchronous = new Message("async");
            before = DroppedMessages();
            await sink.Set(asynchronous, new() { ["trace"] = "context" });
            CheckMoved(asynchronous);
            Check(DroppedMessages() - before == 1, "successful async Result<(), E> drops once");
        }

        var asyncValue = new Message("async");
        before = DroppedMessages();
        Check(await ConsumeAsync(asyncValue) == 5, "async success out parameter");
        CheckMoved(asyncValue);
        Check(DroppedMessages() - before == 1, "async success drops once");

        asyncValue = new Message("");
        before = DroppedMessages();
        try
        {
            await ConsumeAsync(asyncValue);
            throw new InvalidOperationException("expected Rust error");
        }
        catch (BoltException) { }
        CheckMoved(asyncValue);
        Check(DroppedMessages() - before == 1, "async Rust error drops once");

        first = new Message("first");
        before = DroppedMessages();
        try
        {
            await ConsumePairAsync(first, null!);
            throw new InvalidOperationException("expected null argument rejection");
        }
        catch (NullReferenceException) { }
        CheckMoved(first);
        Check(DroppedMessages() - before == 1, "async preparation failure releases detached ownership");

        asyncValue = new Message("optional");
        before = DroppedMessages();
        Check(await ConsumeOptionalAsync(asyncValue) == 8, "async nullable owned result");
        CheckMoved(asyncValue);
        Check(DroppedMessages() - before == 1, "async nullable owned argument drops once");
        Check(await ConsumeOptionalAsync(null) == 0, "async nullable null is accepted");

        var retained = new Message("retained");
        before = DroppedMessages();
        using (var cancellation = new CancellationTokenSource())
        {
            Task<uint> pending = Hold(retained, cancellation.Token);
            Check(Consume(retained) == 0, "taking an actively borrowed value is rejected");
            CheckMoved(retained);
            Check(DroppedMessages() == before, "borrow keeps rejected transfer alive");
            cancellation.Cancel();
            await CheckCancelled(pending);
        }
        Check(DroppedMessages() - before == 1, "last borrow releases rejected transferred ownership");

        var cancelled = new Message("cancelled");
        before = DroppedMessages();
        using (var cancellation = new CancellationTokenSource())
        {
            Task<uint> pending = HoldOwned(cancelled, cancellation.Token);
            CheckMoved(cancelled);
            Check(DroppedMessages() == before, "pending future owns its value");
            cancellation.Cancel();
            await CheckCancelled(pending);
        }
        Check(DroppedMessages() - before == 1, "cancelled future drops its owned value");

        cancelled = new Message("already cancelled");
        before = DroppedMessages();
        await CheckCancelled(HoldOwned(cancelled, new CancellationToken(true)));
        CheckMoved(cancelled);
        Check(DroppedMessages() - before == 1, "pre-cancelled call releases transferred ownership");

        FinalizeMoved();
        before = DroppedMessages();
        GC.Collect();
        GC.WaitForPendingFinalizers();
        Check(DroppedMessages() == before, "finalizers do not release consumed handles again");
        Console.WriteLine("C# ownership and native destructor checks passed");
    }

    private static void CheckOwned(Action<Message> call)
    {
        var message = new Message("hello");
        uint before = DroppedMessages();
        call(message);
        CheckMoved(message);
        Check(DroppedMessages() - before == 1, "successful transfer drops exactly once");
    }

    private static void CheckMoved(Message message)
    {
        if (message.Handle != 0)
        {
            GC.SuppressFinalize(message);
            throw new InvalidOperationException("consumed wrapper retains a dangling handle");
        }
        message.Dispose();
        message.Dispose();
        Expect<ObjectDisposedException>(() => message.Length());
        Expect<ObjectDisposedException>(() => Consume(message));
    }

    [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.NoInlining)]
    private static void FinalizeMoved()
    {
        var message = new Message("finalized");
        Consume(message);
        Check(message.Handle == 0, "finalized wrapper is detached");
    }

    private static async Task CheckCancelled(Task<uint> pending)
    {
        try
        {
            await pending.WaitAsync(TimeSpan.FromSeconds(10));
            throw new InvalidOperationException("expected cancellation");
        }
        catch (OperationCanceledException) { }
    }

    private static void Expect<TException>(Action action) where TException : Exception
    {
        try
        {
            action();
        }
        catch (TException)
        {
            return;
        }
        throw new InvalidOperationException($"expected {typeof(TException).Name}");
    }

    private static void Check(bool condition, string assertion)
    {
        if (!condition) throw new InvalidOperationException(assertion);
    }
}
