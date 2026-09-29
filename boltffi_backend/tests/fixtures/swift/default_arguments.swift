func checkDefaultArguments() async throws {
    _ = formatDefaults(name: "Ada")
    _ = formatDefaults(name: "Ada", enabled: false, weight: nil)
    _ = try await asyncDefault()

    let counter = DefaultCounter()
    _ = DefaultCounter(offset: 3)
    _ = counter.offset()
    _ = DefaultCounter.sum(right: 5)
    _ = try await counter.asyncOffset()

    let amount = DefaultAmount.new()
    _ = DefaultAmount()
    _ = amount.offset()
    _ = DefaultMode.quiet.matches()

    _ = span()
    _ = throttle()
    _ = defaultAmount()
}
