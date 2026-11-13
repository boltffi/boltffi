package com.boltffi.demo

private suspend fun defaultArguments(value: Envelope, reply: Reply) {
    Envelope.new()
    Envelope.load()
    check(Envelope.defaultStep() == 3u)
    value.offset()
    value.asyncOffset()
    reply.replyOffset()
    reply.offset(-2)
    reply.asyncOffset()
}

private suspend fun shadowedArguments(value: Outer, envelope: Envelope) {
    value.echo(envelope)
    value.asyncEcho(envelope)
    value.echoOptional(envelope)
    value.echoMany(listOf(envelope))
}

fun main() {
    val ping = Ping(7u)
    check(ping is Envelope && ping is Reply)
    check(!Envelope.toByteArray(ping).contentEquals(Reply.toByteArray(ping)))
    check(Envelope.fromByteArray(Envelope.toByteArray(ping)) == ping)
    check(Reply.fromByteArray(Reply.toByteArray(ping)) == ping)

    listOf(Envelope.Unset, ping, Note("hello"), Envelope.Raw("world")).forEach { value ->
        check(Envelope.fromByteArray(Envelope.toByteArray(value)) == value)
    }
    check(Reply.fromByteArray(Reply.toByteArray(Reply.Ack)) == Reply.Ack)
}
