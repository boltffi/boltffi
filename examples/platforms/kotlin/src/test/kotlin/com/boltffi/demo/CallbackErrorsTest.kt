package com.boltffi.demo

import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class CallbackErrorsTest {
    private fun checkMode(mode: Int) {
        when (mode) {
            1 -> throw MathError.NegativeInput
            2 -> error("unchecked callback failure 東京\u0000🦀")
        }
    }

    private val worker = object : FallibleWorker {
        override fun run(mode: Int) = checkMode(mode)
        override fun value(mode: Int): Int {
            checkMode(mode)
            return 42
        }
    }

    private val asyncWorker = object : AsyncFallibleWorker {
        override suspend fun run(mode: Int) = checkMode(mode)
        override suspend fun value(mode: Int): Int {
            checkMode(mode)
            return 42
        }
    }

    @Test
    fun callbackErrorsReachRust() = runBlocking {
        demoCase("case:callbacks.errors.unit.should_report_success")
        assertEquals(Unit, invokeUnitWorker(worker, 0))
        demoCase("case:callbacks.errors.unit.should_report_declared_error")
        assertEquals(MathError.NegativeInput, assertFailsWith<MathError> { invokeUnitWorker(worker, 1) })
        demoCase("case:callbacks.errors.unit.should_report_unexpected_error")
        assertEquals(MathError.Overflow, assertFailsWith<MathError> { invokeUnitWorker(worker, 2) })
        demoCase("case:callbacks.errors.value.should_report_success")
        assertEquals(42, invokeValueWorker(worker, 0))
        demoCase("case:callbacks.errors.value.should_report_declared_error")
        assertEquals(MathError.NegativeInput, assertFailsWith<MathError> { invokeValueWorker(worker, 1) })
        demoCase("case:callbacks.errors.value.should_report_unexpected_error")
        assertEquals(MathError.Overflow, assertFailsWith<MathError> { invokeValueWorker(worker, 2) })
        demoCase("case:callbacks.errors.async_unit.should_report_success")
        assertEquals(Unit, invokeAsyncUnitWorker(asyncWorker, 0))
        demoCase("case:callbacks.errors.async_unit.should_report_declared_error")
        assertEquals(MathError.NegativeInput, assertFailsWith<MathError> { invokeAsyncUnitWorker(asyncWorker, 1) })
        demoCase("case:callbacks.errors.async_unit.should_report_unexpected_error")
        assertEquals(MathError.Overflow, assertFailsWith<MathError> { invokeAsyncUnitWorker(asyncWorker, 2) })
        demoCase("case:callbacks.errors.async_value.should_report_success")
        assertEquals(42, invokeAsyncValueWorker(asyncWorker, 0))
        demoCase("case:callbacks.errors.async_value.should_report_declared_error")
        assertEquals(MathError.NegativeInput, assertFailsWith<MathError> { invokeAsyncValueWorker(asyncWorker, 1) })
        demoCase("case:callbacks.errors.async_value.should_report_unexpected_error")
        assertEquals(MathError.Overflow, assertFailsWith<MathError> { invokeAsyncValueWorker(asyncWorker, 2) })
        assertEquals(MathError.Overflow, assertFailsWith<MathError> {
            applyResultClosure(ClosureI32ToResultI32ErrMathError { error("unexpected closure error") }, 0)
        })
    }
}
