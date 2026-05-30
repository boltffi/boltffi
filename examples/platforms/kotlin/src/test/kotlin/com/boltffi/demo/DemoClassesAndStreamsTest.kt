package com.boltffi.demo

import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull

class DemoClassesAndStreamsTest {
    @Test
    fun inventoryCounterAndMathUtilsCoverAllClassConstructorsAndMethods() {
        Inventory().use { inventory ->
            assertEquals(100u, inventory.capacity())
            assertEquals(0u, inventory.count())
            assertEquals(true, inventory.add("hammer"))
            assertEquals(listOf("hammer"), inventory.getAll())
            assertEquals("hammer", inventory.remove(0u))
            assertNull(inventory.remove(0u))
        }

        Inventory(2u).use { inventory ->
            assertEquals(2u, inventory.capacity())
            assertEquals(true, inventory.add("a"))
            assertEquals(true, inventory.add("b"))
            assertEquals(false, inventory.add("c"))
            assertEquals(listOf("a", "b"), inventory.getAll())
        }

        demoCase("case:classes.constructors.inventory.try_new.should_return_inventory_for_positive_capacity")
        Inventory.tryNew(1u).use { inventory ->
            assertEquals(1u, inventory.capacity())
            assertEquals(true, inventory.add("only"))
            assertEquals(false, inventory.add("overflow"))
        }

        demoCase("case:classes.constructors.inventory.try_new.should_reject_zero_capacity")
        assertMessageContains(assertFailsWith<FfiException> { Inventory.tryNew(0u) }, "capacity must be greater than zero")

        Counter(2).use { counter ->
            assertEquals(2, counter.get())
            counter.increment()
            assertEquals(3, counter.get())
            counter.add(7)
            assertEquals(10, counter.get())
            assertEquals(10, counter.tryGetPositive())
            assertEquals(20, counter.maybeDouble())
            assertPointEquals(10.0, 0.0, counter.asPoint())
            counter.reset()
            assertEquals(0, counter.get())
            assertNull(counter.maybeDouble())
            assertMessageContains(assertFailsWith<FfiException> { counter.tryGetPositive() }, "count is not positive")
            counter.add(4)
            counter.tryResetIfPositive()
            assertEquals(0, counter.get())
            assertMessageContains(assertFailsWith<FfiException> { counter.tryResetIfPositive() }, "count is not positive")
        }

        Counter(42).use { counter ->
            assertEquals("Counter(value=42)", describeCounter(counter))
        }

        MathUtils(2u).use { mathUtils ->
            assertDoubleEquals(3.14, mathUtils.round(3.14159), 1e-9)
        }
        assertEquals(9, MathUtils.add(4, 5))
        assertDoubleEquals(10.0, MathUtils.clamp(12.0, 0.0, 10.0))
        assertDoubleEquals(5.0, MathUtils.distanceBetween(Point(0.0, 0.0), Point(3.0, 4.0)))
        assertPointEquals(2.0, 3.0, MathUtils.midpoint(Point(1.0, 2.0), Point(3.0, 4.0)))
        assertEquals(42, MathUtils.parseInt("42"))
        assertMessageContains(assertFailsWith<FfiException> { MathUtils.parseInt("nope") }, "invalid digit found in string")
        assertDoubleEquals(3.0, MathUtils.safeSqrt(9.0)!!)
        assertNull(MathUtils.safeSqrt(-1.0))
    }

    @Test
    fun asyncWorkerSharedCounterAndStateHolderExerciseSyncAndAsyncMethods() = runBlocking {
        AsyncWorker("test").use { worker ->
            assertEquals("test", worker.getPrefix())
            assertEquals("test: data", worker.process("data"))
            assertEquals("test: data", worker.tryProcess("data"))
            assertMessageContains(assertFailsWith<FfiException> { worker.tryProcess("") }, "input must not be empty")
            assertEquals("test_42", worker.findItem(42))
            assertNull(worker.findItem(-1))
            assertEquals(listOf("test: x", "test: y"), worker.processBatch(listOf("x", "y")))
        }

        SharedCounter(5).use { counter ->
            assertEquals(5, counter.get())
            counter.set(6)
            assertEquals(6, counter.get())
            assertEquals(7, counter.increment())
            assertEquals(10, counter.add(3))
            assertEquals(10, counter.asyncGet())
            assertEquals(15, counter.asyncAdd(5))
        }

        StateHolder("local").use { holder ->
            val doubler = object : ValueCallback {
                override fun onValue(value: Int): Int = value * 2
            }
            assertEquals("local", holder.getLabel())
            assertEquals(0, holder.getValue())
            holder.setValue(5)
            assertEquals(5, holder.getValue())
            assertEquals(6, holder.increment())
            holder.addItem("a")
            holder.addItem("b")
            assertEquals(2u, holder.itemCount())
            assertEquals(listOf("a", "b"), holder.getItems())
            assertEquals("b", holder.removeLast())
            assertEquals(3, holder.transformValue(ClosureI32ToI32 { it / 2 }))
            assertEquals(6, holder.applyValueCallback(doubler))
            assertEquals(6, holder.asyncGetValue())
            holder.asyncSetValue(9)
            assertEquals(9, holder.getValue())
            assertEquals(2u, holder.asyncAddItem("z"))
            assertEquals(listOf("a", "z"), holder.getItems())
            holder.clear()
            assertEquals(0, holder.getValue())
            assertEquals(emptyList(), holder.getItems())
        }
    }

    @Test
    fun eventBusStreamsDeliverValuesAndPoints() = runBlocking {
        withTimeout(10_000) {
            EventBus().use { bus ->
                val valuesDeferred = async {
                    withTimeout(5_000) {
                        bus.subscribeValues().take(4).toList()
                    }
                }
                val pointsDeferred = async {
                    withTimeout(5_000) {
                        bus.subscribePoints().take(2).toList()
                    }
                }
                delay(100)
                bus.emitValue(1)
                assertEquals(3u, bus.emitBatch(intArrayOf(2, 3, 4)))
                bus.emitPoint(Point(1.0, 2.0))
                bus.emitPoint(Point(3.0, 4.0))
                assertEquals(listOf(1, 2, 3, 4), valuesDeferred.await())
                assertEquals(listOf(Point(1.0, 2.0), Point(3.0, 4.0)), pointsDeferred.await())
            }
        }
    }

    @Test
    fun eventBusBatchStreamDeliversValues() {
        EventBus().use { bus ->
            val sub = bus.subscribeValuesBatch()
            bus.emitValue(100)
            bus.emitValue(200)
            bus.emitValue(300)
            Thread.sleep(100)
            val batch = sub.popBatch(16)
            assert(batch.contains(100)) { "batch should contain 100" }
            assert(batch.contains(200)) { "batch should contain 200" }
            assert(batch.contains(300)) { "batch should contain 300" }
            sub.close()
        }
    }

    @Test
    fun eventBusCallbackStreamDeliversValues() = runBlocking {
        withTimeout(10_000) {
            EventBus().use { bus ->
                val received = java.util.concurrent.CopyOnWriteArrayList<Int>()
                val latch = java.util.concurrent.CountDownLatch(3)
                val cancellable = bus.subscribeValuesCallback { value ->
                    received.add(value)
                    latch.countDown()
                }
                delay(100)
                bus.emitValue(10)
                bus.emitValue(20)
                bus.emitValue(30)
                latch.await(5, java.util.concurrent.TimeUnit.SECONDS)
                assert(received.contains(10)) { "should contain 10" }
                assert(received.contains(20)) { "should contain 20" }
                assert(received.contains(30)) { "should contain 30" }
                cancellable.close()
            }
        }
    }
}
