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

fun checkSame(left: Nested, right: Nested) {
    check(left == right)
    check(left.hashCode() == right.hashCode())
}

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

    checkSame(nested(), nested())
    checkSame(nested().copy(maybeChunks = null), nested().copy(maybeChunks = null))
    checkSame(nested().copy(maybeNamed = null), nested().copy(maybeNamed = null))
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
}
