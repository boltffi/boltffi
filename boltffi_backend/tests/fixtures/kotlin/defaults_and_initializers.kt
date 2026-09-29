package com.boltffi.demo

fun main() {
    check(floor == Long.MIN_VALUE)
    check(ceiling == ULong.MAX_VALUE)
    check(Range() == Range(Long.MIN_VALUE, ULong.MAX_VALUE))
    check(Range.fromByteArray(Range().toByteArray()) == Range())
    check(Quota() == Quota(null))
}

// Compiled, not run: each call reaches the native library.
@Suppress("unused")
private fun callers() {
    span()
    span(end = 0uL)
    throttle()
    Ledger.new(Long.MIN_VALUE)
    Tally.new(ULong.MAX_VALUE)
}
