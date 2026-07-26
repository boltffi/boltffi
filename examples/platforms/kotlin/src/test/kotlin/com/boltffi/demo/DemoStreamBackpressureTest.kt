package com.boltffi.demo

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlin.test.Test
import kotlin.test.assertEquals

class DemoStreamBackpressureTest {
    private fun burstThrough(bus: EventBus, perItemDelayMs: Long): List<Int> = runBlocking {
        val ready = CompletableDeferred<Unit>()
        val items = async(Dispatchers.Default) {
            withTimeout(15_000) {
                bus.subscribeValues()
                    .onEach { value ->
                        if (value == -1) ready.complete(Unit)
                        if (perItemDelayMs > 0) delay(perItemDelayMs)
                    }
                    .filter { it > 0 }
                    .take(200)
                    .toList()
            }
        }
        while (!ready.isCompleted) {
            bus.emitValue(-1)
            delay(20)
        }
        assertEquals(200u, bus.emitBatch(IntArray(200) { it + 1 }))
        items.await()
    }

    @Test
    fun burstDeliversEveryItemToFastCollector() {
        EventBus().use { bus ->
            assertEquals((1..200).toList(), burstThrough(bus, perItemDelayMs = 0))
        }
    }

    @Test
    fun burstSuspendsInsteadOfDroppingForSlowCollector() {
        EventBus().use { bus ->
            assertEquals((1..200).toList(), burstThrough(bus, perItemDelayMs = 1))
        }
    }
}
