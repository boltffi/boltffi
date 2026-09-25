#nullable enable

using System;
using Demo;
using static Demo.Demo;

namespace BoltFFI.Demo.Tests;

internal static class CallbackClassHandleTests
{
    private sealed class Receiver : MessageReceiver, FallibleMessageReceiver
    {
        public OwnedMessage? First;
        public OwnedMessage? Second;
        public bool Fail;

        public void Attach(OwnedMessage handle, uint callback)
        {
            Require(callback == 42, "callback argument keeps its value");
            First = handle;
        }
        public void Optional(OwnedMessage? handle) => First = handle;

        public int Pair(OwnedMessage first, string label, OwnedMessage? second)
        {
            Require(label == "pair", "pair label");
            First = first;
            Second = second;
            if (Fail) throw new MathErrorException(MathError.NegativeInput);
            return checked((int)(first.Length() + (second?.Length() ?? 0)));
        }
    }

    internal static void Run()
    {
        using var drops = new MessageDrops();
        var receiver = new Receiver();
        DeliverMessage(receiver, drops);
        Require(drops.Count() == 0 && receiver.First!.Length() == 9,
            "case:callbacks.class_handles.should_retain_after_return");
        receiver.First!.Dispose();
        receiver.First.Dispose();
        Require(drops.Count() == 1, "received message drops once");

        Require(DeliverMessagePair(receiver, drops, true) == 11,
            "case:callbacks.class_handles.should_deliver_multiple_and_optional");
        Require(drops.Count() == 1, "pair remains alive");
        Require(receiver.First!.Length() == 5 && receiver.Second!.Length() == 6, "received pair values");
        receiver.First.Dispose();
        Require(drops.Count() == 2, "first message drops independently");
        receiver.Second!.Dispose();
        Require(drops.Count() == 3, "second message drops independently");
        Require(DeliverMessagePair(receiver, drops, false) == 5 && receiver.Second == null, "optional absence");
        receiver.First!.Dispose();
        Require(drops.Count() == 4, "optional delivery drops once");

        receiver.Fail = true;
        try
        {
            DeliverMessagePair(receiver, drops, true);
            throw new Exception("callback error was lost");
        }
        catch (MathErrorException error)
        {
            Require(error.Error == MathError.NegativeInput,
                "case:callbacks.class_handles.should_retain_after_error");
        }
        Require(drops.Count() == 4, "throwing callback retains its messages");
        Require(receiver.First!.Length() == 5 && receiver.Second!.Length() == 6, "messages survive callback error");
        receiver.First.Dispose();
        receiver.Second!.Dispose();
        Require(drops.Count() == 6, "stored messages drop once after error");

        var measuring = MakeMessageReceiver();
        var moved = new OwnedMessage("moved", drops);
        measuring.Attach(moved, 42);
        moved.Dispose();
        Require(drops.Count() == 7, "case:callbacks.class_handles.should_consume_in_rust_callback");
        var optional = new OwnedMessage("optional", drops);
        measuring.Optional(optional);
        optional.Dispose();
        measuring.Optional(null);
        Require(drops.Count() == 8, "Rust callback consumes optional message");
        ((IDisposable)measuring).Dispose();
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new Exception(message);
    }
}
