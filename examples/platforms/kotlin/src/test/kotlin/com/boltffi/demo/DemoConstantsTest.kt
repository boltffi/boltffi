package com.boltffi.demo

import kotlin.test.Test
import kotlin.test.assertEquals

class DemoConstantsTest {
    @Test
    fun associatedConstantsStayOnTheirOwnerTypes() {
        demoCase("case:constants.associated.should_expose_values_on_exported_types")
        assertEquals(DemoMode.SAFE, DemoMode.PREFERRED)
        assertEquals(DemoState.Idle, DemoState.INITIAL)
        assertEquals(Point(0.0, 0.0), Point.ZERO)
        assertEquals(2u, MathUtils.DEFAULT_PRECISION)
    }
}
