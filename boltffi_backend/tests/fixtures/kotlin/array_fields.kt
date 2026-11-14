@file:OptIn(kotlin.ExperimentalUnsignedTypes::class)

package com.boltffi.demo

fun blob(ratio: Double = 0.5, weight: Float? = 1.5f) = Blob(
    7u,
    byteArrayOf(1, 2, 3),
    uintArrayOf(4u, 5u),
    byteArrayOf(6),
    listOf(byteArrayOf(7), byteArrayOf(8, 9)),
    ratio,
    weight,
)

fun nested() = Nested(
    listOf(byteArrayOf(1), byteArrayOf(2, 3)),
    listOf(listOf(byteArrayOf(4)), listOf(byteArrayOf(5), byteArrayOf(6))),
    listOf(null, listOf(byteArrayOf(7))),
    mapOf("a" to byteArrayOf(8), "b" to byteArrayOf(9)),
    mapOf("c" to listOf(byteArrayOf(10))),
)

fun <T> checkEqual(left: T, right: T) {
    check(left == right)
    check(right == left)
    check(left.hashCode() == right.hashCode())
    check(setOf(left, right).size == 1)
}

fun arrayWrappers() = ArrayWrappers(
    Pair(byteArrayOf(1, 2), 3u),
    Triple(byteArrayOf(4), Double.NaN, true),
    BoltFFIResult.Ok(byteArrayOf(5)),
    Pair(byteArrayOf(6), 7u),
    BoltFFIResult.Err(byteArrayOf(8)),
    listOf(BoltFFIResult.Ok(byteArrayOf(9)), BoltFFIResult.Err(byteArrayOf(10))),
)

fun main() {
    check(blob() == blob())
    check(blob().hashCode() == blob().hashCode())
    check(blob() != blob().copy(payload = byteArrayOf(1, 2, 4)))
    check(blob() != blob().copy(samples = uintArrayOf(4u)))
    check(blob() != blob().copy(checksum = null))
    check(blob().copy(checksum = null) == blob().copy(checksum = null))
    check(blob() != blob().copy(chunks = listOf(byteArrayOf(7), byteArrayOf(8))))
    check(blob() != blob().copy(chunks = listOf(byteArrayOf(7))))
    check(blob(ratio = Double.NaN) == blob(ratio = Double.NaN))
    check(blob(ratio = 0.0) != blob(ratio = -0.0))
    check(blob(weight = null) == blob(weight = null))
    check(blob(weight = null) != blob())
    check(blob() != Label("blob") as Any)
    check(Blob.fromByteArray(blob().toByteArray()) == blob())
    check(setOf(blob(), blob()).size == 1)

    check(Frame.Data(byteArrayOf(1)) == Frame.Data(byteArrayOf(1)))
    check(Frame.Data(byteArrayOf(1)).hashCode() == Frame.Data(byteArrayOf(1)).hashCode())
    check(Frame.Data(byteArrayOf(1)) != Frame.Data(byteArrayOf(2)))
    check(Frame.Tagged(1u, longArrayOf(2)) == Frame.Tagged(1u, longArrayOf(2)))
    check(Frame.Tagged(1u, longArrayOf(2)) != Frame.Tagged(2u, longArrayOf(2)))
    check(Frame.fromByteArray(Frame.Data(byteArrayOf(1)).toByteArray()) == Frame.Data(byteArrayOf(1)))

    check(BlobError("failed", byteArrayOf(1)) == BlobError("failed", byteArrayOf(1)))
    check(BlobError("failed", byteArrayOf(1)) != BlobError("failed", byteArrayOf(2)))

    checkEqual(nested(), nested())
    checkEqual(nested().copy(maybeChunks = null), nested().copy(maybeChunks = null))
    checkEqual(nested().copy(maybeNamed = null), nested().copy(maybeNamed = null))
    check(nested() != nested().copy(maybeChunks = null))
    check(nested().copy(maybeChunks = null) != nested())
    check(nested() != nested().copy(maybeChunks = listOf(byteArrayOf(1), byteArrayOf(2, 4))))
    check(nested() != nested().copy(pages = listOf(listOf(byteArrayOf(4)), listOf(byteArrayOf(5), byteArrayOf(7)))))
    check(nested() != nested().copy(pages = listOf(listOf(byteArrayOf(4)), listOf(byteArrayOf(5)))))
    check(nested() != nested().copy(sparse = listOf(listOf(), listOf(byteArrayOf(7)))))
    check(nested() != nested().copy(sparse = listOf(null, listOf(byteArrayOf(8)))))
    check(nested() != nested().copy(named = mapOf("a" to byteArrayOf(8), "b" to byteArrayOf(0))))
    check(nested() != nested().copy(named = mapOf("a" to byteArrayOf(8), "c" to byteArrayOf(9))))
    check(nested() != nested().copy(named = mapOf("a" to byteArrayOf(8))))
    check(nested() != nested().copy(maybeNamed = null))
    check(nested() != nested().copy(maybeNamed = mapOf("c" to listOf(byteArrayOf(11)))))
    check(Nested.fromByteArray(nested().toByteArray()) == nested())

    val map = ByteKeyMap(linkedMapOf(byteArrayOf(1) to "first", byteArrayOf(2) to "second"))
    val reordered = ByteKeyMap(linkedMapOf(byteArrayOf(2) to "second", byteArrayOf(1) to "first"))
    checkEqual(map, reordered)
    check(map != ByteKeyMap(mapOf(byteArrayOf(1) to "changed", byteArrayOf(2) to "second")))
    check(map != ByteKeyMap(mapOf(byteArrayOf(1) to "first", byteArrayOf(3) to "second")))
    check(ByteKeyMap.fromByteArray(map.toByteArray()) == map)

    val repeated = ByteKeyMap(
        linkedMapOf(byteArrayOf(1) to "a", byteArrayOf(1) to "a", byteArrayOf(1) to "b"),
    )
    val repeatedReordered = ByteKeyMap(
        linkedMapOf(byteArrayOf(1) to "b", byteArrayOf(1) to "a", byteArrayOf(1) to "a"),
    )
    val repeatedDifferent = ByteKeyMap(
        linkedMapOf(byteArrayOf(1) to "a", byteArrayOf(1) to "b", byteArrayOf(1) to "b"),
    )
    checkEqual(repeated, repeatedReordered)
    check(repeated != repeatedDifferent)
    check(repeatedDifferent != repeated)

    val collisions = ByteKeyMap(
        linkedMapOf(byteArrayOf(0, 31) to "a", byteArrayOf(1, 0) to "b"),
    )
    checkEqual(
        collisions,
        ByteKeyMap(linkedMapOf(byteArrayOf(1, 0) to "b", byteArrayOf(0, 31) to "a")),
    )
    check(
        collisions != ByteKeyMap(
            linkedMapOf(byteArrayOf(0, 31) to "b", byteArrayOf(1, 0) to "a"),
        ),
    )

    val wrappers = arrayWrappers()
    checkEqual(wrappers, arrayWrappers())
    check(wrappers != wrappers.copy(pair = Pair(byteArrayOf(1, 3), 3u)))
    check(wrappers != wrappers.copy(pair = Pair(byteArrayOf(1, 2), 4u)))
    check(wrappers != wrappers.copy(triple = Triple(null, Double.NaN, true)))
    check(wrappers != wrappers.copy(triple = Triple(byteArrayOf(4), Double.NaN, false)))
    checkEqual(
        wrappers.copy(triple = Triple(null, Double.NaN, true)),
        wrappers.copy(triple = Triple(null, Double.NaN, true)),
    )
    check(
        wrappers.copy(triple = Triple(byteArrayOf(4), 0.0, true)) !=
            wrappers.copy(triple = Triple(byteArrayOf(4), -0.0, true)),
    )
    check(wrappers != wrappers.copy(outcome = BoltFFIResult.Ok(byteArrayOf(0))))
    check(wrappers != wrappers.copy(outcome = BoltFFIResult.Err(byteArrayOf(5))))
    checkEqual(
        wrappers.copy(outcome = BoltFFIResult.Err(byteArrayOf(5))),
        wrappers.copy(outcome = BoltFFIResult.Err(byteArrayOf(5))),
    )
    checkEqual(
        wrappers.copy(optionalPair = null, optionalOutcome = null),
        wrappers.copy(optionalPair = null, optionalOutcome = null),
    )
    check(wrappers != wrappers.copy(optionalPair = null))
    check(wrappers != wrappers.copy(optionalOutcome = null))
    check(
        wrappers != wrappers.copy(
            outcomes = listOf(BoltFFIResult.Ok(byteArrayOf(9)), BoltFFIResult.Err(byteArrayOf(11))),
        ),
    )
    checkEqual(ArrayWrappers.fromByteArray(wrappers.toByteArray()), wrappers)

    val wrappedMap = WrappedKeyMap(mapOf(Pair(byteArrayOf(1), 2u) to BoltFFIResult.Ok(byteArrayOf(3))))
    val wrappedMapCopy = WrappedKeyMap(mapOf(Pair(byteArrayOf(1), 2u) to BoltFFIResult.Ok(byteArrayOf(3))))
    checkEqual(wrappedMap, wrappedMapCopy)
    check(wrappedMap != WrappedKeyMap(mapOf(Pair(byteArrayOf(1), 2u) to BoltFFIResult.Err(byteArrayOf(3)))))

    checkEqual(
        Frame.Wrapped(Pair(byteArrayOf(1), 2u), BoltFFIResult.Ok(byteArrayOf(3))),
        Frame.Wrapped(Pair(byteArrayOf(1), 2u), BoltFFIResult.Ok(byteArrayOf(3))),
    )
}
