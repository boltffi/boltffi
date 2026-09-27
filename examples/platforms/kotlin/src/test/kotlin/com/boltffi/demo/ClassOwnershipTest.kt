package com.boltffi.demo

import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.runBlocking
import java.lang.reflect.InvocationTargetException
import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs

class ClassOwnershipTest {
    @Test
    fun jniPreparationFailureReleasesTransferredClasses() {
        MessageDrops().use { drops ->
            val message = OwnedMessage("unread", drops)
            val native = Class.forName("com.boltffi.demo.Native")
            val join = native.getDeclaredMethod(
                "boltffi_function_demo_classes_ownership_join_message",
                ByteBuffer::class.java,
                Int::class.javaPrimitiveType,
                Long::class.javaPrimitiveType,
            )
            join.isAccessible = true
            val failure = assertFailsWith<InvocationTargetException> {
                join.invoke(null, ByteBuffer.allocate(4), 4, message.__boltffiTakeHandle())
            }
            assertIs<RuntimeException>(failure.cause)
            assertEquals(1u, drops.count())
            message.close()
            assertEquals(1u, drops.count())
        }
    }

    @Test
    fun transfersAndErrorsReleaseEachMessageOnce() = runBlocking {
        MessageDrops().use { drops ->
            val message = OwnedMessage("hello", drops)
            assertEquals(5u, consumeMessage(message))
            message.close()
            assertFailsWith<IllegalStateException> { message.length() }
            assertEquals(1u, drops.count())

            val rejected = OwnedMessage("", drops)
            assertFailsWith<RuntimeException> { consumeMessageResult(rejected) }
            rejected.close()
            assertEquals(2u, drops.count())

            val first = OwnedMessage("first", drops)
            val disposed = OwnedMessage("disposed", drops)
            disposed.close()
            assertFailsWith<IllegalStateException> { consumeMessages(first, disposed) }
            first.close()
            assertEquals(4u, drops.count())

            val duplicate = OwnedMessage("duplicate", drops)
            assertFailsWith<IllegalStateException> { consumeMessages(duplicate, duplicate) }
            duplicate.close()
            assertEquals(5u, drops.count())

            MessageStore().use { store ->
                val stored = OwnedMessage("stored", drops)
                store.set(stored, mapOf("trace" to "context"))
                stored.close()
                assertEquals(7u, store.count())
                assertEquals(6u, drops.count())
            }
            assertEquals(0u, consumeOptionalMessage(null))
            val moved = OwnedMessage("callback", drops)
            assertEquals(9u, consumeMessageWithCallback({ it + 1u }, moved))
            moved.close()
            assertEquals(7u, drops.count())
        }
    }

    @Test
    fun rejectedTransferDuringAsyncBorrowOutlivesTheWrapper() = runBlocking {
        MessageDrops().use { drops ->
            val message = OwnedMessage("retained", drops)
            val pending = async(start = CoroutineStart.UNDISPATCHED) { holdMessage(message) }
            assertEquals(1u, drops.borrowCount())
            assertEquals(0u, consumeMessage(message))
            message.close()
            assertEquals(0u, drops.count())
            pending.cancelAndJoin()
            assertEquals(0u, drops.borrowCount())
            assertEquals(1u, drops.count())
            message.close()
            assertEquals(1u, drops.count())
        }
    }
}
