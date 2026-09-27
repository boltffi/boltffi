package com.boltffi.demo

import java.lang.reflect.InvocationTargetException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

class CallbackClassHandleTest {
    private class Receiver : MessageReceiver, FallibleMessageReceiver {
        var first: OwnedMessage? = null
        var second: OwnedMessage? = null
        var fail = false
        var unexpected = false
        var forward: MessageReceiver? = null

        override fun attach(handle: OwnedMessage, callback: UInt) {
            assertEquals(42u, callback)
            assertEquals(9u, handle.length())
            val target = forward
            if (target == null) first = handle else target.attach(handle, callback)
        }

        override fun optional(handle: OwnedMessage?) {
            first = handle
        }

        override fun pair(first: OwnedMessage, label: String, second: OwnedMessage?): Int {
            assertEquals("pair", label)
            this.first = first
            this.second = second
            if (unexpected) throw IllegalStateException("receiver failed")
            if (fail) throw MathError.NegativeInput
            return (first.length() + (second?.length() ?: 0u)).toInt()
        }
    }

    @Test
    fun retainedMessageOutlivesCallback() {
        demoCase("case:callbacks.class_handles.should_retain_after_return")
        MessageDrops().use { drops ->
            val receiver = Receiver()
            deliverMessage(receiver, drops)
            assertEquals(0u, drops.count())
            assertEquals(9u, receiver.first!!.length())
            receiver.first!!.close()
            receiver.first!!.close()
            assertEquals(1u, drops.count())
        }
    }

    @Test
    fun multipleAndOptionalMessages() {
        demoCase("case:callbacks.class_handles.should_deliver_multiple_and_optional")
        MessageDrops().use { drops ->
            val receiver = Receiver()
            assertEquals(11, deliverMessagePair(receiver, drops, true))
            assertEquals(0u, drops.count())
            assertEquals(5u, receiver.first!!.length())
            assertEquals(6u, receiver.second!!.length())
            receiver.first!!.close()
            assertEquals(1u, drops.count())
            receiver.second!!.close()
            assertEquals(2u, drops.count())
            assertEquals(5, deliverMessagePair(receiver, drops, false))
            assertNull(receiver.second)
            assertEquals(2u, drops.count())
            receiver.first!!.close()
            assertEquals(3u, drops.count())
        }
    }

    @Test
    fun storedMessagesSurviveCallbackError() {
        demoCase("case:callbacks.class_handles.should_retain_after_error")
        MessageDrops().use { drops ->
            val receiver = Receiver().apply { fail = true }
            assertFailsWith<MathError.NegativeInput> { deliverMessagePair(receiver, drops, true) }
            assertEquals(0u, drops.count())
            assertEquals(5u, receiver.first!!.length())
            assertEquals(6u, receiver.second!!.length())
            receiver.first!!.close()
            receiver.second!!.close()
            assertEquals(2u, drops.count())
        }
    }

    @Test
    fun rustCallbackConsumesMessages() {
        demoCase("case:callbacks.class_handles.should_consume_in_rust_callback")
        MessageDrops().use { drops ->
            val receiver = makeMessageReceiver()
            val first = OwnedMessage("hello", drops)
            receiver.attach(first, 42u)
            assertFailsWith<IllegalStateException> { first.length() }
            first.close()
            assertEquals(1u, drops.count())
            val second = OwnedMessage("two", drops)
            receiver.optional(second)
            second.close()
            assertEquals(2u, drops.count())
            receiver.optional(null)
            assertEquals(2u, drops.count())
            (receiver as AutoCloseable).close()
        }
    }

    @Test
    fun failedCallbackLookupReleasesUndeliveredMessages() {
        MessageDrops().use { drops ->
            val receiver = Receiver()
            deliverMessagePair(receiver, drops, false)
            receiver.first!!.close()
            val native = Class.forName("com.boltffi.demo.Native")
            val deliver = native.getDeclaredMethod(
                "boltffi_function_demo_callbacks_class_handles_deliver_message_pair",
                Long::class.javaPrimitiveType,
                Long::class.javaPrimitiveType,
                Boolean::class.javaPrimitiveType,
            )
            deliver.isAccessible = true
            val failure = assertFailsWith<InvocationTargetException> {
                deliver.invoke(null, Long.MAX_VALUE, drops.handle, true)
            }
            assertIs<BoltFfiErrorBufferException>(failure.cause)
            assertEquals(3u, drops.count())
        }
    }

    @Test
    fun argumentDecodeFailureReleasesAllUndeliveredMessages() {
        MessageDrops().use { drops ->
            val receiver = Receiver()
            val identity = FallibleMessageReceiverBridge.create(receiver)
            val first = OwnedMessage("first", drops)
            val second = OwnedMessage("second", drops)
            assertTrue(CallbackClassHandleFaults.deliverMalformedPair(
                identity, first.__boltffiTakeHandle(), second.__boltffiTakeHandle(),
            ))
            assertNull(receiver.first)
            assertNull(receiver.second)
            assertEquals(2u, drops.count())
            first.close()
            second.close()
            assertEquals(2u, drops.count())
        }
    }

    @Test
    fun unexpectedExceptionDoesNotReleaseDeliveredMessages() {
        MessageDrops().use { drops ->
            val receiver = Receiver().apply { unexpected = true }
            assertFailsWith<MathError.Overflow> {
                deliverMessagePair(receiver, drops, true)
            }
            assertEquals(0u, drops.count())
            assertEquals(5u, receiver.first!!.length())
            assertEquals(6u, receiver.second!!.length())
            receiver.first!!.close()
            receiver.second!!.close()
            assertEquals(2u, drops.count())
        }
    }

    @Test
    fun callbackCanImmediatelyPassOwnershipBackToRust() {
        MessageDrops().use { drops ->
            val target = makeMessageReceiver()
            try {
                val receiver = Receiver().apply { forward = target }
                deliverMessage(receiver, drops)
                assertNull(receiver.first)
                assertEquals(1u, drops.count())
            } finally {
                (target as AutoCloseable).close()
            }
        }
    }

    @Test
    fun closedRustCallbackDoesNotLeakOrReleaseAMessageTwice() {
        MessageDrops().use { drops ->
            val receiver = makeMessageReceiver()
            (receiver as AutoCloseable).close()
            val message = OwnedMessage("undelivered", drops)
            assertFailsWith<IllegalStateException> { receiver.attach(message, 42u) }
            message.close()
            assertEquals(1u, drops.count())
        }
    }
}

private object CallbackClassHandleFaults {
    init {
        System.loadLibrary("callback_class_handles")
    }

    @JvmStatic
    external fun deliverMalformedPair(identity: Long, first: Long, second: Long): Boolean
}
