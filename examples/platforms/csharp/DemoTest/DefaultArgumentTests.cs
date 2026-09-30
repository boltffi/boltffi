using System;
using System.Threading;
using Demo;
using static Demo.Demo;

namespace BoltFFI.Demo.Tests;

public static partial class DemoTest
{
    private static async System.Threading.Tasks.Task TestDefaultArguments()
    {
        DemoCase("case:primitives.default_arguments.should_apply_omitted_scalar_and_string_defaults");
        Require(RepeatGreeting("ada") == "hello ada, hello ada", "omitted greeting defaults");
        Require(RepeatGreeting("ada", shout: true) == "HELLO ADA, HELLO ADA", "named argument preserves other defaults");
        Require(RepeatGreeting("ada", "hi", 1, false) == "hi ada", "explicit greeting arguments");
        Require(RepeatGreeting("ada", "", 1, false) == " ada", "empty string stays empty");
        Require(RepeatGreeting("ada", times: 0) == "", "zero stays zero");

        DemoCase("case:primitives.default_arguments.should_apply_none_and_value_defaults_to_optionals");
        Require(DescribeLimit() == "none:7", "optional defaults");
        Require(DescribeLimit(label: "daily") == "daily:7", "supplied optional label");
        Require(DescribeLimit(limit: null) == "none:unlimited", "explicit null overrides present default");
        Require(DescribeLimit("", 0) == ":0", "empty and zero optionals stay present");

        DemoCase("case:primitives.default_arguments.should_default_an_optional_callback_to_none");
        ValueCallback doubler = new ValueCallbackImpl(value => value * 2);
        Require(ApplyOptionalCallback(21) == 21, "omitted callback");
        Require(ApplyOptionalCallback(21, callback: doubler) == 42, "supplied callback");
        Require(ApplyOptionalCallback(21, callback: null) == 21, "explicit null callback");
        Require(ApplyOptionalClosure(21) == 21, "omitted closure");
        Require(ApplyOptionalClosure(21, callback: value => value * 2) == 42, "supplied closure");
        Require(ApplyOptionalClosure(21, callback: null) == 21, "explicit null closure");

        DemoCase("case:primitives.default_arguments.defaulted_counter.should_apply_constructor_and_method_defaults");
        using var counter = new DefaultedCounter();
        using var explicitCounter = new DefaultedCounter(5);
        using var namedCounter = DefaultedCounter.FromText();
        using var namedOffset = DefaultedCounter.WithOffset(offset: 3);
        using var explicitOffset = DefaultedCounter.WithOffset(5, 3);
        Require(counter.Offset() == 11, "constructor and method defaults");
        Require(explicitCounter.Offset(3) == 8, "explicit constructor and method arguments");
        Require(namedCounter.Offset() == 41, "named constructor default");
        Require(namedOffset.Offset(0) == 23, "default before a required argument");
        Require(explicitOffset.Offset(0) == 8, "original positional order");
        var limits = DefaultedCounter.IntegerLimits();
        Require(limits.Lower == long.MinValue && limits.Upper == ulong.MaxValue, "integer boundary defaults");

        DemoCase("case:primitives.default_arguments.should_apply_float_and_enum_defaults");
        Require(ScaleDefault() == 0.75, "float and enum defaults");
        Require(ScaleDefault(2, 4, DefaultMode.Loud) == 16, "explicit float and enum values");
        Require(ScaleDefault(weight: null) == 0.5, "null weight stays null");
        Require(DefaultMode.Quiet.Matches(), "enum method default");
        Require(DefaultMode.Loud.Matches(DefaultMode.Loud), "explicit enum method argument");
        Require(ChooseDefault() == 0, "data enum default");
        Require(ChooseDefault(new DefaultChoice.Value(7)) == 7, "positional argument still selects the first parameter");
        Require(ChooseDefault(second: new DefaultChoice.Value(7)) == -7, "named argument omits the constructed default");
        Require(ChooseDefault(second: null) == 0, "explicit null keeps the constructed default");
        Require(DefaultFloatBits() == 0x80000000U, "negative zero float default");
        Require(DefaultDoubleBits() == 0x8000000000000000UL, "negative zero double default");

        DemoCase("case:primitives.default_arguments.should_apply_async_defaults");
        Require(await AsyncDefault() == 9, "async function default");
        Require(await AsyncDefault(12) == 12, "explicit async argument");
        Require(await counter.AsyncOffset() == 13, "async method default");
        Require(await counter.AsyncOffset(7) == 17, "explicit async method argument");
        using var started = await DefaultedCounter.Start();
        using var withCallback = await DefaultedCounter.Start(second: doubler);
        using var withNullCallbacks = await DefaultedCounter.Start(4, null, null);
        Require(started.Offset(0) == 30, "async constructor defaults");
        Require(withCallback.Offset(0) == 60, "second callback supplied by name");
        Require(withNullCallbacks.Offset(0) == 4, "explicit async constructor arguments");
        Require(await DefaultModeMethods.Load() == DefaultMode.Quiet, "async enum constructor default");
        Require(await DefaultModeMethods.Load(DefaultMode.Loud) == DefaultMode.Loud, "explicit async enum argument");
        try
        {
            await AsyncDefault(cancellationToken: new CancellationToken(true));
            throw new Exception("expected cancellation with an omitted default argument");
        }
        catch (OperationCanceledException) { }

        DemoCase("case:primitives.default_arguments.should_apply_record_defaults");
        Require(new DefaultAmount().Offset() == 5, "memberwise and method defaults");
        Require(new DefaultAmount(7).Offset(3) == 10, "explicit memberwise and method arguments");
        Require(DefaultAmount.WithScaledValue() == new DefaultAmount(4), "named record constructor default");
        Require(DefaultAmount.WithScaledValue(7) == new DefaultAmount(14), "explicit named record argument");
        Require(DefaultAmount.TryScaledValue() == new DefaultAmount(4), "optional record constructor default");
        Require(DefaultAmount.TryScaledValue(-1) is null, "optional record constructor returns none");
        Require(NamedAmount.WithValue() == new NamedAmount(5), "named amount default");
        Require(NamedAmount.WithValue(8) == new NamedAmount(8), "explicit named amount");
        Require(await NamedAmount.Load() == new NamedAmount(6), "async record constructor default");
        Require(await NamedAmount.Load(8) == new NamedAmount(8), "explicit async record argument");

        DemoCase("case:primitives.default_arguments.should_apply_custom_type_defaults");
        Require(DefaultTimeoutSeconds() == 1.5, "custom record default");
        Require(DefaultTimeoutSeconds(new TimeoutFfi(2.5)) == 2.5, "explicit custom record");
        Require(DefaultLimit() is null, "custom optional default");
        Require(DefaultLimit(12) == 12, "explicit custom optional");
        Require(DefaultEmail() == "mailto:ada@example.com", "custom string default");
        Require(DefaultEmail("mailto:grace@example.com") == "mailto:grace@example.com", "explicit custom string");
        Require(DefaultOptionalEmail() == "mailto:ada@example.com", "present custom optional default");
        Require(DefaultOptionalEmail(null) is null, "explicit null custom optional");
    }
}
