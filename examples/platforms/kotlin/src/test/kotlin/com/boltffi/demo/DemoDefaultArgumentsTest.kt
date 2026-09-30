package com.boltffi.demo

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout

class DemoDefaultArgumentsTest {
    @Test
    fun omittedScalarAndStringArgumentsTakeTheirDefaults() {
        demoCase("case:primitives.default_arguments.should_apply_omitted_scalar_and_string_defaults")
        assertEquals("hello ada, hello ada", repeatGreeting("ada"))
        assertEquals("HELLO ADA, HELLO ADA", repeatGreeting("ada", shout = true))
        assertEquals("hi ada", repeatGreeting("ada", greeting = "hi", times = 1u))
    }

    @Test
    fun optionalArgumentsDefaultToNoneOrTheirValue() {
        demoCase("case:primitives.default_arguments.should_apply_none_and_value_defaults_to_optionals")
        assertEquals("none:7", describeLimit())
        assertEquals("tag:7", describeLimit(label = "tag"))
        assertEquals("tag:unlimited", describeLimit("tag", null))
    }

    @Test
    fun anOptionalCallbackDefaultsToNone() {
        demoCase("case:primitives.default_arguments.should_default_an_optional_callback_to_none")
        assertEquals(21, applyOptionalCallback(21))
        val doubler = object : ValueCallback {
            override fun onValue(value: Int): Int = value * 2
        }
        assertEquals(42, applyOptionalCallback(21, callback = doubler))
    }

    @Test
    fun constructorsAndMethodsTakeDefaultsToo() {
        demoCase("case:primitives.default_arguments.defaulted_counter.should_apply_constructor_and_method_defaults")
        DefaultedCounter().use { counter -> assertEquals(11, counter.offset()) }
        DefaultedCounter(5).use { counter -> assertEquals(8, counter.offset(step = 3)) }
        DefaultedCounter(offset = 3).use { counter -> assertEquals(24, counter.offset()) }
        DefaultedCounter(start = 5, offset = 3).use { counter -> assertEquals(9, counter.offset()) }
        DefaultedCounter.fromText().use { counter -> assertEquals(41, counter.offset()) }
        DefaultedCounter.fromText(start = "12").use { counter -> assertEquals(13, counter.offset()) }
        assertEquals(IntegerLimits(Long.MIN_VALUE, ULong.MAX_VALUE), DefaultedCounter.integerLimits())
        assertEquals(IntegerLimits(0L, 1uL), DefaultedCounter.integerLimits(lower = 0, upper = 1uL))
    }

    @Test
    fun floatAndEnumArgumentsTakeTheirDefaults() {
        demoCase("case:primitives.default_arguments.should_apply_float_and_enum_defaults")
        assertEquals(0.75, scaleDefault())
        assertEquals(16.0, scaleDefault(ratio = 2.0f, weight = 4.0, mode = DefaultMode.LOUD))
        assertEquals(0.5, scaleDefault(weight = null))
        assertTrue(DefaultMode.QUIET.matches())
        assertFalse(DefaultMode.LOUD.matches())
        assertTrue(DefaultMode.LOUD.matches(mode = DefaultMode.LOUD))
        assertEquals(0x80000000u, defaultFloatBits())
        assertEquals(0x8000000000000000uL, defaultDoubleBits())
        assertEquals(0u, defaultFloatBits(value = 0.0f))
        assertEquals(0uL, defaultDoubleBits(value = 0.0))
    }

    @Test
    fun recordFieldsAndMethodsTakeTheirDefaults() {
        demoCase("case:primitives.default_arguments.should_apply_record_defaults")
        val amount = DefaultAmount()
        assertEquals(3, amount.value)
        assertEquals(5, amount.offset())
        assertEquals(7, amount.offset(step = 4))
        assertEquals(9, DefaultAmount(value = 9).value)
        assertEquals(4, DefaultAmount.withScaledValue().value)
        assertEquals(12, DefaultAmount.withScaledValue(value = 6).value)
        assertEquals(4, DefaultAmount.tryScaledValue()?.value)
        assertNull(DefaultAmount.tryScaledValue(value = -1))
        assertEquals(5, NamedAmount.withValue().value)
        assertEquals(9, NamedAmount.withValue(value = 9).value)
    }

    @Test
    fun customTypeArgumentsTakeTheirDefaults() {
        demoCase("case:primitives.default_arguments.should_apply_custom_type_defaults")
        assertEquals(1.5, defaultTimeoutSeconds())
        assertEquals(2.5, defaultTimeoutSeconds(timeout = TimeoutFFI(seconds = 2.5)))
        assertNull(defaultLimit())
        assertEquals(7u, defaultLimit(limit = 7u))
        assertEquals("mailto:ada@example.com", defaultEmail())
        assertEquals("mailto:grace@example.com", defaultEmail(email = "mailto:grace@example.com"))
        assertEquals("mailto:ada@example.com", defaultOptionalEmail())
        assertEquals("mailto:grace@example.com", defaultOptionalEmail(email = "mailto:grace@example.com"))
        assertNull(defaultOptionalEmail(email = null))
    }

    @Test
    fun asyncFunctionsMethodsAndFactoriesTakeTheirDefaults() = runBlocking {
        withTimeout(10_000) {
            demoCase("case:primitives.default_arguments.should_apply_async_defaults")
            assertEquals(9u, asyncDefault())
            assertEquals(4u, asyncDefault(value = 4u))
            assertEquals(6, NamedAmount.load().value)
            assertEquals(8, NamedAmount.load(value = 8).value)
            assertEquals(DefaultMode.QUIET, DefaultMode.load())
            assertEquals(DefaultMode.LOUD, DefaultMode.load(mode = DefaultMode.LOUD))
            DefaultedCounter().use { counter ->
                assertEquals(13, counter.asyncOffset())
                assertEquals(16, counter.asyncOffset(step = 6))
            }
            DefaultedCounter.start().use { counter -> assertEquals(31, counter.offset()) }
            val doubler = object : ValueCallback {
                override fun onValue(value: Int): Int = value * 2
            }
            DefaultedCounter.start(second = doubler).use { counter -> assertEquals(61, counter.offset()) }
        }
    }
}
