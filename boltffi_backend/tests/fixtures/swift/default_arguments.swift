func checkDefaultArguments() async throws {
    _ = greet(name: "Ada")
    _ = greet(name: "Ada", shout: false, suffix: "!")
    _ = Server().port()
    _ = try await Server.start(port: 8080).port(mapped: true)
    _ = formatDefaults(name: "Ada")
    _ = formatDefaults(name: "Ada", enabled: false, weight: nil)
    _ = try await asyncDefault()

    let counter = DefaultCounter()
    _ = DefaultCounter(offset: 3)
    _ = counter.offset()
    _ = DefaultCounter.sum(right: 5)
    _ = try await counter.asyncOffset()

    let amount = DefaultAmount()
    _ = DefaultAmount(value: 9)
    _ = amount.offset()
    _ = NamedAmount()
    _ = NamedAmount(withValue: 9)
    _ = DefaultMode.quiet.matches()
    _ = try await DefaultMode.load()
    _ = try await NamedAmount.load()

    _ = span()
    _ = throttle()
    _ = defaultAmount()
}
