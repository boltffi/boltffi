private fun <T, E> WireReader.readResult(
    readOk: (WireReader) -> T,
    readErr: (WireReader) -> E,
): BoltFFIResult<T, E> {
    return when (readU8().toInt()) {
        0 -> BoltFFIResult.Ok(readOk(this))
        1 -> BoltFFIResult.Err(readErr(this))
        else -> throw IllegalArgumentException("invalid result wire tag")
    }
}

private fun <T, E> WireWriter.writeResult(
    value: BoltFFIResult<T, E>,
    writeOk: (WireWriter, T) -> Unit,
    writeErr: (WireWriter, E) -> Unit,
) {
    when (value) {
        is BoltFFIResult.Ok -> {
            writeU8(0u)
            writeOk(this, value.value)
        }
        is BoltFFIResult.Err -> {
            writeU8(1u)
            writeErr(this, value.error)
        }
    }
}

sealed class BoltFFIResult<out T, out E> {
    data class Ok<T>(val value: T) : BoltFFIResult<T, Nothing>()
    data class Err<E>(val error: E) : BoltFFIResult<Nothing, E>()
}

private inline fun <T, E, R> BoltFFIResult<T, E>.fold(ok: (T) -> R, err: (E) -> R): R =
    when (this) {
        is BoltFFIResult.Ok -> ok(value)
        is BoltFFIResult.Err -> err(error)
    }

private inline fun <T, E> BoltFFIResult<T, E>.wireSize(
    okSize: (T) -> Int,
    errSize: (E) -> Int,
): Int =
    1 + when (this) {
        is BoltFFIResult.Ok -> okSize(value)
        is BoltFFIResult.Err -> errSize(error)
    }
