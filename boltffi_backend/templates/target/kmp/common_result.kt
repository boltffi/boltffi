class FfiException(val code: kotlin.Int, message: kotlin.String) : kotlin.Exception(message)

sealed class BoltFFIResult<out T, out E> {
    data class Ok<T>(val value: T) : BoltFFIResult<T, kotlin.Nothing>()
    data class Err<E>(val error: E) : BoltFFIResult<kotlin.Nothing, E>()

    val isSuccess: kotlin.Boolean get() = this is Ok
    val isFailure: kotlin.Boolean get() = this is Err

    fun getOrThrow(): T = when (this) {
        is Ok -> value
        is Err -> throw when (error) {
            is kotlin.Throwable -> error
            else -> FfiException(-1, error.toString())
        }
    }

    fun getOrNull(): T? = when (this) {
        is Ok -> value
        is Err -> null
    }

    fun exceptionOrNull(): kotlin.Throwable? = when (this) {
        is Ok -> null
        is Err -> when (error) {
            is kotlin.Throwable -> error
            else -> FfiException(-1, error.toString())
        }
    }

    inline fun <R> fold(onSuccess: (T) -> R, onFailure: (E) -> R): R = when (this) {
        is Ok -> onSuccess(value)
        is Err -> onFailure(error)
    }
}
