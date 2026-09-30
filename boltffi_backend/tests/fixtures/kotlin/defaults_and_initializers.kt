package com.boltffi.demo

fun main() {
    check(floor == Long.MIN_VALUE)
    check(ceiling == ULong.MAX_VALUE)
    check(Range() == Range(Long.MIN_VALUE, ULong.MAX_VALUE))
    check(Range.fromByteArray(Range().toByteArray()) == Range())
    check(Quota() == Quota(null))
    check(DefaultAmount() == DefaultAmount(3))
    check(DefaultAmount(value = 9).value == 9)
    check(FloatDefaults().single.toRawBits() == Int.MIN_VALUE)
    check(FloatDefaults().double.toRawBits() == Long.MIN_VALUE)
    check(ContactDefaults().email.toString() == "mailto:ada@example.com")
    check(ContactDefaults().identifier.toString() == "01234567-89ab-cdef-0123-456789abcdef")
    check(ContactDefaults().optionalEmail == null)
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
    DefaultCounter.fromText().use { counter -> counter.offset() }
    DefaultCounter.fromText(value = "12").use { counter -> counter.offset() }
    DefaultCounter.sum(right = 5)
    DefaultAmount().offset()
    DefaultAmount.withScaledValue()
    DefaultAmount.tryScaledValue()
    NamedAmount.withValue()
    NamedAmount.empty()
    NamedAmount.withValue(value = 9)
    DefaultMode.QUIET.matches()
    DefaultMode.new()
    DefaultMode.withMode()
    DefaultMode.load()
    NamedAmount.load()
    defaultAmount()
    span()
    span(end = 0uL)
    throttle()
    ContactDefaults()
    defaultEmail()
    defaultIdentifier()
    defaultOptionalEmail()
    defaultFloatBits()
    defaultDoubleBits()
    Ledger.new(Long.MIN_VALUE)
    Tally.new(ULong.MAX_VALUE)
}
