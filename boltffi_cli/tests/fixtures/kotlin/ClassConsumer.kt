package com.boltffi.constructors

fun main() {
    DefaultedWideCounter().use { check(it.value() == 10L) }
    DefaultedWideCounter(value = Long.MIN_VALUE).use { check(it.value() == Long.MIN_VALUE) }
    DefaultedWideCounter(Long.MAX_VALUE).use { check(it.value() == Long.MAX_VALUE) }

    val consumed = DefaultedWideCounter(4L)
    DefaultedWideCounter(counter = consumed).use { check(it.value() == 5L) }
    check(runCatching { consumed.value() }.exceptionOrNull() is IllegalStateException)
    consumed.close()
    consumed.close()

    val closed = DefaultedWideCounter(9L)
    val closers = List(8) { Thread { closed.close() } }
    closers.forEach { it.start() }
    closers.forEach { it.join() }
    check(runCatching { closed.value() }.exceptionOrNull() is IllegalStateException)
    closed.close()
}
