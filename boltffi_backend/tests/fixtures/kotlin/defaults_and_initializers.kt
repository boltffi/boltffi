package com.boltffi.demo

fun main() {
    check(floor == Long.MIN_VALUE)
    check(ceiling == ULong.MAX_VALUE)
    check(Range() == Range(Long.MIN_VALUE, ULong.MAX_VALUE))
    check(Range.fromByteArray(Range().toByteArray()) == Range())
    check(Quota() == Quota(null))
    check(DefaultAmount() == DefaultAmount(3))
    check(DefaultAmount(value = 9).value == 9)
}

suspend fun checkDefaultArguments() {
    greet(name = "Ada")
    greet(name = "Ada", shout = false, suffix = "!")
    Server().use { server -> server.port() }
    Server.start(port = 8080u).use { server -> server.port(mapped = true) }
    formatDefaults(name = "Ada")
    formatDefaults(name = "Ada", enabled = false, weight = null)
    asyncDefault()
    DefaultCounter().use { counter ->
        counter.offset()
        counter.asyncOffset()
    }
    DefaultCounter(offset = 3).use { counter -> counter.offset() }
    DefaultCounter.sum(right = 5)
    DefaultAmount().offset()
    NamedAmount.withValue()
    NamedAmount.withValue(value = 9)
    DefaultMode.QUIET.matches()
    DefaultMode.load()
    NamedAmount.load()
    defaultAmount()
    span()
    span(end = 0uL)
    throttle()
    Ledger.new(Long.MIN_VALUE)
    Tally.new(ULong.MAX_VALUE)
}
