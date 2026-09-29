@file:OptIn(kotlin.ExperimentalUnsignedTypes::class)

package com.boltffi.demo

import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout

private fun requireThat(condition: Boolean, message: String) {
    if (!condition) {
        throw IllegalStateException(message)
    }
}

fun main() {
    requireThat(
        echoVecIsize(longArrayOf(-3L, 0L, 9L)).contentEquals(longArrayOf(-3L, 0L, 9L)),
        "echoVecIsize failed",
    )
    requireThat(
        echoVecUsize(ulongArrayOf(0uL, 7uL, 42uL)).contentEquals(ulongArrayOf(0uL, 7uL, 42uL)),
        "echoVecUsize failed",
    )

    Inventory.tryNew(3u).use { inventory ->
        requireThat(inventory.capacity() == 3u, "Inventory.tryNew capacity failed")
        requireThat(inventory.count() == 0u, "Inventory.tryNew initial count failed")
        requireThat(inventory.add("alpha"), "Inventory.add failed")
        requireThat(inventory.count() == 1u, "Inventory.count after add failed")
    }

    val toggled = mapStatus(
        mapper = object : StatusMapper {
            override fun mapStatus(status: Status): Status =
                if (status == Status.ACTIVE) Status.INACTIVE else Status.ACTIVE
        },
        status = Status.ACTIVE,
    )
    requireThat(toggled == Status.INACTIVE, "mapStatus callback failed")

    requireThat(directionToDegrees(Direction.NORTH) == 0, "directionToDegrees failed")
    requireThat(oppositeDirection(Direction.NORTH) == Direction.SOUTH, "oppositeDirection failed")

    val origin = makePoint(0.0, 0.0)
    requireThat(origin == Point(0.0, 0.0), "Point.origin failed")

    runBlocking {
        withTimeout(5_000) {
            EventBus().use { bus ->
                val points = async {
                    bus.subscribePoints().take(2).toList()
                }
                delay(100)
                bus.emitPoint(Point(1.0, 2.0))
                bus.emitPoint(Point(3.0, 4.0))
                requireThat(
                    points.await() == listOf(Point(1.0, 2.0), Point(3.0, 4.0)),
                    "EventBus.subscribePoints failed",
                )
            }
        }
    }

    println("Kotlin smoke test passed")
}
