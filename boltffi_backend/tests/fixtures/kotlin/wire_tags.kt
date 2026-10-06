package com.boltffi.demo

fun main() {
    val writer = WireWriter(16)
    writer.writeOptionalU8(null)
    writer.writeOptionalU8(255u)
    val bytes = writer.toByteArray()
    check(bytes.contentEquals(byteArrayOf(0, 1, -1)))
    val reader = WireReader(bytes)
    check(reader.readOptionalU8() == null)
    check(reader.readOptionalU8() == UByte.MAX_VALUE)
    check(runCatching { WireReader(byteArrayOf(2)).readOptionalU8() }.exceptionOrNull() is IllegalArgumentException)

    val ok = Response(BoltFFIResult.Ok(42))
    check(ok.toByteArray().contentEquals(byteArrayOf(0, 42, 0, 0, 0)))
    check(Response.fromByteArray(ok.toByteArray()) == ok)
    val error = Response(BoltFFIResult.Err("bad"))
    check(error.toByteArray().contentEquals(byteArrayOf(1, 3, 0, 0, 0, 98, 97, 100)))
    check(Response.fromByteArray(error.toByteArray()) == error)
    check(runCatching { Response.fromByteArray(byteArrayOf(2)) }.exceptionOrNull() is IllegalArgumentException)

    val label = Shape.Label("tag")
    check(Shape.fromByteArray(label.toByteArray()) == label)

    val defaults = UnsignedDefaults()
    check(defaults.byte == UByte.MAX_VALUE)
    check(defaults.short == UShort.MAX_VALUE)
    check(defaults.word == UInt.MAX_VALUE)
    check(defaults.wide == ULong.MAX_VALUE)
    check(defaults.size == 1uL)
    check(UnsignedDefaults.fromByteArray(defaults.toByteArray()) == defaults)
}
