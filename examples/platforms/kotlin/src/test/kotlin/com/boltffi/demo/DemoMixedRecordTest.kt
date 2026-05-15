package com.boltffi.demo

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlin.test.Test
import kotlin.test.assertEquals

class DemoMixedRecordTest {
    private fun sampleMixedRecordParameters() = MixedRecordParameters(
        tags = listOf("alpha", "beta"),
        checkpoints = listOf(Point(1.0, 2.0), Point(3.0, 5.0)),
        fallbackAnchor = Point(-1.0, -2.0),
        maxRetries = 4u,
        previewOnly = true
    )

    private fun sampleMixedRecord() = MixedRecord(
        name = "outline",
        anchor = Point(10.0, 20.0),
        priority = Priority.CRITICAL,
        shape = Shape.Rectangle(3.0, 4.0),
        parameters = sampleMixedRecordParameters()
    )

    @Test
    fun mixedRecordFunctionsRoundTripThroughKotlin() {
        val record = sampleMixedRecord()

        demoCase("case:records.mixed.should_roundtrip_composed_record")
        assertEquals(record, echoMixedRecord(record))
        demoCase("case:records.mixed.should_make_from_composed_parts")
        assertEquals(
            record,
            makeMixedRecord(
                record.name,
                record.anchor,
                record.priority,
                record.shape,
                record.parameters
            )
        )
    }

    @Test
    fun asyncMixedRecordFunctionsRoundTripThroughKotlin() = runBlocking {
        withTimeout(10_000) {
            val record = sampleMixedRecord()

            demoCase("case:async_fns.mixed_record.echo.should_roundtrip_record")
            assertEquals(record, asyncEchoMixedRecord(record))
            demoCase("case:async_fns.mixed_record.make.should_construct_record")
            assertEquals(
                record,
                asyncMakeMixedRecord(
                    record.name,
                    record.anchor,
                    record.priority,
                    record.shape,
                    record.parameters
                )
            )
        }
    }

    @Test
    fun mixedRecordServiceSyncAndAsyncMethodsRoundTripThroughKotlin() = runBlocking {
        withTimeout(10_000) {
            MixedRecordService("records").use { service ->
                val record = sampleMixedRecord()

                assertEquals("records", service.getLabel())
                assertEquals(0u, service.storedCount())
                assertEquals(record, service.echoRecord(record))
                assertEquals(
                    record,
                    service.storeRecordParts(
                        record.name,
                        record.anchor,
                        record.priority,
                        record.shape,
                        record.parameters
                    )
                )
                assertEquals(1u, service.storedCount())
                assertEquals(record, service.asyncEchoRecord(record))
                assertEquals(
                    record,
                    service.asyncStoreRecordParts(
                        record.name,
                        record.anchor,
                        record.priority,
                        record.shape,
                        record.parameters
                    )
                )
                assertEquals(2u, service.storedCount())
            }
        }
    }
}
