package com.boltffi.demo

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

class DemoCallbacksAndAsyncTest {
    @Test
    fun unaryClosureExportsInvokeKotlinClosuresCorrectly() {
        var observedValue: Int? = null

        assertEquals(10, applyClosure(ClosureI32ToI32 { it * 2 }, 5))
        applyVoidClosure(ClosureI32 { observedValue = it }, 42)
        assertEquals(42, observedValue)
        assertEquals(99, applyNullaryClosure(ClosureToI32 { 99 }))
        assertEquals("HELLO", applyStringClosure(ClosureStringToString { it.uppercase() }, "hello"))
        assertEquals(false, applyBoolClosure(ClosureBoolToBool { !it }, true))
        assertDoubleEquals(9.0, applyF64Closure(ClosureF64ToF64 { it * it }, 3.0))
        assertContentEquals(intArrayOf(2, 4, 6), mapVecWithClosure(ClosureI32ToI32 { it * 2 }, intArrayOf(1, 2, 3)))
        assertContentEquals(
            intArrayOf(2, 4),
            filterVecWithClosure(ClosureI32ToBool { it % 2 == 0 }, intArrayOf(1, 2, 3, 4))
        )
        assertEquals(3L, applyOffsetClosure(ClosureISizeUSizeToISize { value, delta -> value + delta.toLong() }, -5L, 8uL))
        assertEquals(Status.PENDING, applyStatusClosure(ClosureStatusToStatus {
            if (it == Status.ACTIVE) Status.PENDING else Status.ACTIVE
        }, Status.ACTIVE))
        assertPointEquals(
            3.0,
            5.0,
            applyOptionalPointClosure(
                ClosureOptPointToOptPoint { point -> point?.let { Point(it.x + 2.0, it.y + 3.0) } },
                Point(1.0, 2.0)
            )!!
        )
        assertEquals(null, applyOptionalPointClosure(ClosureOptPointToOptPoint { it }, null))
        assertEquals(24, applyResultClosure(ClosureI32ToResultI32ErrMathError { value ->
            if (value < 0) {
                throw MathError.NegativeInput
            }
            value * 4
        }, 6))
        assertEquals(
            MathError.NegativeInput,
            assertFailsWith<MathError> {
                applyResultClosure(ClosureI32ToResultI32ErrMathError { throw MathError.NegativeInput }, -1)
            }
        )
    }

    @Test
    fun binaryAndPointClosureExportsInvokeKotlinClosuresCorrectly() {
        assertEquals(7, applyBinaryClosure(ClosureI32I32ToI32 { left, right -> left + right }, 3, 4))
        assertPointEquals(2.0, 3.0, applyPointClosure(ClosurePointToPoint { Point(it.x + 1.0, it.y + 1.0) }, Point(1.0, 2.0)))
    }

    @Test
    fun scalarSynchronousCallbackTraitsUseTheCorrectBridgeConversions() {
        val doubler = object : ValueCallback {
            override fun onValue(value: Int): Int = value * 2
        }
        val tripler = object : ValueCallback {
            override fun onValue(value: Int): Int = value * 3
        }
        val incrementer = makeIncrementingCallback(5)
        val pointTransformer = object : PointTransformer {
            override fun transform(point: Point): Point = Point(point.x + 10.0, point.y + 20.0)
        }
        val statusMapper = object : StatusMapper {
            override fun mapStatus(status: Status): Status = if (status == Status.PENDING) Status.ACTIVE else Status.INACTIVE
        }
        val flipper = makeStatusFlipper()
        val messageFormatter = object : MessageFormatter {
            override fun formatMessage(scope: String, message: String): String = "$scope::${message.uppercase()}"
        }
        val optionalMessageCallback = object : OptionalMessageCallback {
            override fun findMessage(key: Int): String? = key.takeIf { it > 0 }?.let { "message:$it" }
        }
        val resultMessageCallback = object : ResultMessageCallback {
            override fun renderMessage(key: Int): String {
                if (key < 0) {
                    throw MathError.NegativeInput
                }
                return "message:$key"
            }
        }
        val multiMethod = object : MultiMethodCallback {
            override fun methodA(x: Int): Int = x + 1
            override fun methodB(x: Int, y: Int): Int = x * y
            override fun methodC(): Int = 5
        }
        val optionCallback = object : OptionCallback {
            override fun findValue(key: Int): Int? = key.takeIf { it > 0 }?.times(10)
        }
        val resultCallback = object : ResultCallback {
            override fun compute(value: Int): Int {
                if (value < 0) {
                    throw MathError.NegativeInput
                }
                return value * 10
            }
        }
        val falliblePointTransformer = object : FalliblePointTransformer {
            override fun transformPoint(point: Point, status: Status): Point {
                if (status == Status.INACTIVE) {
                    throw MathError.NegativeInput
                }
                return Point(point.x + 100.0, point.y + 200.0)
            }
        }
        val offsetCallback = object : OffsetCallback {
            override fun offset(value: Long, delta: ULong): Long = value + delta.toLong()
        }
        val vecProcessor = object : VecProcessor {
            override fun process(values: IntArray): IntArray = values.map { it * it }.toIntArray()
        }

        assertEquals(8, invokeValueCallback(doubler, 4))
        assertEquals(14, invokeValueCallbackTwice(doubler, 3, 4))
        assertEquals(10, invokeBoxedValueCallback(doubler, 5))
        assertEquals(9, incrementer.onValue(4))
        assertEquals(9, invokeValueCallback(incrementer, 4))
        assertEquals(8, invokeOptionalValueCallback(doubler, 4))
        assertEquals(4, invokeOptionalValueCallback(null, 4))
        assertEquals(Status.ACTIVE, mapStatus(statusMapper, Status.PENDING))
        assertEquals(Status.INACTIVE, flipper.mapStatus(Status.ACTIVE))
        assertEquals(Status.PENDING, mapStatus(flipper, Status.INACTIVE))
        assertEquals("sync::BORROWED STRINGS", formatMessageWithCallback(messageFormatter, "sync", "borrowed strings"))
        assertEquals("boxed::BORROWED STRINGS", formatMessageWithBoxedCallback(messageFormatter, "boxed", "borrowed strings"))
        assertEquals("optional::BORROWED STRINGS", formatMessageWithOptionalCallback(messageFormatter, "optional", "borrowed strings"))
        assertEquals("fallback::message", formatMessageWithOptionalCallback(null, "fallback", "message"))
        val prefixer = makeMessagePrefixer("prefix")
        assertEquals("prefix::scope::message", prefixer.formatMessage("scope", "message"))
        assertEquals("prefix::sync::formatter", formatMessageWithCallback(prefixer, "sync", "formatter"))
        assertEquals("message:7", invokeOptionalMessageCallback(optionalMessageCallback, 7))
        assertNull(invokeOptionalMessageCallback(optionalMessageCallback, 0))
        assertEquals("message:8", invokeResultMessageCallback(resultMessageCallback, 8))
        assertEquals(MathError.NegativeInput, assertFailsWith<MathError> { invokeResultMessageCallback(resultMessageCallback, -1) })
        assertContentEquals(intArrayOf(1, 4, 9), processVec(vecProcessor, intArrayOf(1, 2, 3)))
        assertEquals(21, invokeMultiMethod(multiMethod, 3, 4))
        assertEquals(21, invokeMultiMethodBoxed(multiMethod, 3, 4))
        assertEquals(25, invokeTwoCallbacks(doubler, tripler, 5))
        assertEquals(70, invokeOptionCallback(optionCallback, 7))
        assertNull(invokeOptionCallback(optionCallback, 0))
        assertEquals(70, invokeResultCallback(resultCallback, 7))
        assertEquals(MathError.NegativeInput, assertFailsWith<MathError> { invokeResultCallback(resultCallback, -1) })
        assertEquals(3L, invokeOffsetCallback(offsetCallback, -5L, 8uL))
        assertEquals(14L, invokeBoxedOffsetCallback(offsetCallback, 10L, 4uL))
        assertPointEquals(102.0, 203.0, invokeFalliblePointTransformer(falliblePointTransformer, Point(2.0, 3.0), Status.ACTIVE))
        assertEquals(
            MathError.NegativeInput,
            assertFailsWith<MathError> {
                invokeFalliblePointTransformer(falliblePointTransformer, Point(2.0, 3.0), Status.INACTIVE)
            }
        )
    }

    @Test
    fun pointSynchronousCallbackTraitsUseTheCorrectBridgeConversions() {
        val pointTransformer = object : PointTransformer {
            override fun transform(point: Point): Point = Point(point.x + 10.0, point.y + 20.0)
        }

        assertPointEquals(11.0, 22.0, transformPoint(pointTransformer, Point(1.0, 2.0)))
        assertPointEquals(13.0, 24.0, transformPointBoxed(pointTransformer, Point(3.0, 4.0)))
    }

    @Test
    fun topLevelAsyncFunctionsRoundTripThroughKotlin() = runBlocking {
        withTimeout(10_000) {
            demoCase("case:async_fns.basic.add.should_return_sum")
            assertEquals(10, asyncAdd(3, 7))
            demoCase("case:async_fns.basic.echo.should_prefix_message")
            assertEquals("Echo: hello async", asyncEcho("hello async"))
            demoCase("case:async_fns.basic.double_all.should_double_i32_vector")
            assertContentEquals(intArrayOf(2, 4, 6), asyncDoubleAll(intArrayOf(1, 2, 3)))
            demoCase("case:async_fns.basic.find_positive.should_return_first_positive")
            assertEquals(5, asyncFindPositive(intArrayOf(-1, 0, 5, 3)))
            demoCase("case:async_fns.basic.find_positive.should_return_none_for_all_negative")
            assertNull(asyncFindPositive(intArrayOf(-1, -2, -3)))
            demoCase("case:async_fns.basic.concat.should_join_string_vector")
            assertEquals("a, b, c", asyncConcat(listOf("a", "b", "c")))
            demoCase("case:async_fns.basic.get_numbers.should_return_counting_sequence")
            assertContentEquals(intArrayOf(0, 1, 2, 3, 4), asyncGetNumbers(5))
        }
    }

    @Test
    fun asyncResultFunctionsRoundTripThroughKotlin() = runBlocking {
        withTimeout(10_000) {
            demoCase("case:results.async_results.safe_divide.should_return_quotient")
            assertEquals(5, asyncSafeDivide(10, 2))
            demoCase("case:results.async_results.safe_divide.should_reject_division_by_zero")
            assertTrue(assertFailsWith<MathError> { asyncSafeDivide(1, 0) } is MathError.DivisionByZero)
            demoCase("case:results.async_results.fallible_fetch.should_return_value_for_non_negative_key")
            assertEquals("value_7", asyncFallibleFetch(7))
            demoCase("case:results.async_results.fallible_fetch.should_reject_negative_key")
            assertMessageContains(assertFailsWith<FfiException> { asyncFallibleFetch(-1) }, "invalid key")
            demoCase("case:results.async_results.find_value.should_return_some_for_positive_key")
            assertEquals(40, asyncFindValue(4))
            demoCase("case:results.async_results.find_value.should_return_none_for_zero_key")
            assertNull(asyncFindValue(0))
            demoCase("case:results.async_results.find_value.should_reject_negative_key")
            assertMessageContains(assertFailsWith<FfiException> { asyncFindValue(-1) }, "invalid key")

            demoCase("case:async_fns.results.try_compute.should_return_doubled_value")
            assertEquals(14, tryComputeAsync(7))
            demoCase("case:async_fns.results.try_compute.should_return_invalid_input_for_zero")
            assertIs<ComputeError.InvalidInput>(assertFailsWith<ComputeError> { tryComputeAsync(0) })
            demoCase("case:async_fns.results.try_compute.should_return_overflow_for_negative_value")
            assertIs<ComputeError.Overflow>(assertFailsWith<ComputeError> { tryComputeAsync(-1) })
            demoCase("case:async_fns.results.fetch_data.should_return_scaled_positive_id")
            assertEquals(90, fetchData(9))
            demoCase("case:async_fns.results.fetch_data.should_reject_non_positive_id")
            assertMessageContains(assertFailsWith<FfiException> { fetchData(-1) }, "invalid id")
        }
    }

    @Test
    fun asyncCallbackTraitsRoundTripThroughKotlin() = runBlocking {
        withTimeout(10_000) {
            val asyncFetcher = object : AsyncFetcher {
                override suspend fun fetchValue(key: Int): Int = key * 100
                override suspend fun fetchString(input: String): String = input.uppercase()
                override suspend fun fetchJoinedMessage(scope: String, message: String): String =
                    "$scope::${message.uppercase()}"
            }
            val asyncPointTransformer = object : AsyncPointTransformer {
                override suspend fun transformPoint(point: Point): Point = Point(point.x + 50.0, point.y + 60.0)
            }
            val asyncOptionFetcher = object : AsyncOptionFetcher {
                override suspend fun find(key: Int): Long? = key.takeIf { it > 0 }?.toLong()?.times(1000L)
            }
            val asyncOptionalMessageFetcher = object : AsyncOptionalMessageFetcher {
                override suspend fun findMessage(key: Int): String? = key.takeIf { it > 0 }?.let { "async-message:$it" }
            }
            val asyncResultFormatter = object : AsyncResultFormatter {
                override suspend fun renderMessage(scope: String, message: String): String {
                    if (scope.isEmpty()) {
                        throw MathError.NegativeInput
                    }
                    return "$scope::${message.uppercase()}"
                }

                override suspend fun transformPoint(point: Point, status: Status): Point {
                    if (status == Status.INACTIVE) {
                        throw MathError.NegativeInput
                    }
                    return Point(point.x + 500.0, point.y + 600.0)
                }
            }

            assertEquals(500, fetchWithAsyncCallback(asyncFetcher, 5))
            assertEquals("BOLTFFI", fetchStringWithAsyncCallback(asyncFetcher, "boltffi"))
            assertEquals("async::BORROWED STRINGS", fetchJoinedMessageWithAsyncCallback(asyncFetcher, "async", "borrowed strings"))
            assertPointEquals(51.0, 62.0, transformPointWithAsyncCallback(asyncPointTransformer, Point(1.0, 2.0)))
            assertEquals(7_000L, invokeAsyncOptionFetcher(asyncOptionFetcher, 7))
            assertNull(invokeAsyncOptionFetcher(asyncOptionFetcher, 0))
            assertEquals("async-message:9", invokeAsyncOptionalMessageFetcher(asyncOptionalMessageFetcher, 9))
            assertNull(invokeAsyncOptionalMessageFetcher(asyncOptionalMessageFetcher, 0))
            assertEquals("async::RESULT", renderMessageWithAsyncResultCallback(asyncResultFormatter, "async", "result"))
            assertPointEquals(503.0, 604.0, transformPointWithAsyncResultCallback(asyncResultFormatter, Point(3.0, 4.0), Status.ACTIVE))
            assertEquals(
                MathError.NegativeInput,
                assertFailsWith<MathError> {
                    renderMessageWithAsyncResultCallback(asyncResultFormatter, "", "result")
                }
            )
            assertEquals(
                MathError.NegativeInput,
                assertFailsWith<MathError> {
                    transformPointWithAsyncResultCallback(asyncResultFormatter, Point(3.0, 4.0), Status.INACTIVE)
                }
            )
        }
    }
}
