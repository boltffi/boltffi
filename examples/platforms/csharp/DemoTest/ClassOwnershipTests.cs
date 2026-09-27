using System;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using Demo;
using static Demo.Demo;

namespace BoltFFI.Demo.Tests;

internal static class ClassOwnershipTests
{
    private static int failNextImport;

    internal static async System.Threading.Tasks.Task Run()
    {
        Console.WriteLine("Testing class ownership...");
        NativeLibrary.SetDllImportResolver(typeof(OwnedMessage).Assembly, (_, _, _) =>
        {
            if (Interlocked.Exchange(ref failNextImport, 0) != 0)
                throw new EntryPointNotFoundException("injected native entry failure");
            return IntPtr.Zero;
        });
        await AsyncStoreTakesMessage();
        SuccessfulTransfers();
        RustErrorDropsMessage();
        ReturnedMessageStaysAlive();
        BorrowingPreservesMessage();
        PreparationFailureReleasesDetachedMessages();
        EncodingFailurePreservesMessage();
        NativeEntryFailureReleasesMessage();
        ClosurePreparationReleasesCaptures();
        await AsyncTransfers();
        await CancellationDropsMessage();
        await ActiveBorrowDelaysDrop();
        FinalizersDoNotReleaseMovedMessages();
        Console.WriteLine("  PASS\n");
    }

    private static async System.Threading.Tasks.Task AsyncStoreTakesMessage()
    {
        using var drops = new MessageDrops();
        using var store = new MessageStore();
        var message = new OwnedMessage("hello", drops);
        await store.Set(message, new() { ["trace"] = "context" });
        Require(store.Count() == 6, "async Result<(), E> executes the Rust method successfully");
        AssertMoved(message);
        Require(drops.Count() == 1, "successful async store drops its message exactly once");
    }

    private static void SuccessfulTransfers()
    {
        CheckConsumed(message => Require(ConsumeMessage(message) == 5, "primitive return"));
        CheckConsumed(message => Require(ConsumeNamedMessage(message, 1, 2, 3) == 11, "parameter names preserve arguments"));
        CheckConsumed(DiscardMessage);
        CheckConsumed(message => Require(ConsumeOptionalMessage(message) == 5, "optional input"));
        CheckConsumed(message => Require(ConsumeMessageResult(message) == 5, "fallible success"));
        CheckConsumed(message => Require(JoinMessage(["a", "b"], message) == "ahellob", "encoded return"));
        CheckConsumed(message =>
        {
            using var store = MessageStore.FromMessage(message);
            Require(store.Count() == 5, "class initializer takes its message");
        });
        Require(ConsumeOptionalMessage(null) == 0, "optional null remains null");

        using var drops = new MessageDrops();
        var first = new OwnedMessage("first", drops);
        var second = new OwnedMessage("second", drops);
        Require(ConsumeMessages(first, second) == 11, "two owned arguments reach Rust");
        AssertMoved(first);
        AssertMoved(second);
        Require(drops.Count() == 2, "two owned arguments each drop once");
    }

    private static void RustErrorDropsMessage()
    {
        using var drops = new MessageDrops();
        var message = new OwnedMessage("", drops);
        Expect<BoltException>(() => ConsumeMessageResult(message));
        AssertMoved(message);
        Require(drops.Count() == 1, "Rust Err drops the message before C# throws");
        Expect<ObjectDisposedException>(() => ConsumeOptionalMessage(message));
        Require(drops.Count() == 1, "disposed optional argument is rejected without another drop");
    }

    private static void ReturnedMessageStaysAlive()
    {
        using var drops = new MessageDrops();
        var original = new OwnedMessage("returned", drops);
        var returned = ReturnMessage(original);
        AssertMoved(original);
        Require(drops.Count() == 0 && returned.Length() == 8, "returned ownership remains usable");
        returned.Dispose();
        returned.Dispose();
        Require(drops.Count() == 1, "returned message drops exactly once");
    }

    private static void BorrowingPreservesMessage()
    {
        using var drops = new MessageDrops();
        var message = new OwnedMessage("borrowed", drops);
        Require(BorrowMessage(message) == 8, "shared borrow reads the message");
        MutateMessage(message);
        Require(message.Length() == 9 && drops.Count() == 0, "mutable borrow preserves ownership");
        message.Dispose();
        Require(drops.Count() == 1, "caller releases a borrowed message");
    }

    private static void PreparationFailureReleasesDetachedMessages()
    {
        using var drops = new MessageDrops();
        var first = new OwnedMessage("first", drops);
        Expect<NullReferenceException>(() => ConsumeMessages(first, null));
        AssertMoved(first);
        Require(drops.Count() == 1, "a null second argument releases the detached first argument");

        var untouched = new OwnedMessage("untouched", drops);
        Expect<NullReferenceException>(() => ConsumeMessages(null, untouched));
        Require(untouched.Length() == 9 && drops.Count() == 1, "a null first argument leaves the second untouched");
        untouched.Dispose();

        first = new OwnedMessage("first", drops);
        var disposed = new OwnedMessage("disposed", drops);
        disposed.Dispose();
        Expect<ObjectDisposedException>(() => ConsumeMessages(first, disposed));
        AssertMoved(first);
        Require(drops.Count() == 4, "a disposed second argument releases the first exactly once");

        var duplicate = new OwnedMessage("duplicate", drops);
        Expect<ObjectDisposedException>(() => ConsumeMessages(duplicate, duplicate));
        AssertMoved(duplicate);
        Require(drops.Count() == 5, "passing the same wrapper twice drops it once");

        var receiver = new OwnedMessage("receiver", drops);
        Require(receiver.Combine(receiver) == 0, "a receiver moved into its own argument is rejected");
        AssertMoved(receiver);
        Require(drops.Count() == 6, "rejected receiver alias drops its message once");
    }

    private static void EncodingFailurePreservesMessage()
    {
        using var drops = new MessageDrops();
        var message = new OwnedMessage("unprepared", drops);
        Expect<ArgumentNullException>(() => JoinMessage([null], message));
        Require(message.Length() == 10 && drops.Count() == 0, "encoding failure precedes ownership transfer");
        using var store = new MessageStore();
        Expect<NullReferenceException>(() => store.Set(message, null));
        Require(message.Length() == 10 && drops.Count() == 0, "a later encoded argument fails before detaching the message");
        message.Dispose();
        Require(drops.Count() == 1, "message remains disposable after encoding failure");
    }

    private static void NativeEntryFailureReleasesMessage()
    {
        using var drops = new MessageDrops();
        var message = new OwnedMessage("missing", drops);
        Interlocked.Exchange(ref failNextImport, 1);
        Expect<EntryPointNotFoundException>(() => ConsumeMessageWithOffset(message, 1, 2, 3));
        Require(failNextImport == 0, "native import failure was exercised");
        AssertMoved(message);
        Require(drops.Count() == 1, "native entry failure releases detached ownership");

        CheckConsumed(value => Require(ConsumeMessageWithOffset(value, 1, 2, 3) == 11, "parameter names do not collide with generated locals"));
    }

    private static void ClosurePreparationReleasesCaptures()
    {
        using var drops = new MessageDrops();
        WeakReference[] captures = [CallWithCapture(drops, false), CallWithCapture(drops, true), RejectNativeCallWithCapture(drops)];
        GC.Collect();
        GC.WaitForPendingFinalizers();
        GC.Collect();
        Require(Array.TrueForAll(captures, capture => !capture.IsAlive), "closures release captures after success, preparation failure, and missing native entry");
        Require(drops.Count() == 3, "closure calls release every message exactly once");
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static WeakReference CallWithCapture(MessageDrops drops, bool disposed)
    {
        var captured = new uint[] { 7 };
        var message = new OwnedMessage("hello", drops);
        if (disposed)
        {
            message.Dispose();
            Expect<ObjectDisposedException>(() => ConsumeMessageWithCallback(value => value + captured[0], message));
        }
        else
        {
            Require(ConsumeMessageWithCallback(value => value + captured[0], message) == 12, "closure receives the owned message's length");
        }
        AssertMoved(message);
        return new WeakReference(captured);
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static WeakReference RejectNativeCallWithCapture(MessageDrops drops)
    {
        var captured = new uint[] { 7 };
        var message = new OwnedMessage("hello", drops);
        Interlocked.Exchange(ref failNextImport, 1);
        Expect<EntryPointNotFoundException>(() => ConsumeMessageBeforeCallback(message, value => value + captured[0]));
        Require(failNextImport == 0, "closure native entry failure was exercised");
        AssertMoved(message);
        return new WeakReference(captured);
    }

    private static async System.Threading.Tasks.Task AsyncTransfers()
    {
        using var drops = new MessageDrops();
        var message = new OwnedMessage("async", drops);
        Require(await ConsumeMessageAsync(message) == 5, "async fallible success returns its value");
        AssertMoved(message);
        Require(drops.Count() == 1, "async success drops once");

        message = new OwnedMessage("", drops);
        await ExpectAsync<BoltException>(() => ConsumeMessageAsync(message));
        AssertMoved(message);
        Require(drops.Count() == 2, "async Rust Err drops once");

        message = new OwnedMessage("first", drops);
        await ExpectAsync<NullReferenceException>(() => ConsumeMessagesAsync(message, null));
        AssertMoved(message);
        Require(drops.Count() == 3, "async preparation failure releases detached ownership");

        message = new OwnedMessage("optional", drops);
        Require(await ConsumeOptionalMessageAsync(message) == 8, "async optional argument reaches Rust");
        AssertMoved(message);
        Require(drops.Count() == 4, "async optional argument drops once");
        Require(await ConsumeOptionalMessageAsync(null) == 0, "async optional null remains null");

        message = new OwnedMessage("first", drops);
        var second = new OwnedMessage("second", drops);
        Require(await ConsumeMessagesAsync(message, second) == 11, "two async owned arguments reach Rust");
        AssertMoved(message);
        AssertMoved(second);
        Require(drops.Count() == 6, "two async owned arguments each drop once");
    }

    private static async System.Threading.Tasks.Task CancellationDropsMessage()
    {
        using var drops = new MessageDrops();
        using var cancellation = new CancellationTokenSource();
        var message = new OwnedMessage("pending", drops);
        Task<uint> pending = HoldOwnedMessage(message, cancellation.Token);
        AssertMoved(message);
        Require(drops.Count() == 0, "pending future owns the message");
        cancellation.Cancel();
        await ExpectAsync<OperationCanceledException>(() => pending);
        Require(drops.Count() == 1, "cancelling the future drops its message once");

        message = new OwnedMessage("pre-cancelled", drops);
        await ExpectAsync<OperationCanceledException>(() => HoldOwnedMessage(message, new CancellationToken(true)));
        AssertMoved(message);
        Require(drops.Count() == 2, "pre-cancelled call drops transferred ownership once");
    }

    private static async System.Threading.Tasks.Task ActiveBorrowDelaysDrop()
    {
        using var drops = new MessageDrops();
        using var cancellation = new CancellationTokenSource();
        var message = new OwnedMessage("retained", drops);
        Task<uint> pending = HoldMessage(message, cancellation.Token);
        Require(ConsumeMessage(message) == 0, "moving an actively borrowed message is rejected");
        AssertMoved(message);
        Require(drops.Count() == 0, "borrow keeps a rejected transfer alive");
        cancellation.Cancel();
        await ExpectAsync<OperationCanceledException>(() => pending);
        Require(drops.Count() == 1, "last borrow releases rejected ownership");
    }

    private static void FinalizersDoNotReleaseMovedMessages()
    {
        using var drops = new MessageDrops();
        WeakReference[] wrappers = Enumerable.Repeat(drops, 128)
            .Select(ConsumeWithoutDisposing)
            .ToArray();
        GC.Collect();
        GC.WaitForPendingFinalizers();
        GC.Collect();
        Require(Array.TrueForAll(wrappers, wrapper => !wrapper.IsAlive), "consumed wrappers were finalized");
        Require(drops.Count() == wrappers.Length, "finalizers never release consumed handles again");
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static WeakReference ConsumeWithoutDisposing(MessageDrops drops)
    {
        var message = new OwnedMessage("finalized", drops);
        Require(ConsumeMessage(message) == 9, "finalizer test consumed its message");
        return new WeakReference(message);
    }

    private static void CheckConsumed(Action<OwnedMessage> consume)
    {
        using var drops = new MessageDrops();
        var message = new OwnedMessage("hello", drops);
        consume(message);
        AssertMoved(message);
        Require(drops.Count() == 1, "consumed message drops exactly once");
    }

    private static void AssertMoved(OwnedMessage message)
    {
        Expect<ObjectDisposedException>(() => message.Length());
        Expect<ObjectDisposedException>(() => ConsumeMessage(message));
        message.Dispose();
        message.Dispose();
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

    private static async System.Threading.Tasks.Task ExpectAsync<TException>(Func<System.Threading.Tasks.Task> action) where TException : Exception
    {
        try
        {
            await action().WaitAsync(TimeSpan.FromSeconds(10));
        }
        catch (TException)
        {
            return;
        }
        throw new InvalidOperationException($"expected {typeof(TException).Name}");
    }

    private static void Require(bool condition, string assertion)
    {
        if (!condition) throw new InvalidOperationException(assertion);
    }
}
