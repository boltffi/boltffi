using System.Threading;
using System.Threading.Tasks;
using Demo;
using static Demo.Demo;

public static class DefaultArguments
{
    public static async Task Compile(CancellationToken cancellationToken)
    {
        Greet("ada");
        Greet("ada", shout: false, suffix: null, limit: null);
        FormatDefaults("ada", enabled: false, handler: null);
        using var server = new Server();
        using var startedServer = await Server.Start(8080, second: null, cancellationToken: cancellationToken);
        server.Port();
        using var counter = new DefaultCounter();
        using var namedCounter = DefaultCounter.FromText();
        using var withOffset = DefaultCounter.WithOffset(offset: 3);
        using var explicitOffset = DefaultCounter.WithOffset(5, 3);
        counter.Offset();
        DefaultCounter.Sum(right: 7);
        await counter.AsyncOffset(cancellationToken: cancellationToken);
        await AsyncDefault(cancellationToken: cancellationToken);
        DefaultMode.Quiet.Matches();
        await DefaultModeMethods.Load();
        new DefaultAmount().Offset();
        global::Demo.DefaultAmount.WithScaledValue();
        global::Demo.DefaultAmount.TryScaledValue();
        NamedAmount.WithValue();
        await NamedAmount.Load();
        DefaultAmount();
        Span();
        Throttle();
        DefaultOptionalEmail(null);

        NonTrailingDefaults(required: 1);
        NonTrailingDefaults(required: 1, limit: null, mode: DefaultMode.Loud);
        NonTrailingDefaults(-2, 3, -4, 8, 1, DefaultMode.Loud, 5, 6, 1);
        NativeBoundaries(required: 1);
        NativeBoundaries(lower: 2, required: 1);
        NativeBoundaries(upper: 3, required: 1);
        NativeBoundaries(2, 3, 1);
        OptionalClosure(value: 1);
        OptionalClosure(value: 1, callback: null);
        OptionalClosure(value: 1, callback: value => value * 2);

        using var defaults = new RuntimeDefaults();
        using var explicitDefaults = new RuntimeDefaults(new DefaultAmount(7));
        defaults.Amount();
        defaults.Amount(new DefaultAmount(7));
        using var startedDefaults = await RuntimeDefaults.Start(cancellationToken: cancellationToken);
        await defaults.AsyncAmount(cancellationToken: cancellationToken);
        RuntimeMode.Low.Amount();
        await RuntimeMode.Low.AsyncAmount(cancellationToken: cancellationToken);
        MixedDefaults(required: 1);
        MixedDefaults(amount: new DefaultAmount(7), required: 1);
        MixedDefaults(choice: new Choice.Value(2), limit: null, required: 1);
        MixedDefaults(new DefaultAmount(7), new Choice.Value(2), 8, 1);
        OptionalRecordDefaults();
        OptionalRecordDefaults(amount: new DefaultAmount(7));
        OptionalRecordDefaults(optionalAmount: new DefaultAmount(8));
        OptionalRecordDefaults(optionalAmount: null);
    }
}
