package com.boltffi.demo

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class ErrorMessagesTest {
    @Test
    fun errorVariantExposesMessageThroughThrowable() {
        demoCase("case:results.error_enums.message.should_preserve_text")
        listOf("", "service failed 東京\u0000🦀").forEach { message ->
            val error = assertFailsWith<ServiceError.Failed> { failWithMessage(message) }
            val throwable: Throwable = error

            assertEquals(message, throwable.message)
            assertEquals(message, throwable.localizedMessage)
            assertEquals(error, ServiceError.fromByteArray(error.toByteArray()))
        }
    }

    @Test
    fun errorVariantPreservesNullableMessage() {
        demoCase("case:results.error_enums.message.should_preserve_optional_text")
        listOf(null, "", "optional failure 東京\u0000🦀").forEach { message ->
            val error = assertFailsWith<ServiceError.Optional> { failWithOptionalMessage(message) }
            val throwable: Throwable = error

            assertEquals(message, throwable.message)
            assertEquals(error, ServiceError.fromByteArray(error.toByteArray()))
        }
    }
}
