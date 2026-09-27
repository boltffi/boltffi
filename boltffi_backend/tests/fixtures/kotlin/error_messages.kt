package com.boltffi.demo

fun main() {
    listOf("", "service failed", "東京\u0000🦀").forEach { message ->
        val error = ServiceError.Failed(message)
        val throwable: Throwable = error
        check(throwable.message == message)
        check(throwable.localizedMessage == message)
        check(ServiceError.fromByteArray(error.toByteArray()) == error)
        check(error.copy().message == message)
        check(error.component1() == message)

        val detailed = ServiceError.Detailed(42u, message)
        check((detailed as Throwable).message == message)
        check(ServiceError.fromByteArray(detailed.toByteArray()) == detailed)

        val record = RecordError(message)
        check((record as Throwable).message == message)
        check(RecordError.fromByteArray(record.toByteArray()) == record)

        val shadowed = ShadowedError.String(message)
        check((shadowed as Throwable).message == message)
        check(ShadowedError.fromByteArray(shadowed.toByteArray()) == shadowed)
    }

    listOf(null, "", "optional failure 東京\u0000🦀").forEach { message ->
        val error = ServiceError.Optional(message)
        check((error as Throwable).message == message)
        check(ServiceError.fromByteArray(error.toByteArray()) == error)

        val record = OptionalRecordError(message)
        check((record as Throwable).message == message)
        check(OptionalRecordError.fromByteArray(record.toByteArray()) == record)

        val shadowed = ShadowedError.Optional(message)
        check((shadowed as Throwable).message == message)
        check(ShadowedError.fromByteArray(shadowed.toByteArray()) == shadowed)
    }

    val plain = Message.Plain("payload", "reason")
    check(plain.message == "payload" && plain.cause == "reason")
    check(Message.fromByteArray(plain.toByteArray()) == plain)
    val numeric = Message.Numeric(42u)
    check(Message.fromByteArray(numeric.toByteArray()) == numeric)
    val record = MessageRecord("payload", "reason")
    check(record.message == "payload" && record.cause == "reason")
    check(MessageRecord.fromByteArray(record.toByteArray()) == record)
    check(ServiceError.Unknown.message == null)
    check(ServiceError.Text("payload").message == null)
}
