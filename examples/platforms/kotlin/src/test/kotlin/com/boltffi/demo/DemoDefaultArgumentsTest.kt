package com.boltffi.demo

import kotlin.test.Test
import kotlin.test.assertEquals

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
    }
}
