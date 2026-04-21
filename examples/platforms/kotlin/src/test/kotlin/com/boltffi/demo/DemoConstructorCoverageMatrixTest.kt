package com.boltffi.demo

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class DemoConstructorCoverageMatrixTest {
    @Test
    fun constructorCoverageMatrixExercisesAllConstructorShapes() {
        ConstructorCoverageMatrix().use { matrix ->
            assertEquals("new", matrix.constructorVariant())
            assertEquals("default", matrix.summary())
            assertEquals(0u, matrix.payloadChecksum())
            assertEquals(0u, matrix.vectorCount())
        }

        ConstructorCoverageMatrix(7u, true, Priority.HIGH).use { matrix ->
            assertEquals("with_scalar_mix", matrix.constructorVariant())
            assertEquals("version=7;enabled=true;priority=high", matrix.summary())
            assertEquals(0u, matrix.payloadChecksum())
            assertEquals(0u, matrix.vectorCount())
        }

        ConstructorCoverageMatrix("bolt", byteArrayOf(1, 2, 3, 4)).use { matrix ->
            assertEquals("with_string_and_bytes", matrix.constructorVariant())
            assertEquals("label=bolt;bytes=4", matrix.summary())
            assertEquals(10u, matrix.payloadChecksum())
            assertEquals(4u, matrix.vectorCount())
        }

        ConstructorCoverageMatrix(Point(1.5, 2.5), Person("Alice", 31u)).use { matrix ->
            assertEquals("with_blittable_and_record", matrix.constructorVariant())
            assertEquals("origin=1.5:2.5;person=Alice#31", matrix.summary())
            assertEquals(0u, matrix.payloadChecksum())
            assertEquals(1u, matrix.vectorCount())
        }

        ConstructorCoverageMatrix(
            UserProfile("John", 29u, "john@example.com", 9.5),
            "cursor-7",
        ).use { matrix ->
            assertEquals("with_optional_profile_and_cursor", matrix.constructorVariant())
            assertEquals("profile=John#29#john@example.com#9.5;cursor=cursor-7", matrix.summary())
            assertEquals(0u, matrix.payloadChecksum())
            assertEquals(2u, matrix.vectorCount())
        }

        ConstructorCoverageMatrix(
            listOf("ffi", "swift"),
            listOf(Point(0.0, 0.0), Point(1.0, 1.0)),
            Polygon(listOf(Point(0.0, 0.0), Point(2.0, 0.0), Point(1.0, 1.0))),
        ).use { matrix ->
            assertEquals("with_vectors_and_polygon", matrix.constructorVariant())
            assertEquals("tags=ffi|swift;anchors=2;polygon=3", matrix.summary())
            assertEquals(0u, matrix.payloadChecksum())
            assertEquals(7u, matrix.vectorCount())
        }

        ConstructorCoverageMatrix(
            Team("Platform", listOf("Alice", "John")),
            Classroom(listOf(Person("Alice", 20u), Person("John", 21u))),
            Polygon(listOf(Point(0.0, 0.0), Point(1.0, 0.0), Point(1.0, 1.0))),
        ).use { matrix ->
            assertEquals("with_collection_records", matrix.constructorVariant())
            assertEquals("team=Platform;members=2;students=2;polygon=3", matrix.summary())
            assertEquals(0u, matrix.payloadChecksum())
            assertEquals(7u, matrix.vectorCount())
        }

        ConstructorCoverageMatrix(
            Filter.ByGroups(listOf(listOf("café", "🌍"), emptyList(), listOf("common"))),
            Message.Image("https://example.com/image.png", 640u, 480u),
            Task("ship", Priority.CRITICAL, false),
        ).use { matrix ->
            assertEquals("with_enum_mix", matrix.constructorVariant())
            assertEquals(
                "filter=groups:3;message=image:https://example.com/image.png#640x480;task=ship#critical",
                matrix.summary(),
            )
            assertEquals(0u, matrix.payloadChecksum())
            assertEquals(1u, matrix.vectorCount())
        }

        ConstructorCoverageMatrix(
            Person("Alice", 31u),
            Address("Main", "AMS", "1000"),
            UserProfile("John", 29u, "john@example.com", 9.5),
            SearchResult("route", 5u, "next-9", 7.5),
            byteArrayOf(4, 5, 6),
            Filter.ByRange(1.0, 3.0),
            listOf("alpha", "beta"),
        ).use { matrix ->
            assertEquals("with_everything", matrix.constructorVariant())
            assertEquals(
                "person=Alice#31;city=AMS;profile=profile=John#29#john@example.com#9.5;query=route;filter=range:1.0-3.0;tags=alpha|beta",
                matrix.summary(),
            )
            assertEquals(15u, matrix.payloadChecksum())
            assertEquals(10u, matrix.vectorCount())
            assertEquals(
                "profile=John#29#john@example.com#9.5;query=route;filter=range:1.0-3.0",
                matrix.summarizeBorrowedInputs(
                    UserProfile("John", 29u, "john@example.com", 9.5),
                    SearchResult("route", 5u, "next-9", 7.5),
                    Filter.ByRange(1.0, 3.0),
                ),
            )
        }

        ConstructorCoverageMatrix(
            byteArrayOf(7, 8),
            SearchResult("search", 4u, "cursor-4", null),
            Filter.ByName("ali"),
        ).use { matrix ->
            assertEquals("try_with_payload_and_search_result", matrix.constructorVariant())
            assertEquals("query=search;cursor=cursor-4;filter=name:ali", matrix.summary())
            assertEquals(15u, matrix.payloadChecksum())
            assertEquals(6u, matrix.vectorCount())
        }

        assertMessageContains(
            assertFailsWith<FfiException> {
                ConstructorCoverageMatrix(
                    byteArrayOf(),
                    SearchResult("search", 4u, null, null),
                    Filter.None,
                )
            },
            "payload must not be empty",
        )
    }
}
