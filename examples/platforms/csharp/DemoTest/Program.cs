using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Demo;
using static Demo.Demo;

namespace BoltFFI.Demo.Tests;

public static class DemoTest
{
    private static string currentDemoCase;

    public static async System.Threading.Tasks.Task<int> Main()
    {
        try
        {
            Console.WriteLine("Testing C# bindings...\n");
            TestBool();
            TestI8();
            TestU8();
            TestI16();
            TestU16();
            TestI32();
            TestU32();
            TestI64();
            TestU64();
            TestF32();
            TestF64();
            TestUsize();
            TestIsize();
            TestStrings();
            TestCustomTypes();
            TestBuiltins();
            TestBlittableRecords();
            TestRecordsWithStrings();
            TestRecordsWithDefaults();
            TestNestedRecords();
            TestCStyleEnums();
            TestDataEnums();
            TestRecordsWithEnumFields();
            TestPrimitiveVecs();
            TestBytes();
            TestStringAndNestedVecs();
            TestBlittableRecordVecs();
            TestEnumVecs();
            TestVecFields();
            TestOptions();
            TestOptionsInRecords();
            TestOptionsWithVec();
            TestClasses();
            TestResultFunctions();
            TestResultClassMethods();
            TestResultEnumErrors();
            await TestAsyncFunctions();
            await TestAsyncResults();
            await TestAsyncClassMethods();
            await TestAsyncCancellation();
            TestCallbackTraits();
            TestClosures();
            await TestAsyncCallbackTraits();
            await TestStreams();
            Console.WriteLine("All tests passed!");
            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"FAIL: {DescribeFailure(ex)}");
            return 1;
        }
    }

    private static void TestBool()
    {
        Console.WriteLine("Testing bool...");
        Require(EchoBool(true), "case:primitives.scalars.bool.should_roundtrip_true echoBool(true)");
        Require(!EchoBool(false), "echoBool(false)");
        Require(!NegateBool(true), "negateBool(true)");
        Require(NegateBool(false), "case:primitives.scalars.bool.should_negate_false_to_true negateBool(false)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestI8()
    {
        Console.WriteLine("Testing i8...");
        Require(EchoI8(42) == 42, "echoI8(42)");
        Require(EchoI8(-128) == -128, "case:primitives.scalars.i8.should_roundtrip_negative_value echoI8(min)");
        Require(EchoI8(127) == 127, "echoI8(max)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestU8()
    {
        Console.WriteLine("Testing u8...");
        Require(EchoU8(0) == 0, "echoU8(0)");
        Require(EchoU8(255) == 255, "case:primitives.scalars.u8.should_roundtrip_max_value echoU8(max)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestI16()
    {
        Console.WriteLine("Testing i16...");
        Require(EchoI16(-32768) == -32768, "case:primitives.scalars.i16.should_roundtrip_negative_value echoI16(min)");
        Require(EchoI16(32767) == 32767, "echoI16(max)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestU16()
    {
        Console.WriteLine("Testing u16...");
        Require(EchoU16(0) == 0, "echoU16(0)");
        Require(EchoU16(65535) == 65535, "case:primitives.scalars.u16.should_roundtrip_large_value echoU16(max)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestI32()
    {
        Console.WriteLine("Testing i32...");
        Require(EchoI32(42) == 42, "echoI32(42)");
        Require(EchoI32(-100) == -100, "case:primitives.scalars.i32.should_roundtrip_negative_value echoI32(-100)");
        Require(AddI32(10, 20) == 30, "case:primitives.scalars.i32.should_add_two_values addI32(10, 20)");
        Require(Add(7, 9) == 16, "case:primitives.scalars.i32.should_add_with_benchmark_alias add(7, 9)");
        DemoCase("case:primitives.scalars.noop.should_cross_without_values");
        Noop();
        Console.WriteLine("  PASS\n");
    }

    private static void TestU32()
    {
        Console.WriteLine("Testing u32...");
        Require(EchoU32(0u) == 0u, "echoU32(0)");
        Require(EchoU32(uint.MaxValue) == uint.MaxValue, "case:primitives.scalars.u32.should_roundtrip_large_value echoU32(max)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestI64()
    {
        Console.WriteLine("Testing i64...");
        Require(EchoI64(9999999999L) == 9999999999L, "echoI64(large)");
        Require(EchoI64(-9999999999L) == -9999999999L, "case:primitives.scalars.i64.should_roundtrip_large_negative_value echoI64(negative large)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestU64()
    {
        Console.WriteLine("Testing u64...");
        Require(EchoU64(0UL) == 0UL, "echoU64(0)");
        Require(EchoU64(ulong.MaxValue) == ulong.MaxValue, "case:primitives.scalars.u64.should_roundtrip_large_value echoU64(max)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestF32()
    {
        Console.WriteLine("Testing f32...");
        Require(Math.Abs(EchoF32(3.14f) - 3.14f) < 0.001f, "case:primitives.scalars.f32.should_roundtrip_value_with_tolerance echoF32(3.14)");
        Require(Math.Abs(AddF32(1.5f, 2.5f) - 4.0f) < 0.001f, "case:primitives.scalars.f32.should_add_two_values_with_tolerance addF32(1.5, 2.5)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestF64()
    {
        Console.WriteLine("Testing f64...");
        Require(Math.Abs(EchoF64(3.14159265359) - 3.14159265359) < 0.0000001, "case:primitives.scalars.f64.should_roundtrip_pi_with_tolerance echoF64(pi)");
        Require(Math.Abs(AddF64(1.5, 2.5) - 4.0) < 0.0000001, "case:primitives.scalars.f64.should_add_two_values_with_tolerance addF64(1.5, 2.5)");
        Require(Math.Abs(Multiply(1.5, 4.0) - 6.0) < 0.0000001, "case:primitives.scalars.f64.should_multiply_two_values multiply(1.5, 4.0)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestUsize()
    {
        Console.WriteLine("Testing usize...");
        Require(EchoUsize((nuint)42) == (nuint)42, "case:primitives.scalars.usize.should_roundtrip_value echoUsize(42)");
        Require(EchoUsize((nuint)0) == (nuint)0, "echoUsize(0)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestIsize()
    {
        Console.WriteLine("Testing isize...");
        Require(EchoIsize((nint)42) == (nint)42, "echoIsize(42)");
        Require(EchoIsize((nint)(-100)) == (nint)(-100), "case:primitives.scalars.isize.should_roundtrip_negative_value echoIsize(-100)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestStrings()
    {
        Console.WriteLine("Testing strings...");
        Require(EchoString("hello") == "hello", "echoString(hello)");
        Require(EchoString("") == "", "case:primitives.strings.string.should_roundtrip_empty echoString(empty)");
        Require(EchoString("café") == "café", "echoString(unicode)");
        Require(EchoString("日本語") == "日本語", "echoString(cjk)");
        Require(EchoString("hello 🌍 world") == "hello 🌍 world", "case:primitives.strings.string.should_roundtrip_emoji echoString(emoji)");

        Require(ConcatStrings("foo", "bar") == "foobar", "case:primitives.strings.string.should_concatenate_values concatStrings(foo, bar)");
        Require(ConcatStrings("", "bar") == "bar", "concatStrings(empty, bar)");
        Require(ConcatStrings("foo", "") == "foo", "concatStrings(foo, empty)");
        Require(ConcatStrings("🎉", "🎊") == "🎉🎊", "concatStrings(emoji)");

        Require(StringLength("hello") == 5u, "stringLength(hello)");
        Require(StringLength("") == 0u, "stringLength(empty)");
        Require(StringLength("café") == 5u, "case:primitives.strings.string.should_report_utf8_byte_length stringLength(utf8 bytes)");
        Require(StringLength("🌍") == 4u, "stringLength(emoji 4 bytes)");

        Require(StringIsEmpty(""), "case:primitives.strings.string.should_detect_empty stringIsEmpty(empty)");
        Require(!StringIsEmpty("x"), "stringIsEmpty(nonempty)");

        Require(RepeatString("ab", 3u) == "ababab", "case:primitives.strings.string.should_repeat_value repeatString(ab, 3)");
        Require(RepeatString("x", 0u) == "", "repeatString(x, 0)");
        Require(RepeatString("🌟", 2u) == "🌟🌟", "repeatString(emoji, 2)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestCustomTypes()
    {
        Console.WriteLine("Testing custom types (Email, UtcDateTime, Event)...");

        string email = "café@example.com";
        DemoCase("case:custom_types.email.should_roundtrip_value");
        Require(EchoEmail(email) == email, "EchoEmail roundtrip");
        DemoCase("case:custom_types.email.should_extract_domain");
        Require(EmailDomain(email) == "example.com", "EmailDomain");

        long ts = 1_710_000_000_000L;
        DemoCase("case:custom_types.datetime.should_roundtrip_millis");
        Require(EchoDatetime(ts) == ts, "EchoDatetime");
        DemoCase("case:custom_types.datetime.should_convert_to_millis");
        Require(DatetimeToMillis(ts) == ts, "DatetimeToMillis");

        DemoCase("case:custom_types.datetime.should_format_rfc3339_timestamp");
        Require(FormatTimestamp(ts).StartsWith("2024-03-"), "FormatTimestamp");

        Event evt = new Event("launch", ts);
        DemoCase("case:custom_types.event.should_roundtrip_datetime_field");
        Event echoed = EchoEvent(evt);
        Require(echoed.Name == "launch", "EchoEvent.Name");
        Require(echoed.Timestamp == ts, "EchoEvent.Timestamp");
        DemoCase("case:custom_types.event.should_extract_timestamp_millis");
        Require(EventTimestamp(evt) == ts, "EventTimestamp");
        DemoCase("case:custom_types.event.should_expose_datetime_field");
        Require(evt.Timestamp == ts, "Event.Timestamp datetime field");

        string[] emails = new[] { "café@example.com", "user@example.org" };
        DemoCase("case:custom_types.vectors.emails.should_roundtrip_values");
        string[] echoedEmails = EchoEmails(emails);
        Require(echoedEmails.Length == 2, "EchoEmails length");
        Require(echoedEmails[0] == "café@example.com", "EchoEmails[0] roundtrip (utf-8)");
        Require(echoedEmails[1] == "user@example.org", "EchoEmails[1] roundtrip");

        long[] dts = new[] { 1_710_000_000_000L, 1_710_000_001_000L, 1_710_000_002_000L };
        DemoCase("case:custom_types.vectors.datetimes.should_roundtrip_millis_values");
        long[] echoedDts = EchoDatetimes(dts);
        Require(echoedDts.Length == 3, "EchoDatetimes length");
        Require(echoedDts[0] == dts[0] && echoedDts[1] == dts[1] && echoedDts[2] == dts[2],
            "EchoDatetimes roundtrip (blittable)");

        Console.WriteLine("  PASS\n");
    }

    private static void TestBuiltins()
    {
        Console.WriteLine("Testing builtins (Duration, SystemTime, UUID, URL)...");

        // TimeSpan has 100ns ticks; we pick a sub-second value that's a
        // multiple of 100ns so the wire roundtrip is lossless.
        TimeSpan duration = new TimeSpan(12L * TimeSpan.TicksPerSecond + 3_450_000L);
        DemoCase("case:builtins.duration.should_roundtrip_value");
        Require(EchoDuration(duration) == duration, "EchoDuration roundtrip");

        DemoCase("case:builtins.duration.should_construct_from_parts");
        TimeSpan made = MakeDuration(7UL, 89_000_000U);
        Require(made == TimeSpan.FromSeconds(7) + TimeSpan.FromMilliseconds(89), "MakeDuration");

        DemoCase("case:builtins.duration.should_report_milliseconds");
        Require(DurationAsMillis(TimeSpan.FromMilliseconds(1234)) == 1234UL, "DurationAsMillis");

        DateTime instant = DateTime.UnixEpoch.AddMilliseconds(1_710_000_000_123L);
        DemoCase("case:builtins.system_time.should_roundtrip_value");
        Require(EchoSystemTime(instant) == instant, "EchoSystemTime roundtrip");

        DemoCase("case:builtins.system_time.should_roundtrip_pre_epoch_value");
        DateTime preEpochInstant = DateTime.UnixEpoch.AddSeconds(-1).AddMilliseconds(500);
        Require(EchoSystemTime(preEpochInstant) == preEpochInstant, "EchoSystemTime pre-epoch roundtrip");

        DemoCase("case:builtins.system_time.should_convert_to_epoch_milliseconds");
        Require(SystemTimeToMillis(instant) == 1_710_000_000_123UL, "SystemTimeToMillis");

        DemoCase("case:builtins.system_time.should_construct_from_epoch_milliseconds");
        Require(MillisToSystemTime(1_710_000_000_123UL) == instant, "MillisToSystemTime");

        Guid uuid = Guid.Parse("550e8400-e29b-41d4-a716-446655440000");
        DemoCase("case:builtins.uuid.should_roundtrip_value");
        Require(EchoUuid(uuid) == uuid, "EchoUuid roundtrip");

        DemoCase("case:builtins.uuid.should_format_canonical_string");
        Require(UuidToString(uuid) == "550e8400-e29b-41d4-a716-446655440000", "UuidToString");

        Uri url = new Uri("https://example.com/path?q=boltffi");
        DemoCase("case:builtins.url.should_roundtrip_value");
        Require(EchoUrl(url) == url, "EchoUrl roundtrip");

        DemoCase("case:builtins.url.should_format_string");
        Require(UrlToString(url) == "https://example.com/path?q=boltffi", "UrlToString");

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Blittable records (Point, Color) cross the ABI as direct struct
    /// values via [StructLayout(Sequential)] — no WireWriter / WireReader
    /// involvement. These tests exercise the zero-copy fast path.
    /// </summary>
    private static void TestBlittableRecords()
    {
        Console.WriteLine("Testing blittable records (Point, Color)...");

        DemoCase("case:records.blittable.point.should_make_from_coordinates");
        Point p = MakePoint(1.5, 2.5);
        Require(p.X == 1.5, "MakePoint.X");
        Require(p.Y == 2.5, "MakePoint.Y");

        DemoCase("case:records.blittable.point.should_roundtrip_value");
        Point echoed = EchoPoint(new Point(3.0, 4.0));
        Require(echoed == new Point(3.0, 4.0), "EchoPoint value equality");

        DemoCase("case:records.blittable.point.should_add_values");
        Point sum = AddPoints(new Point(1.0, 2.0), new Point(3.0, 4.0));
        Require(sum == new Point(4.0, 6.0), "AddPoints");

        DemoCase("case:records.blittable.color.should_make_from_channels");
        Color c = MakeColor(10, 20, 30, 255);
        Require(c.R == 10 && c.G == 20 && c.B == 30 && c.A == 255, "MakeColor fields");

        DemoCase("case:records.blittable.color.should_roundtrip_value");
        Color echoedColor = EchoColor(new Color(255, 0, 0, 128));
        Require(echoedColor == new Color(255, 0, 0, 128), "EchoColor value equality");

        // Static factories on a blittable record — return by value across
        // the ABI as a [StructLayout(Sequential)] struct.
        DemoCase("case:records.blittable.point.should_construct_with_static_new");
        Require(Point.New(1.5, 2.5) == new Point(1.5, 2.5), "Point.New");
        DemoCase("case:records.blittable.point.should_return_origin");
        Require(Point.Origin() == new Point(0.0, 0.0), "Point.Origin()");
        DemoCase("case:records.blittable.point.should_return_some_for_nonzero_coordinates");
        Point? tryNonzero = TryMakePoint(1.0, 2.0);
        Require(tryNonzero.HasValue && tryNonzero.Value == new Point(1.0, 2.0), "TryMakePoint(1,2)");
        DemoCase("case:records.blittable.point.should_return_none_for_origin_coordinates");
        Require(!TryMakePoint(0.0, 0.0).HasValue, "TryMakePoint(0,0)");
        DemoCase("case:records.blittable.point.should_construct_from_polar_coordinates");
        Point fromPolar = Point.FromPolar(2.0, Math.PI / 2.0);
        Require(Math.Abs(fromPolar.X) < 1e-9 && Math.Abs(fromPolar.Y - 2.0) < 1e-9, "Point.FromPolar");
        DemoCase("case:records.blittable.point.should_report_dimension_count");
        Require(Point.Dimensions() == 2u, "Point.Dimensions() == 2");

        DemoCase("case:records.blittable.point.should_normalize_unit_vector");
        Point unit = Point.TryUnit(3.0, 4.0);
        Require(Math.Abs(unit.X - 0.6) < 1e-9 && Math.Abs(unit.Y - 0.8) < 1e-9, "Point.TryUnit(3,4)");

        DemoCase("case:records.blittable.point.should_reject_zero_unit_vector");
        try
        {
            Point.TryUnit(0.0, 0.0);
            throw new Exception("expected Point.TryUnit(0,0) to throw");
        }
        catch (BoltException) { }

        DemoCase("case:records.blittable.point.should_return_some_for_checked_unit");
        Point? checked1 = Point.CheckedUnit(3.0, 4.0);
        Require(
            checked1 is { } cu && Math.Abs(cu.X - 0.6) < 1e-9 && Math.Abs(cu.Y - 0.8) < 1e-9,
            "Point.CheckedUnit(3,4)"
        );

        DemoCase("case:records.blittable.point.should_return_none_for_zero_checked_unit");
        Require(Point.CheckedUnit(0.0, 0.0) is null, "Point.CheckedUnit(0,0) == null");

        DemoCase("case:records.blittable.point.should_scale_coordinates");
        Point scaled = new Point(1.5, -2.5).Scale(2.0);
        Require(scaled == new Point(3.0, -5.0), "Point.Scale(2) doubles coordinates");

        // Instance methods on a blittable record — `this` passes by value
        // through P/Invoke (no wire encode), exercising the
        // owner_is_blittable branch of CSharpReceiver::InstanceNative.
        DemoCase("case:records.blittable.point.should_compute_distance");
        Require(Math.Abs(new Point(3.0, 4.0).Distance() - 5.0) < 1e-9, "Point(3,4).Distance() == 5");
        Require(new Point(0.0, 0.0).Distance() == 0.0, "Point.Origin.Distance() == 0");
        DemoCase("case:records.blittable.point.should_add_coordinates");
        Require(
            new Point(1.0, 2.0).Add(new Point(10.0, 20.0)) == new Point(11.0, 22.0),
            "Point.Add returns Point"
        );
        DemoCase("case:records.blittable.point.should_compute_path_length");
        Require(
            Math.Abs(Point.PathLength(new[] { new Point(0.0, 0.0), new Point(3.0, 4.0), new Point(6.0, 8.0) }) - 10.0) < 1e-9,
            "Point.PathLength(Point[])"
        );

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Non-blittable records travel through the wire path: WireWriter on
    /// the way in, FfiBuf + WireReader + FreeBuf on the way out. Strings
    /// inside records exercise the per-field UTF-8 length prefix.
    /// </summary>
    private static void TestRecordsWithStrings()
    {
        Console.WriteLine("Testing records with strings (Person, Address)...");

        DemoCase("case:records.with_strings.person.should_make_from_fields");
        Person alice = MakePerson("Alice", 30);
        Require(alice.Name == "Alice", "MakePerson.Name");
        Require(alice.Age == 30u, "MakePerson.Age");

        DemoCase("case:records.with_strings.person.should_roundtrip_value");
        Person echoed = EchoPerson(new Person("Bob", 42));
        Require(echoed == new Person("Bob", 42), "EchoPerson value equality");

        // Empty string boundary — the wire length prefix is 0.
        DemoCase("case:records.with_strings.person.should_roundtrip_value");
        Person empty = EchoPerson(new Person("", 0));
        Require(empty.Name == "", "EchoPerson empty name");

        // Multi-byte UTF-8 boundary — one code point that encodes as 4 bytes.
        DemoCase("case:records.with_strings.person.should_roundtrip_value");
        Person emoji = EchoPerson(new Person("\ud83c\udf89 Party", 25));
        Require(emoji.Name == "\ud83c\udf89 Party", "EchoPerson emoji round-trip");

        DemoCase("case:records.with_strings.person.should_format_greeting");
        Require(
            GreetPerson(new Person("Alice", 30)) == "Hello, Alice! You are 30 years old.",
            "GreetPerson format"
        );

        // Address has three string fields back-to-back — exercises multiple
        // length-prefixed slices in one wire buffer.
        DemoCase("case:records.with_strings.address.should_roundtrip_value");
        Address home = new Address("221B Baker Street", "London", "NW1 6XE");
        Address echoedAddress = EchoAddress(home);
        Require(echoedAddress == home, "EchoAddress round-trip");

        DemoCase("case:records.with_strings.address.should_format_value");
        Require(
            FormatAddress(home) == "221B Baker Street, London, NW1 6XE",
            "FormatAddress concatenation"
        );

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Non-blittable records with `#[data(impl)]` instance methods. The
    /// receiver wire-encodes `this` into a `byte[] self, UIntPtr selfLen`
    /// pair before the native call — the same shape as a non-blittable
    /// record passed as a regular parameter.
    /// </summary>
    private static void TestRecordsWithDefaults()
    {
        Console.WriteLine("Testing records with defaults and instance methods (ServiceConfig)...");

        DemoCase("case:records.default_values.service_config.should_roundtrip_value");
        ServiceConfig config = new ServiceConfig("worker", 3, "standard", null, "https://default");
        ServiceConfig echoed = EchoServiceConfig(config);
        Require(echoed == config, "EchoServiceConfig round-trip");

        DemoCase("case:records.default_values.service_config.should_describe_values");
        Require(
            config.Describe() == "worker:3:standard:none:https://default",
            "ServiceConfig.Describe() with defaults"
        );
        DemoCase("case:records.default_values.service_config.should_describe_with_prefix");
        Require(
            config.DescribeWithPrefix("cfg") == "cfg:worker:3:standard:none:https://default",
            "ServiceConfig.DescribeWithPrefix() string param"
        );

        ServiceConfig withEndpoint = new ServiceConfig("api", 5, "us-east", "https://primary", "https://backup");
        DemoCase("case:records.default_values.service_config.should_describe_values");
        Require(
            withEndpoint.Describe() == "api:5:us-east:https://primary:https://backup",
            "ServiceConfig.Describe() with endpoints"
        );

        DemoCase("case:records.default_values.service_config.try_with_retries.should_return_config");
        ServiceConfig withRetries = ServiceConfig.TryWithRetries(5);
        Require(
            withRetries == new ServiceConfig("generated", 5, "standard", null, "https://default"),
            "ServiceConfig.TryWithRetries(5)"
        );

        DemoCase("case:records.default_values.service_config.try_with_retries.should_reject_negative_retries");
        try
        {
            ServiceConfig.TryWithRetries(-1);
            throw new Exception("expected ServiceConfig.TryWithRetries(-1) to throw");
        }
        catch (BoltException) { }

        DemoCase("case:records.default_values.service_config.maybe_with_retries.should_return_some");
        ServiceConfig? maybeWithRetries = ServiceConfig.MaybeWithRetries(7);
        Require(
            maybeWithRetries is { } retries && retries == new ServiceConfig("generated", 7, "standard", null, "https://default"),
            "ServiceConfig.MaybeWithRetries(7)"
        );

        DemoCase("case:records.default_values.service_config.maybe_with_retries.should_return_none");
        Require(ServiceConfig.MaybeWithRetries(-1) is null, "ServiceConfig.MaybeWithRetries(-1) == null");

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Nested records: Line holds two Points, Rect holds Point + Dimensions.
    /// Exercises the record-inside-record wire encode/decode path.
    /// </summary>
    private static void TestNestedRecords()
    {
        Console.WriteLine("Testing nested records (Line, Rect)...");

        DemoCase("case:records.nested.line.should_make_from_coordinates");
        Line line = MakeLine(0.0, 0.0, 3.0, 4.0);
        Require(line.Start == new Point(0.0, 0.0), "MakeLine.Start");
        Require(line.End == new Point(3.0, 4.0), "MakeLine.End");

        DemoCase("case:records.nested.line.should_roundtrip_nested_points");
        Line echoed = EchoLine(line);
        Require(echoed == line, "EchoLine round-trip");

        DemoCase("case:records.nested.line.should_compute_length");
        Require(Math.Abs(LineLength(line) - 5.0) < 1e-9, "LineLength 3-4-5");

        DemoCase("case:records.nested.rect.should_roundtrip_nested_records");
        Rect rect = new Rect(
            new Point(1.0, 2.0),
            new Dimensions(10.0, 20.0)
        );
        Rect echoedRect = EchoRect(rect);
        Require(echoedRect == rect, "EchoRect round-trip");

        DemoCase("case:records.nested.rect.should_compute_area");
        Require(Math.Abs(RectArea(rect) - 200.0) < 1e-9, "RectArea 10*20");

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// C-style enums (Status, Direction, LogLevel) pass across P/Invoke as
    /// their declared backing type — no wire encoding. Instance methods show up as C#
    /// extension methods; static factories live on a `{Name}Methods`
    /// companion class.
    /// </summary>
    private static void TestCStyleEnums()
    {
        Console.WriteLine("Testing C-style enums (Status, Direction, LogLevel)...");

        // Direct P/Invoke round-trip — the CLR marshals the enum as its
        // declared backing type.
        DemoCase("case:enums.c_style.status.should_roundtrip_values");
        Require(EchoStatus(Status.Active) == Status.Active, "EchoStatus(Active)");
        Require(EchoStatus(Status.Pending) == Status.Pending, "EchoStatus(Pending)");
        DemoCase("case:enums.c_style.status.should_render_labels");
        Require(StatusToString(Status.Active) == "active", "StatusToString(Active)");
        DemoCase("case:enums.c_style.status.should_identify_active_values");
        Require(IsActive(Status.Active), "IsActive(Active)");
        Require(!IsActive(Status.Inactive), "IsActive(Inactive) false");

        DemoCase("case:enums.c_style.direction.should_roundtrip_value");
        Require(EchoDirection(Direction.North) == Direction.North, "EchoDirection(North)");
        DemoCase("case:enums.c_style.direction.should_return_opposite_from_free_function");
        Require(
            OppositeDirection(Direction.East) == Direction.West,
            "OppositeDirection(East) == West"
        );

        // Extension methods generated on the Methods companion class.
        DemoCase("case:enums.c_style.direction.should_return_opposite_from_method");
        Require(Direction.North.Opposite() == Direction.South, "North.Opposite()");
        DemoCase("case:enums.c_style.direction.should_identify_horizontal_values");
        Require(Direction.East.IsHorizontal(), "East.IsHorizontal()");
        Require(!Direction.North.IsHorizontal(), "!North.IsHorizontal()");
        DemoCase("case:enums.c_style.direction.should_render_compass_label");
        Require(Direction.South.Label() == "S", "South.Label()");

        // Static factories on the companion class.
        DemoCase("case:enums.c_style.direction.should_return_cardinal_value");
        Require(DirectionMethods.Cardinal() == Direction.North, "Cardinal() == North");
        DemoCase("case:enums.c_style.direction.should_construct_from_degrees");
        Require(DirectionMethods.FromDegrees(90.0) == Direction.East, "FromDegrees(90) == East");
        Require(DirectionMethods.FromDegrees(180.0) == Direction.South, "FromDegrees(180) == South");
        DemoCase("case:enums.c_style.direction.should_report_variant_count");
        Require(DirectionMethods.Count() == 4u, "Count() == 4");
        DemoCase("case:enums.c_style.direction.should_construct_from_raw_value");
        Require(DirectionMethods.New(2) == Direction.East, "New(2) == East");

        DemoCase("case:enums.c_style.direction.should_return_degrees");
        Require(DirectionToDegrees(Direction.North) == 0, "DirectionToDegrees(North)");
        Require(DirectionToDegrees(Direction.East) == 90, "DirectionToDegrees(East)");

        DemoCase("case:enums.c_style.direction.find_direction.should_return_some_for_known_id");
        Direction? foundDir = FindDirection(1);
        Require(foundDir.HasValue && foundDir.Value == Direction.East, "FindDirection(1)");
        DemoCase("case:enums.c_style.direction.find_direction.should_return_none_for_unknown_id");
        Require(!FindDirection(99).HasValue, "FindDirection(99)");

        DemoCase("case:enums.c_style.direction.find_directions.should_return_sequence_for_positive_count");
        Direction[] someDirs = FindDirections(3);
        Require(someDirs is not null && someDirs.Length == 3, "FindDirections(3)");
        DemoCase("case:enums.c_style.direction.find_directions.should_return_none_for_non_positive_count");
        Require(FindDirections(0) is null, "FindDirections(0)");

        // Non-default backing type: LogLevel is #[repr(u8)] on the Rust side,
        // so these direct P/Invoke calls catch any accidental `enum : int`
        // projection in the generated C# surface.
        DemoCase("case:enums.repr_int.priority.should_roundtrip_value");
        Require(EchoPriority(Priority.High) == Priority.High, "EchoPriority(High)");
        DemoCase("case:enums.repr_int.priority.should_render_label");
        Require(PriorityLabel(Priority.Low) == "low", "PriorityLabel(Low)");
        Require(PriorityLabel(Priority.Critical) == "critical", "PriorityLabel(Critical)");
        DemoCase("case:enums.repr_int.priority.should_identify_high_priority");
        Require(IsHighPriority(Priority.High), "IsHighPriority(High)");
        Require(IsHighPriority(Priority.Critical), "IsHighPriority(Critical)");
        Require(!IsHighPriority(Priority.Low), "IsHighPriority(Low) == false");

        DemoCase("case:enums.repr_int.log_level.should_roundtrip_value");
        Require(EchoLogLevel(LogLevel.Trace) == LogLevel.Trace, "EchoLogLevel(Trace)");
        Require(EchoLogLevel(LogLevel.Error) == LogLevel.Error, "EchoLogLevel(Error)");
        DemoCase("case:enums.repr_int.log_level.should_compare_against_minimum");
        Require(ShouldLog(LogLevel.Error, LogLevel.Warn), "ShouldLog(Error, Warn)");
        Require(!ShouldLog(LogLevel.Debug, LogLevel.Info), "!ShouldLog(Debug, Info)");

        // HttpCode has gapped #[repr(u16)] discriminants (200, 404, 500).
        // The raw value of each C# member must equal the Rust discriminant,
        // and a value constructed on the Rust side must map back to the
        // corresponding named member on the C# side.
        DemoCase("case:enums.repr_int.http_code.should_expose_discriminant_values");
        Require((ushort)HttpCode.Ok == 200, "HttpCode.Ok == 200");
        Require((ushort)HttpCode.NotFound == 404, "HttpCode.NotFound == 404");
        Require((ushort)HttpCode.ServerError == 500, "HttpCode.ServerError == 500");
        DemoCase("case:enums.repr_int.http_code.should_return_not_found");
        Require(HttpCodeNotFound() == HttpCode.NotFound, "Rust NotFound == C# NotFound");
        DemoCase("case:enums.repr_int.http_code.should_roundtrip_values");
        Require(EchoHttpCode(HttpCode.Ok) == HttpCode.Ok, "EchoHttpCode(Ok)");
        Require(EchoHttpCode(HttpCode.ServerError) == HttpCode.ServerError, "EchoHttpCode(ServerError)");

        // Sign has a #[repr(i8)] with a negative discriminant. The CLR
        // marshals sbyte across P/Invoke; the bit pattern must stay signed
        // in both directions.
        DemoCase("case:enums.repr_int.sign.should_expose_signed_discriminant_values");
        Require((sbyte)Sign.Negative == -1, "Sign.Negative == -1");
        Require((sbyte)Sign.Zero == 0, "Sign.Zero == 0");
        Require((sbyte)Sign.Positive == 1, "Sign.Positive == 1");
        DemoCase("case:enums.repr_int.sign.should_return_negative");
        Require(SignNegative() == Sign.Negative, "Rust Negative == C# Negative");
        DemoCase("case:enums.repr_int.sign.should_roundtrip_signed_values");
        Require(EchoSign(Sign.Negative) == Sign.Negative, "EchoSign(Negative)");
        Require(EchoSign(Sign.Positive) == Sign.Positive, "EchoSign(Positive)");

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Data enums (Shape, Message, Animal) travel across the wire —
    /// `WireWriter` on the way in, `FfiBuf` + `WireReader` on the way
    /// out. Exercises every variant shape the renderer produces: unit,
    /// single-field, multi-field, and nested-record payloads. Pattern
    /// matching on the returned value confirms the discriminated-union
    /// surface is intact.
    /// </summary>
    private static void TestDataEnums()
    {
        Console.WriteLine("Testing data enums (Shape, Message, Animal)...");

        // Shape — named-field variants, a nested-record variant with a
        // shadowed outer Point, and a unit variant that collides with
        // the outer Point record name.
        DemoCase("case:enums.data_enum.shape.should_roundtrip_core_variants");
        Shape circle = new Shape.Circle(5.0);
        Shape echoedCircle = EchoShape(circle);
        Require(echoedCircle is Shape.Circle c && c.Radius == 5.0, "EchoShape(Circle)");

        Shape rect = new Shape.Rectangle(3.0, 4.0);
        Shape echoedRect = EchoShape(rect);
        Require(
            echoedRect is Shape.Rectangle r && r.Width == 3.0 && r.Height == 4.0,
            "EchoShape(Rectangle)"
        );

        Shape triangle = new Shape.Triangle(
            new Point(0.0, 0.0),
            new Point(4.0, 0.0),
            new Point(0.0, 3.0)
        );
        Shape echoedTriangle = EchoShape(triangle);
        Require(
            echoedTriangle is Shape.Triangle t
                && t.A == new Point(0.0, 0.0)
                && t.B == new Point(4.0, 0.0)
                && t.C == new Point(0.0, 3.0),
            "EchoShape(Triangle) with nested Point"
        );

        Shape point = new Shape.Point();
        Shape echoedPoint = EchoShape(point);
        Require(echoedPoint is Shape.Point, "EchoShape(Point) unit variant");

        // Apex — Option<Point> as a variant field where Point is shadowed
        // by the sibling Shape.Point unit variant. Drives the scoped
        // rendering of the nullable cast inside the Shape scope.
        DemoCase("case:enums.data_enum.shape.apex.should_roundtrip_some_point_payload");
        Shape apexSome = new Shape.Apex(new Point(3.0, 4.0));
        Shape echoedApexSome = EchoShape(apexSome);
        Require(
            echoedApexSome is Shape.Apex asome && asome.Tip == new Point(3.0, 4.0),
            "EchoShape(Apex with Some(Point))"
        );

        DemoCase("case:enums.data_enum.shape.apex.should_roundtrip_none_payload");
        Shape apexNone = new Shape.Apex(null);
        Shape echoedApexNone = EchoShape(apexNone);
        Require(
            echoedApexNone is Shape.Apex anone && anone.Tip is null,
            "EchoShape(Apex with None)"
        );

        // Cluster — Vec<Point> as a variant field, same shadow setup.
        // Drives the scoped rendering of the ReadEncodedArray / blittable
        // array element type inside the Shape scope.
        DemoCase("case:enums.data_enum.shape.should_roundtrip_vector_record_fields");
        Shape cluster = new Shape.Cluster(new[]
        {
            new Point(1.0, 2.0),
            new Point(3.0, 4.0),
            new Point(5.0, 6.0),
        });
        Shape echoedCluster = EchoShape(cluster);
        Require(
            echoedCluster is Shape.Cluster cl && cl.Members.Length == 3
                && cl.Members[0] == new Point(1.0, 2.0)
                && cl.Members[2] == new Point(5.0, 6.0),
            "EchoShape(Cluster with Vec<Point>)"
        );
        Require(
            EchoShape(new Shape.Cluster(Array.Empty<Point>())) is Shape.Cluster clE
                && clE.Members.Length == 0,
            "EchoShape(Cluster empty)"
        );

        // Free-function factories producing Shape.
        DemoCase("case:enums.data_enum.shape.should_support_free_function_factories");
        Require(MakeCircle(2.0) is Shape.Circle c2 && c2.Radius == 2.0, "MakeCircle");
        Require(
            MakeRectangle(5.0, 10.0) is Shape.Rectangle r2 && r2.Width == 5.0 && r2.Height == 10.0,
            "MakeRectangle"
        );

        // Instance methods on the data enum — wire-encode self, call
        // native, decode return.
        DemoCase("case:enums.data_enum.shape.should_support_numeric_instance_methods");
        Require(Math.Abs(new Shape.Circle(1.0).Area() - Math.PI) < 1e-9, "Circle(1).Area() == PI");
        Require(new Shape.Rectangle(3.0, 4.0).Area() == 12.0, "Rectangle(3,4).Area()");
        Require(new Shape.Point().Area() == 0.0, "Point.Area() == 0");

        DemoCase("case:enums.data_enum.shape.should_support_string_instance_methods");
        Require(new Shape.Circle(2.0).Describe() == "circle r=2", "Circle.Describe()");
        Require(new Shape.Point().Describe() == "point", "Point.Describe()");

        // Static methods / factories on the data enum.
        DemoCase("case:enums.data_enum.shape.unit_circle.should_construct_circle");
        Require(Shape.UnitCircle() is Shape.Circle uc && uc.Radius == 1.0, "Shape.UnitCircle()");
        DemoCase("case:enums.data_enum.shape.square.should_construct_rectangle");
        Require(
            Shape.Square(7.0) is Shape.Rectangle sq && sq.Width == 7.0 && sq.Height == 7.0,
            "Shape.Square(7)"
        );

        DemoCase("case:enums.data_enum.shape.should_report_variant_count");
        Require(Shape.VariantCount() == 6u, "Shape.VariantCount() == 6");

        DemoCase("case:enums.data_enum.shape.should_support_primary_constructor");
        Require(Shape.New(3.0) is Shape.Circle sn && sn.Radius == 3.0, "Shape.New(3)");

        DemoCase("case:enums.data_enum.shape.try_circle.should_return_circle_for_positive_radius");
        Require(Shape.TryCircle(2.5) is Shape.Circle tc && tc.Radius == 2.5, "Shape.TryCircle(2.5)");

        DemoCase("case:enums.data_enum.shape.should_reject_non_positive_circle_radius");
        try
        {
            Shape.TryCircle(-1.0);
            throw new Exception("expected Shape.TryCircle(-1) to throw");
        }
        catch (BoltException) { }

        DemoCase("case:enums.data_enum.shape.maybe_circle.should_return_some_for_positive_radius");
        Require(Shape.MaybeCircle(1.25) is Shape.Circle mc && mc.Radius == 1.25, "Shape.MaybeCircle(1.25)");

        DemoCase("case:enums.data_enum.shape.maybe_circle.should_return_none_for_non_positive_radius");
        Require(Shape.MaybeCircle(0.0) is null, "Shape.MaybeCircle(0) == null");

        // TryApexPoint — static method whose return type is Option<Point>
        // where Point is shadowed by a sibling variant. Drives scoped
        // rendering of the Option decode inside the Shape scope.
        DemoCase("case:enums.data_enum.shape.try_apex_point.should_return_some_for_positive_radius");
        Point? apexPt = Shape.TryApexPoint(2.5);
        Require(apexPt is { } pt && pt.X == 0.0 && pt.Y == 2.5, "Shape.TryApexPoint(positive)");
        DemoCase("case:enums.data_enum.shape.try_apex_point.should_return_none_for_non_positive_radius");
        Require(Shape.TryApexPoint(-1.0) is null, "Shape.TryApexPoint(negative) == null");

        // Message — mixes string, primitive, and unit variants.
        DemoCase("case:enums.data_enum.message.text.should_roundtrip_string_payload");
        Message text = new Message.Text("hello");
        Require(
            EchoMessage(text) is Message.Text et && et.Body == "hello",
            "EchoMessage(Text)"
        );

        DemoCase("case:enums.data_enum.message.image.should_roundtrip_url_dimensions_payload");
        Message image = new Message.Image("https://example.com/a.png", 1920, 1080);
        Require(
            EchoMessage(image) is Message.Image ei
                && ei.Url == "https://example.com/a.png"
                && ei.Width == 1920u
                && ei.Height == 1080u,
            "EchoMessage(Image)"
        );

        DemoCase("case:enums.data_enum.message.ping.should_roundtrip_unit_variant");
        Message ping = new Message.Ping();
        Require(EchoMessage(ping) is Message.Ping, "EchoMessage(Ping)");

        DemoCase("case:enums.data_enum.message.text.should_render_text_summary");
        Require(
            MessageSummary(new Message.Text("hi")) == "text: hi",
            "MessageSummary(Text)"
        );
        DemoCase("case:enums.data_enum.message.ping.should_render_ping_summary");
        Require(MessageSummary(new Message.Ping()) == "ping", "MessageSummary(Ping)");
        DemoCase("case:enums.data_enum.message.image.should_render_image_summary");
        Require(
            MessageSummary(new Message.Image("https://example.com/a.png", 1920, 1080))
                == "image: 1920x1080 at https://example.com/a.png",
            "MessageSummary(Image)"
        );

        // Animal — three struct variants, one with a bool field.
        DemoCase("case:enums.data_enum.animal.dog.should_roundtrip_string_payloads");
        Animal dog = new Animal.Dog("Rex", "Labrador");
        Require(
            EchoAnimal(dog) is Animal.Dog d && d.Name == "Rex" && d.Breed == "Labrador",
            "EchoAnimal(Dog)"
        );

        DemoCase("case:enums.data_enum.animal.cat.should_roundtrip_name_and_bool_payload");
        Animal cat = new Animal.Cat("Whiskers", true);
        Require(
            EchoAnimal(cat) is Animal.Cat ca && ca.Name == "Whiskers" && ca.Indoor,
            "EchoAnimal(Cat indoor)"
        );

        DemoCase("case:enums.data_enum.animal.fish.should_roundtrip_count_payload");
        Animal fish = new Animal.Fish(3u);
        Require(
            EchoAnimal(fish) is Animal.Fish f && f.Count == 3u,
            "EchoAnimal(Fish)"
        );

        DemoCase("case:enums.data_enum.animal.dog.should_derive_name");
        Require(AnimalName(new Animal.Dog("Rex", "Lab")) == "Rex", "AnimalName(Dog)");
        DemoCase("case:enums.data_enum.animal.cat.should_derive_name");
        Require(AnimalName(new Animal.Cat("Whiskers", true)) == "Whiskers", "AnimalName(Cat)");
        DemoCase("case:enums.data_enum.animal.fish.should_derive_count_label");
        Require(AnimalName(new Animal.Fish(5u)) == "5 fish", "AnimalName(Fish)");

        // LifecycleEvent — a data enum whose variant payload carries a
        // C-style enum (Priority). The codec must wire-encode the outer
        // variant tag and the inner enum's backing integer together.
        DemoCase("case:enums.data_enum.lifecycle_event.should_make_critical_event");
        LifecycleEvent started = MakeCriticalLifecycleEvent(7);
        Require(
            started is LifecycleEvent.TaskStarted ts
                && ts.Priority == Priority.Critical
                && ts.Id == 7,
            "MakeCriticalLifecycleEvent returns TaskStarted with Critical priority"
        );
        DemoCase("case:enums.data_enum.lifecycle_event.should_roundtrip_priority_payload");
        LifecycleEvent echoedStarted = EchoLifecycleEvent(started);
        Require(echoedStarted == started, "EchoLifecycleEvent(TaskStarted) round-trip");
        DemoCase("case:enums.data_enum.lifecycle_event.should_roundtrip_tick_variant");
        LifecycleEvent tick = new LifecycleEvent.Tick();
        Require(EchoLifecycleEvent(tick) is LifecycleEvent.Tick, "EchoLifecycleEvent(Tick)");

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Records that embed a C-style enum field stay on the wire path if
    /// they also have non-blittable fields (e.g., a string). The enum
    /// field flows through via `PriorityWire.Decode` / the
    /// `WireEncodeTo` extension method, uniform with how record fields
    /// embed other records.
    /// </summary>
    private static void TestRecordsWithEnumFields()
    {
        Console.WriteLine("Testing records with enum fields (Notification, Task)...");

        // Task is a C# keyword in `System.Threading.Tasks` — the generated
        // record fully qualifies to avoid collision when addressing it
        // directly. Using the namespace-qualified form makes the intent
        // explicit here too.
        global::Demo.Task task = new global::Demo.Task("Write docs", Priority.High, false);
        DemoCase("case:records.with_enums.task.should_roundtrip_priority_field");
        global::Demo.Task echoedTask = EchoTask(task);
        Require(echoedTask == task, "EchoTask round-trip");
        Require(echoedTask.Priority == Priority.High, "Task.Priority preserved");

        DemoCase("case:records.with_enums.task.should_make_incomplete_task");
        global::Demo.Task made = MakeTask("New task", Priority.Low);
        Require(made.Title == "New task" && made.Priority == Priority.Low && !made.Completed, "MakeTask defaults");

        DemoCase("case:records.with_enums.task.should_detect_urgent_priority");
        Require(IsUrgent(new global::Demo.Task("urgent", Priority.Critical, false)), "IsUrgent(Critical)");
        Require(!IsUrgent(new global::Demo.Task("normal", Priority.Low, false)), "IsUrgent(Low) false");

        Notification notification = new Notification("Build failed", Priority.Critical, false);
        DemoCase("case:records.with_enums.notification.should_roundtrip_priority_field");
        Notification echoedNotification = EchoNotification(notification);
        Require(echoedNotification == notification, "EchoNotification round-trip");
        Require(echoedNotification.Priority == Priority.Critical, "Notification.Priority preserved");
        Require(!echoedNotification.Read, "Notification.Read preserved");

        // Holder is #[repr(C)] but wraps a data enum (Shape). Data enums
        // have a variable-width on-the-wire representation — this record
        // must ride the wire codec, not direct P/Invoke, despite the
        // repr(C) decoration.
        DemoCase("case:records.with_enums.holder.should_make_triangle_variant");
        Holder triangle = MakeTriangleHolder();
        Require(
            triangle.Shape is Shape.Triangle t
                && t.A == new Point(0.0, 0.0)
                && t.B == new Point(4.0, 0.0)
                && t.C == new Point(0.0, 3.0),
            "MakeTriangleHolder returns Triangle"
        );
        DemoCase("case:records.with_enums.holder.should_roundtrip_data_enum_field");
        Holder echoedHolder = EchoHolder(triangle);
        Require(echoedHolder == triangle, "EchoHolder round-trip");

        // TaskHeader is #[repr(C)] with primitive + C-style enum fields,
        // but rides the wire codec like any record with a non-primitive
        // field: the Rust #[export] macro doesn't yet admit C-style enums
        // as layout-compatible primitives, so both sides agree on wire
        // encoding. Follow-up work (see TaskHeader doc) can widen both
        // sides together to lift this onto direct P/Invoke.
        DemoCase("case:records.with_enums.task_header.should_make_critical_header");
        TaskHeader header = MakeCriticalTaskHeader(42);
        Require(header.Id == 42, "MakeCriticalTaskHeader.Id");
        Require(header.Priority == Priority.Critical, "MakeCriticalTaskHeader.Priority");
        Require(!header.Completed, "MakeCriticalTaskHeader.Completed");
        DemoCase("case:records.with_enums.task_header.should_roundtrip_repr_enum_field");
        TaskHeader echoedHeader = EchoTaskHeader(header);
        Require(echoedHeader == header, "EchoTaskHeader round-trip");

        // LogEntry — same family as TaskHeader but the C-style enum field
        // is u8-backed, so field alignment matters. Wire-encoded today for
        // the same reason TaskHeader is.
        DemoCase("case:records.with_enums.log_entry.should_make_error_entry");
        LogEntry entry = MakeErrorLogEntry(1234567890, 42);
        Require(entry.Timestamp == 1234567890, "MakeErrorLogEntry.Timestamp");
        Require(entry.Level == LogLevel.Error, "MakeErrorLogEntry.Level");
        Require(entry.Code == 42, "MakeErrorLogEntry.Code");
        DemoCase("case:records.with_enums.log_entry.should_roundtrip_u8_enum_field");
        LogEntry echoedEntry = EchoLogEntry(entry);
        Require(echoedEntry == entry, "EchoLogEntry round-trip");

        Console.WriteLine("  PASS\n");
    }

    private static void TestPrimitiveVecs()
    {
        Console.WriteLine("Testing primitive vecs...");

        DemoCase("case:primitives.vecs.i32.should_roundtrip_non_empty");
        int[] echoedI32 = EchoVecI32(new int[] { 1, 2, 3 });
        Require(echoedI32.SequenceEqual(new[] { 1, 2, 3 }), "echoVecI32");
        DemoCase("case:primitives.vecs.i32.should_roundtrip_empty");
        Require(EchoVecI32(Array.Empty<int>()).Length == 0, "echoVecI32 empty");

        DemoCase("case:primitives.vecs.i8.should_roundtrip_values");
        Require(EchoVecI8(new sbyte[] { -1, 0, 7 }).SequenceEqual(new sbyte[] { -1, 0, 7 }), "echoVecI8");
        DemoCase("case:primitives.vecs.u8.should_roundtrip_values");
        Require(EchoVecU8(new byte[] { 0, 1, 2, 3 }).SequenceEqual(new byte[] { 0, 1, 2, 3 }), "echoVecU8");
        DemoCase("case:primitives.vecs.i16.should_roundtrip_values");
        Require(EchoVecI16(new short[] { -3, 0, 9 }).SequenceEqual(new short[] { -3, 0, 9 }), "echoVecI16");
        DemoCase("case:primitives.vecs.u16.should_roundtrip_values");
        Require(EchoVecU16(new ushort[] { 0, 10, 20 }).SequenceEqual(new ushort[] { 0, 10, 20 }), "echoVecU16");
        DemoCase("case:primitives.vecs.u32.should_roundtrip_values");
        Require(EchoVecU32(new uint[] { 0, 10, 20 }).SequenceEqual(new uint[] { 0, 10, 20 }), "echoVecU32");
        DemoCase("case:primitives.vecs.i64.should_roundtrip_values");
        Require(EchoVecI64(new long[] { -5L, 0L, 8L }).SequenceEqual(new long[] { -5L, 0L, 8L }), "echoVecI64");
        DemoCase("case:primitives.vecs.u64.should_roundtrip_values");
        Require(EchoVecU64(new ulong[] { 0UL, 1UL, 2UL }).SequenceEqual(new ulong[] { 0UL, 1UL, 2UL }), "echoVecU64");
        DemoCase("case:primitives.vecs.isize.should_roundtrip_values");
        Require(EchoVecIsize(new nint[] { -2, 0, 5 }).SequenceEqual(new nint[] { -2, 0, 5 }), "echoVecIsize");
        DemoCase("case:primitives.vecs.usize.should_roundtrip_values");
        Require(EchoVecUsize(new nuint[] { 0, 2, 4 }).SequenceEqual(new nuint[] { 0, 2, 4 }), "echoVecUsize");
        DemoCase("case:primitives.vecs.f32.should_roundtrip_values_with_tolerance");
        Require(EchoVecF32(new float[] { 1.25f, -2.5f }).SequenceEqual(new float[] { 1.25f, -2.5f }), "echoVecF32");
        DemoCase("case:primitives.vecs.f64.should_roundtrip_values");
        Require(EchoVecF64(new double[] { 1.5, 2.5 }).SequenceEqual(new double[] { 1.5, 2.5 }), "echoVecF64");
        DemoCase("case:primitives.vecs.bool.should_roundtrip_values");
        Require(EchoVecBool(new bool[] { true, false, true }).SequenceEqual(new bool[] { true, false, true }), "echoVecBool");

        DemoCase("case:primitives.vecs.i32.should_sum_values");
        Require(SumVecI32(new int[] { 10, 20, 30 }) == 60L, "sumVecI32");
        Require(SumVecI32(Array.Empty<int>()) == 0L, "sumVecI32 empty");

        DemoCase("case:primitives.vecs.i32.should_sum_benchmark_values");
        Require(SumI32Vec(new int[] { 1, 2, 3, 4 }) == 10L, "sumI32Vec");

        DemoCase("case:primitives.vecs.u64.should_increment_value");
        Require(IncU64Value(41UL) == 42UL, "incU64Value");

        DemoCase("case:primitives.vecs.u64.should_increment_first_value_in_place");
        ulong[] incBuf = new ulong[] { 10UL, 20UL, 30UL };
        IncU64(incBuf);
        Require(
            incBuf[0] == 11UL && incBuf[1] == 20UL && incBuf[2] == 30UL,
            "incU64 mutates first element in place"
        );

        DemoCase("case:primitives.vecs.i32.should_make_range");
        Require(MakeRange(0, 5).SequenceEqual(new int[] { 0, 1, 2, 3, 4 }), "makeRange");
        DemoCase("case:primitives.vecs.i32.should_reverse_values");
        Require(ReverseVecI32(new int[] { 1, 2, 3 }).SequenceEqual(new int[] { 3, 2, 1 }), "reverseVecI32");
        DemoCase("case:primitives.vecs.i32.should_generate_sequence");
        Require(GenerateI32Vec(4).SequenceEqual(new int[] { 0, 1, 2, 3 }), "generateI32Vec");
        DemoCase("case:primitives.vecs.f64.should_generate_sequence");
        Require(GenerateF64Vec(3).Length == 3, "generateF64Vec length");
        DemoCase("case:primitives.vecs.f64.should_sum_values");
        Require(Math.Abs(SumF64Vec(new double[] { 0.5, 1.5, 2.0 }) - 4.0) < 1e-9, "sumF64Vec");

        Console.WriteLine("  PASS\n");
    }

    private static void TestBytes()
    {
        Console.WriteLine("Testing bytes...");

        DemoCase("case:bytes.bytes.should_roundtrip_values");
        Require(EchoBytes(new byte[] { 1, 2, 3, 4 }).SequenceEqual(new byte[] { 1, 2, 3, 4 }), "echoBytes");
        DemoCase("case:bytes.bytes.should_report_length");
        Require(BytesLength(new byte[] { 9, 8, 7 }) == 3u, "bytesLength");
        DemoCase("case:bytes.bytes.should_sum_values");
        Require(BytesSum(new byte[] { 1, 2, 3, 4 }) == 10u, "bytesSum");
        DemoCase("case:bytes.bytes.should_make_sequential_values");
        Require(MakeBytes(4).SequenceEqual(new byte[] { 0, 1, 2, 3 }), "makeBytes");
        DemoCase("case:bytes.bytes.should_reverse_values");
        Require(ReverseBytes(new byte[] { 1, 2, 3, 4 }).SequenceEqual(new byte[] { 4, 3, 2, 1 }), "reverseBytes");

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Vec&lt;String&gt; and Vec&lt;Vec&lt;_&gt;&gt; travel wire-encoded: the param
    /// side builds a length-prefixed buffer via WireWriter, the return
    /// side walks the buffer through ReadEncodedArray. Exercises the
    /// 2-byte ("café") and 4-byte ("🌍") UTF-8 boundaries at the element
    /// level so truncation or mis-sized length prefixes surface loudly.
    /// </summary>
    private static void TestStringAndNestedVecs()
    {
        Console.WriteLine("Testing Vec<String> and Vec<Vec<_>>...");

        string[] words = new[] { "hello", "", "café", "🌍" };
        DemoCase("case:primitives.vecs.string.should_roundtrip_values");
        string[] echoedWords = EchoVecString(words);
        Require(echoedWords.SequenceEqual(words), "echoVecString round-trip");
        Require(EchoVecString(Array.Empty<string>()).Length == 0, "echoVecString empty");

        DemoCase("case:primitives.vecs.string.should_report_utf8_byte_lengths");
        uint[] lengths = VecStringLengths(new[] { "", "a", "café", "🌍" });
        Require(lengths.SequenceEqual(new uint[] { 0u, 1u, 5u, 4u }), "vecStringLengths UTF-8 byte counts");

        DemoCase("case:primitives.vecs.nested_i32.should_roundtrip_values");
        int[][] nestedInts = new[]
        {
            new[] { 1, 2, 3 },
            Array.Empty<int>(),
            new[] { -1 },
        };
        int[][] echoedInts = EchoVecVecI32(nestedInts);
        Require(echoedInts.Length == nestedInts.Length, "echoVecVecI32 outer length");
        for (int i = 0; i < nestedInts.Length; i++)
        {
            Require(echoedInts[i].SequenceEqual(nestedInts[i]), $"echoVecVecI32 inner[{i}]");
        }
        DemoCase("case:primitives.vecs.nested_i32.should_roundtrip_empty_outer");
        Require(EchoVecVecI32(Array.Empty<int[]>()).Length == 0, "echoVecVecI32 empty outer");

        DemoCase("case:primitives.vecs.nested_bool.should_roundtrip_values");
        bool[][] nestedBools = new[]
        {
            new[] { true, false, true },
            Array.Empty<bool>(),
            new[] { false },
        };
        bool[][] echoedBools = EchoVecVecBool(nestedBools);
        Require(echoedBools.Length == nestedBools.Length, "echoVecVecBool outer length");
        for (int i = 0; i < nestedBools.Length; i++)
        {
            Require(echoedBools[i].SequenceEqual(nestedBools[i]), $"echoVecVecBool inner[{i}]");
        }

        DemoCase("case:primitives.vecs.nested_isize.should_roundtrip_values");
        nint[][] nestedIsizes = new[]
        {
            new nint[] { -2, 0, 5 },
            Array.Empty<nint>(),
            new nint[] { 9 },
        };
        nint[][] echoedIsizes = EchoVecVecIsize(nestedIsizes);
        Require(echoedIsizes.Length == nestedIsizes.Length, "echoVecVecIsize outer length");
        for (int i = 0; i < nestedIsizes.Length; i++)
        {
            Require(echoedIsizes[i].SequenceEqual(nestedIsizes[i]), $"echoVecVecIsize inner[{i}]");
        }

        DemoCase("case:primitives.vecs.nested_usize.should_roundtrip_values");
        nuint[][] nestedUsizes = new[]
        {
            new nuint[] { 0, 2, 4 },
            Array.Empty<nuint>(),
            new nuint[] { 8 },
        };
        nuint[][] echoedUsizes = EchoVecVecUsize(nestedUsizes);
        Require(echoedUsizes.Length == nestedUsizes.Length, "echoVecVecUsize outer length");
        for (int i = 0; i < nestedUsizes.Length; i++)
        {
            Require(echoedUsizes[i].SequenceEqual(nestedUsizes[i]), $"echoVecVecUsize inner[{i}]");
        }

        DemoCase("case:primitives.vecs.nested_i32.should_flatten_values");
        int[] flattened = FlattenVecVecI32(nestedInts);
        Require(flattened.SequenceEqual(new[] { 1, 2, 3, -1 }), "flattenVecVecI32");
        DemoCase("case:primitives.vecs.nested_i32.should_flatten_empty");
        Require(FlattenVecVecI32(Array.Empty<int[]>()).Length == 0, "flattenVecVecI32 empty");

        DemoCase("case:primitives.vecs.nested_string.should_roundtrip_utf8_values");
        string[][] nestedStrings = new[]
        {
            new[] { "café", "🌍" },
            Array.Empty<string>(),
            new[] { "" },
            new[] { "one", "two", "three" },
        };
        string[][] echoedStrings = EchoVecVecString(nestedStrings);
        Require(echoedStrings.Length == nestedStrings.Length, "echoVecVecString outer length");
        for (int i = 0; i < nestedStrings.Length; i++)
        {
            Require(echoedStrings[i].SequenceEqual(nestedStrings[i]), $"echoVecVecString inner[{i}]");
        }

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Vec&lt;BlittableRecord&gt; rides the fast path: returns reinterpret the
    /// FfiBuf as a T[] via ReadBlittableArray&lt;T&gt;, params pin a T[] and
    /// hand a pointer across P/Invoke. No wire encoding on either side.
    /// The generate_* and reduce_* demo pairs cross the boundary in both
    /// directions with the same struct layout on each side, so any mismatch
    /// between Rust's #[repr(C)] and C#'s [StructLayout(Sequential)] would
    /// surface as a wrong sum or a segfault.
    /// </summary>
    private static void TestBlittableRecordVecs()
    {
        Console.WriteLine("Testing blittable record vecs (Location, Trade, Particle, SensorReading)...");

        DemoCase("case:records.blittable.locations.should_generate_sample_vector");
        Location[] locations = GenerateLocations(3);
        Require(locations.Length == 3, "generateLocations length");
        Require(locations[0].Id == 0L, "locations[0].Id");
        Require(locations[0].Rating == 3.0, "locations[0].Rating");
        Require(locations[0].IsOpen, "locations[0].IsOpen");
        Require(locations[1].Id == 1L, "locations[1].Id");
        Require(!locations[1].IsOpen, "locations[1].IsOpen");
        Require(locations[2].ReviewCount == 20, "locations[2].ReviewCount");

        DemoCase("case:records.blittable.locations.should_count_vector_items");
        Require(ProcessLocations(locations) == 3, "processLocations roundtrip");
        DemoCase("case:records.blittable.locations.should_count_empty_vector");
        Require(ProcessLocations(Array.Empty<Location>()) == 0, "processLocations empty");
        DemoCase("case:records.blittable.locations.should_sum_generated_ratings");
        Require(Math.Abs(SumRatings(locations) - (3.0 + 3.1 + 3.2)) < 1e-9, "sumRatings roundtrip");

        DemoCase("case:records.blittable.trades.should_generate_sample_vector");
        Trade[] trades = GenerateTrades(3);
        Require(trades.Length == 3, "generateTrades length");
        Require(trades[0].Volume == 0L && trades[1].Volume == 1000L && trades[2].Volume == 2000L, "trades volumes");
        DemoCase("case:records.blittable.trades.should_sum_volumes");
        Require(SumTradeVolumes(trades) == 3000L, "sumTradeVolumes roundtrip");
        DemoCase("case:records.blittable.trades.should_aggregate_with_locations");
        Require(AggregateLocationTradeStats(locations, trades) == 3002L, "aggregateLocationTradeStats two pinned arrays");

        DemoCase("case:records.blittable.particles.should_generate_sample_vector");
        Particle[] particles = GenerateParticles(3);
        Require(particles.Length == 3, "generateParticles length");
        DemoCase("case:records.blittable.particles.should_sum_masses");
        Require(Math.Abs(SumParticleMasses(particles) - (1.0 + 1.001 + 1.002)) < 1e-9, "sumParticleMasses roundtrip");

        DemoCase("case:records.blittable.sensor_readings.should_generate_sample_vector");
        SensorReading[] readings = GenerateSensorReadings(3);
        Require(readings.Length == 3, "generateSensorReadings length");
        DemoCase("case:records.blittable.sensor_readings.should_average_generated_temperatures");
        Require(Math.Abs(AvgSensorTemperature(readings) - 21.0) < 1e-9, "avgSensorTemperature roundtrip");
        DemoCase("case:records.blittable.sensor_readings.should_average_empty_vector_as_zero");
        Require(AvgSensorTemperature(Array.Empty<SensorReading>()) == 0.0, "avgSensorTemperature empty");

        // Construct a Location[] in C# and pass it to native code. Exercises
        // the param direction independently of the round-trip: if the CLR's
        // struct layout drifts from Rust's, SumRatings will see garbage.
        Location[] handmade = new[]
        {
            new Location(100L, 40.0, -70.0, 2.5, 5, true),
            new Location(101L, 40.5, -70.5, 4.0, 50, false),
        };
        DemoCase("case:records.blittable.locations.should_count_host_constructed_vector");
        Require(ProcessLocations(handmade) == 2, "processLocations handmade");
        DemoCase("case:records.blittable.locations.should_sum_host_constructed_ratings");
        Require(Math.Abs(SumRatings(handmade) - 6.5) < 1e-9, "sumRatings handmade");

        DemoCase("case:records.blittable.locations.find_location.should_return_some_for_positive_id");
        Location? foundLoc = FindLocation(1);
        Require(foundLoc.HasValue && foundLoc.Value.Id == 1L, "findLocation(1)");
        DemoCase("case:records.blittable.locations.find_location.should_return_none_for_non_positive_id");
        Require(!FindLocation(0).HasValue, "findLocation(0)");

        DemoCase("case:records.blittable.locations.find_locations.should_return_some_vector_for_positive_count");
        Location[] foundLocs = FindLocations(3);
        Require(foundLocs is not null && foundLocs.Length == 3, "findLocations(3)");
        DemoCase("case:records.blittable.locations.find_locations.should_return_none_for_non_positive_count");
        Require(FindLocations(0) is null, "findLocations(0)");

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Vec&lt;CStyleEnum&gt; and Vec&lt;DataEnum&gt; both ride the wire-encoded path:
    /// the Rust macro classifies C-style enums as Scalar (not Blittable),
    /// so Vec&lt;Status&gt; and Vec&lt;Direction&gt; cross the boundary the same
    /// way Vec&lt;Shape&gt; does — a length-prefixed encoded buffer. The
    /// C# side decodes with ReadEncodedArray&lt;T&gt; and per-element
    /// {Name}Wire.Decode or {Name}.Decode.
    /// </summary>
    private static void TestEnumVecs()
    {
        Console.WriteLine("Testing Vec<CStyleEnum> and Vec<DataEnum>...");

        Status[] statuses = new[] { Status.Active, Status.Inactive, Status.Pending, Status.Active };
        DemoCase("case:enums.c_style.status.should_roundtrip_vectors");
        Status[] echoedStatuses = EchoVecStatus(statuses);
        Require(echoedStatuses.SequenceEqual(statuses), "echoVecStatus round-trip");
        Require(EchoVecStatus(Array.Empty<Status>()).Length == 0, "echoVecStatus empty");

        DemoCase("case:enums.c_style.direction.should_generate_sequence");
        Direction[] generated = GenerateDirections(6);
        Require(generated.Length == 6, "generateDirections length");
        Require(generated[0] == Direction.North && generated[4] == Direction.North, "generateDirections wraps the 4-direction cycle");
        DemoCase("case:enums.c_style.direction.should_count_north_values");
        Require(CountNorth(generated) == 2, "countNorth on generateDirections(6)");
        Require(CountNorth(Array.Empty<Direction>()) == 0, "countNorth empty");

        LogLevel[] levels = new[] { LogLevel.Trace, LogLevel.Warn, LogLevel.Error, LogLevel.Debug };
        DemoCase("case:enums.repr_int.log_level.should_roundtrip_vectors");
        LogLevel[] echoedLevels = EchoVecLogLevel(levels);
        Require(echoedLevels.SequenceEqual(levels), "echoVecLogLevel round-trip");
        Require(EchoVecLogLevel(Array.Empty<LogLevel>()).Length == 0, "echoVecLogLevel empty");

        Shape[] shapes = new Shape[]
        {
            new Shape.Circle(2.5),
            new Shape.Rectangle(3.0, 4.0),
            new Shape.Triangle(new Point(0.0, 0.0), new Point(4.0, 0.0), new Point(0.0, 3.0)),
            new Shape.Point(),
            new Shape.Apex(new Point(7.0, 8.0)),
            new Shape.Apex(null),
        };
        DemoCase("case:enums.data_enum.shape.should_roundtrip_vectors");
        Shape[] echoedShapes = EchoVecShape(shapes);
        Require(echoedShapes.Length == shapes.Length, "echoVecShape length");
        Require(echoedShapes.SequenceEqual(shapes), "echoVecShape round-trip preserves each variant");
        Require(EchoVecShape(Array.Empty<Shape>()).Length == 0, "echoVecShape empty");

        // Cluster carries a `Point[]`, and C# record default equality treats
        // arrays by reference, so we compare element-wise explicitly.
        Point[] clusterPoints = new[] { new Point(1.0, 2.0), new Point(3.0, 4.0) };
        Shape[] clusterRoundTrip = EchoVecShape(new Shape[] { new Shape.Cluster(clusterPoints) });
        Require(
            clusterRoundTrip.Length == 1
                && clusterRoundTrip[0] is Shape.Cluster rc
                && rc.Members.SequenceEqual(clusterPoints),
            "echoVecShape(Cluster with Vec<Point>)"
        );

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Vec fields inside records and data-enum variants. Polygon.Points and
    /// Filter.ByPoints.Anchors ride the length-prefixed blittable path;
    /// Team.Members, Classroom.Students, Filter.ByTags.Tags,
    /// Filter.ByGroups.Groups, TaggedScores.Scores, and
    /// BenchmarkUserProfile.Tags/Scores mix the encoded and blittable
    /// paths inside the enclosing record's wire buffer. UTF-8 sentinels
    /// (café, 🌍) ride through any Vec&lt;String&gt; position to exercise
    /// 2-byte and 4-byte codepoints across the boundary.
    /// </summary>
    private static void TestVecFields()
    {
        Console.WriteLine("Testing Vec fields inside records and enum variants...");

        DemoCase("case:records.with_collections.polygon.should_roundtrip_point_vector");
        Polygon triangle = new Polygon(new[]
        {
            new Point(0.0, 0.0),
            new Point(4.0, 0.0),
            new Point(0.0, 3.0),
        });
        Polygon echoedTriangle = EchoPolygon(triangle);
        Require(echoedTriangle.Points.SequenceEqual(triangle.Points), "echoPolygon round-trip");
        DemoCase("case:records.with_collections.polygon.should_report_vertex_count");
        Require(PolygonVertexCount(triangle) == 3u, "polygonVertexCount");
        DemoCase("case:records.with_collections.polygon.should_compute_centroid");
        Point centroid = PolygonCentroid(triangle);
        Require(Math.Abs(centroid.X - 4.0 / 3.0) < 1e-9 && Math.Abs(centroid.Y - 1.0) < 1e-9, "polygonCentroid");
        DemoCase("case:records.with_collections.polygon.should_make_from_points");
        Polygon built = MakePolygon(triangle.Points);
        Require(built.Points.SequenceEqual(triangle.Points), "makePolygon");
        DemoCase("case:records.with_collections.polygon.should_roundtrip_point_vector");
        Require(EchoPolygon(new Polygon(Array.Empty<Point>())).Points.Length == 0, "echoPolygon empty");

        DemoCase("case:records.with_collections.team.should_roundtrip_member_vector");
        Team team = new Team("Alpha", new[] { "café", "🌍", "common" });
        Team echoedTeam = EchoTeam(team);
        Require(echoedTeam.Name == team.Name, "echoTeam name");
        Require(echoedTeam.Members.SequenceEqual(team.Members), "echoTeam members utf-8 round-trip");
        DemoCase("case:records.with_collections.team.should_report_member_count");
        Require(TeamSize(team) == 3u, "teamSize");
        DemoCase("case:records.with_collections.team.should_make_from_members");
        Team built2 = MakeTeam("Beta", new[] { "x", "y" });
        Require(built2.Name == "Beta" && built2.Members.SequenceEqual(new[] { "x", "y" }), "makeTeam");
        DemoCase("case:records.with_collections.team.should_roundtrip_member_vector");
        Require(EchoTeam(new Team("Empty", Array.Empty<string>())).Members.Length == 0, "echoTeam empty members");

        DemoCase("case:records.with_collections.classroom.should_roundtrip_student_vector");
        Classroom classroom = new Classroom(new[]
        {
            new Person("café", 7u),
            new Person("🌍", 42u),
        });
        Classroom echoedClass = EchoClassroom(classroom);
        Require(echoedClass.Students.SequenceEqual(classroom.Students), "echoClassroom utf-8 round-trip");
        DemoCase("case:records.with_collections.classroom.should_make_from_students");
        Classroom built3 = MakeClassroom(classroom.Students);
        Require(built3.Students.SequenceEqual(classroom.Students), "makeClassroom (Vec<NonBlittableRecord> param)");
        DemoCase("case:records.with_collections.classroom.should_roundtrip_student_vector");
        Require(EchoClassroom(new Classroom(Array.Empty<Person>())).Students.Length == 0, "echoClassroom empty");

        DemoCase("case:records.with_collections.tagged_scores.should_roundtrip_score_vector");
        TaggedScores scores = new TaggedScores("quiz", new[] { 10.0, 20.0, 30.0 });
        TaggedScores echoedScores = EchoTaggedScores(scores);
        Require(echoedScores.Label == "quiz" && echoedScores.Scores.SequenceEqual(scores.Scores), "echoTaggedScores");
        DemoCase("case:records.with_collections.tagged_scores.should_average_scores");
        Require(Math.Abs(AverageScore(scores) - 20.0) < 1e-9, "averageScore");
        Require(AverageScore(new TaggedScores("empty", Array.Empty<double>())) == 0.0, "averageScore empty");

        DemoCase("case:enums.complex_variants.filter.none.should_roundtrip_unit_variant");
        Filter noneFilter = new Filter.None();
        Require(EchoFilter(noneFilter) is Filter.None, "echoFilter None");

        Filter byName = new Filter.ByName("query");
        DemoCase("case:enums.complex_variants.filter.by_name.should_roundtrip_string_payload");
        Require(EchoFilter(byName) is Filter.ByName n && n.Name == "query", "echoFilter ByName");
        DemoCase("case:enums.complex_variants.filter.by_name.should_describe_string_payload");
        Require(DescribeFilter(byName) == "filter by name: query", "describeFilter ByName");

        Filter byRange = new Filter.ByRange(1.5, 9.0);
        DemoCase("case:enums.complex_variants.filter.by_range.should_describe_numeric_bounds");
        Require(DescribeFilter(byRange) == "filter by range: 1.5..9", "describeFilter ByRange");

        Filter byTags = new Filter.ByTags(new[] { "café", "🌍" });
        DemoCase("case:enums.complex_variants.filter.by_tags.should_roundtrip_string_vector_payload");
        Filter echoedTags = EchoFilter(byTags);
        Require(echoedTags is Filter.ByTags t && t.Tags.SequenceEqual(((Filter.ByTags)byTags).Tags), "echoFilter ByTags");
        DemoCase("case:enums.complex_variants.filter.by_tags.should_describe_string_vector_payload");
        Require(DescribeFilter(byTags) == "filter by 2 tags", "describeFilter ByTags");

        Filter byGroups = new Filter.ByGroups(
            new[]
            {
                new[] { "café", "🌍" },
                Array.Empty<string>(),
                new[] { "common" },
            }
        );
        DemoCase("case:enums.complex_variants.filter.by_groups.should_roundtrip_nested_string_vectors");
        Filter echoedGroups = EchoFilter(byGroups);
        Require(echoedGroups is Filter.ByGroups g && g.Groups.Length == 3, "echoFilter ByGroups outer length");
        Require(
            echoedGroups is Filter.ByGroups g0
                && g0.Groups[0].SequenceEqual(((Filter.ByGroups)byGroups).Groups[0])
                && g0.Groups[1].SequenceEqual(((Filter.ByGroups)byGroups).Groups[1])
                && g0.Groups[2].SequenceEqual(((Filter.ByGroups)byGroups).Groups[2]),
            "echoFilter ByGroups nested strings"
        );
        DemoCase("case:enums.complex_variants.filter.by_groups.should_describe_nested_string_vectors");
        Require(DescribeFilter(byGroups) == "filter by 3 groups", "describeFilter ByGroups");

        Filter byPoints = new Filter.ByPoints(new[] { new Point(1.0, 2.0), new Point(3.0, 4.0) });
        DemoCase("case:enums.complex_variants.filter.by_points.should_roundtrip_record_vector_payload");
        Filter echoedPts = EchoFilter(byPoints);
        Require(echoedPts is Filter.ByPoints p2 && p2.Anchors.SequenceEqual(((Filter.ByPoints)byPoints).Anchors), "echoFilter ByPoints");
        DemoCase("case:enums.complex_variants.filter.by_points.should_describe_record_vector_payload");
        Require(DescribeFilter(byPoints) == "filter by 2 anchor points", "describeFilter ByPoints");

        ApiResponse successResp = new ApiResponse.Success("payload");
        DemoCase("case:enums.complex_variants.api_response.success.should_roundtrip_string_payload");
        Require(EchoApiResponse(successResp) is ApiResponse.Success s && s.Data == "payload", "echoApiResponse Success");
        DemoCase("case:enums.complex_variants.api_response.success.should_identify_success");
        Require(IsSuccess(successResp), "isSuccess Success");

        ApiResponse redirectResp = new ApiResponse.Redirect("https://example.com/r");
        DemoCase("case:enums.complex_variants.api_response.redirect.should_roundtrip_url_payload");
        Require(EchoApiResponse(redirectResp) is ApiResponse.Redirect r2 && r2.Url == "https://example.com/r", "echoApiResponse Redirect");

        DemoCase("case:enums.complex_variants.api_response.empty.should_not_identify_as_success");
        Require(!IsSuccess(new ApiResponse.Empty()), "isSuccess Empty");

        DemoCase("case:records.with_collections.user_profiles.should_generate_profiles");
        BenchmarkUserProfile[] profiles = GenerateUserProfiles(4);
        Require(profiles.Length == 4, "generateUserProfiles length");
        Require(profiles[0].Tags.Length == 3 && profiles[0].Scores.Length == 3, "generateUserProfiles inner vec shapes");
        Require(profiles[0].IsActive && !profiles[1].IsActive, "generateUserProfiles is_active pattern");
        double expectedSum = 0.0 + 1.5 + 3.0 + 4.5;
        DemoCase("case:records.with_collections.user_profiles.should_sum_scores");
        Require(Math.Abs(SumUserScores(profiles) - expectedSum) < 1e-9, "sumUserScores round-trip");
        DemoCase("case:records.with_collections.user_profiles.should_count_active_users");
        Require(CountActiveUsers(profiles) == 2, "countActiveUsers (even indices active)");
        DemoCase("case:records.with_collections.user_profiles.should_sum_scores");
        Require(SumUserScores(Array.Empty<BenchmarkUserProfile>()) == 0.0, "sumUserScores empty");

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Option&lt;T&gt; travels wire-encoded: 1 byte for the present/absent tag,
    /// plus the inner payload when Some. The C# surface renders each
    /// Option as T? uniformly — Nullable&lt;T&gt; for value-type inners,
    /// nullable-annotated references for reference-type inners, both
    /// under #nullable enable in the generated files. Covers the
    /// primitive matrix plus reference-type inners (string), blittable
    /// records (Point), C-style enums (Status), and data enums
    /// (ApiResult). Option fields inside records and nested
    /// Option/Vec combinations land in a later step.
    /// </summary>
    private static void TestOptions()
    {
        Console.WriteLine("Testing Option types...");

        DemoCase("case:options.primitives.i32.should_roundtrip_some");
        Require(EchoOptionalI32(42) == 42, "EchoOptionalI32(Some)");
        DemoCase("case:options.primitives.i32.should_roundtrip_none");
        Require(EchoOptionalI32(null) == null, "EchoOptionalI32(None)");
        DemoCase("case:options.primitives.i32.should_roundtrip_some");
        Require(EchoOptionalI32(int.MinValue) == int.MinValue, "EchoOptionalI32(min)");
        Require(EchoOptionalI32(int.MaxValue) == int.MaxValue, "EchoOptionalI32(max)");

        DemoCase("case:options.primitives.f64.should_roundtrip_some");
        Require(EchoOptionalF64(3.14) == 3.14, "EchoOptionalF64(Some)");
        DemoCase("case:options.primitives.f64.should_roundtrip_none");
        Require(EchoOptionalF64(null) == null, "EchoOptionalF64(None)");

        DemoCase("case:options.primitives.bool.should_roundtrip_some");
        Require(EchoOptionalBool(true) == true, "EchoOptionalBool(true)");
        Require(EchoOptionalBool(false) == false, "EchoOptionalBool(false)");
        DemoCase("case:options.primitives.bool.should_roundtrip_none");
        Require(EchoOptionalBool(null) == null, "EchoOptionalBool(None)");

        DemoCase("case:options.primitives.i32.should_unwrap_some");
        Require(UnwrapOrDefaultI32(10, 99) == 10, "UnwrapOrDefaultI32(Some)");
        DemoCase("case:options.primitives.i32.should_use_default_for_none");
        Require(UnwrapOrDefaultI32(null, 99) == 99, "UnwrapOrDefaultI32(None) falls back");

        DemoCase("case:options.primitives.i32.should_make_some");
        Require(MakeSomeI32(7) == 7, "MakeSomeI32 returns Some");
        DemoCase("case:options.primitives.i32.should_make_none");
        Require(MakeNoneI32() == null, "MakeNoneI32 returns null");

        DemoCase("case:options.primitives.i32.should_double_some");
        Require(DoubleIfSome(5) == 10, "DoubleIfSome(Some)");
        DemoCase("case:options.primitives.i32.should_preserve_none_when_doubling");
        Require(DoubleIfSome(null) == null, "DoubleIfSome(None) stays None");

        DemoCase("case:options.primitives.i32.should_find_even_value");
        Require(FindEven(4) == 4, "FindEven(4) == Some(4)");
        DemoCase("case:options.primitives.i32.should_return_none_for_odd_value");
        Require(FindEven(3) == null, "FindEven(3) == None");

        DemoCase("case:options.primitives.i64.should_find_positive_value");
        Require(FindPositiveI64(100L) == 100L, "FindPositiveI64(100)");
        DemoCase("case:options.primitives.i64.should_return_none_for_non_positive_value");
        Require(FindPositiveI64(-1L) == null, "FindPositiveI64(-1) == None");
        Require(FindPositiveI64(0L) == null, "FindPositiveI64(0) == None");

        DemoCase("case:options.primitives.f64.should_find_positive_value");
        Require(FindPositiveF64(1.5) == 1.5, "FindPositiveF64(1.5)");
        DemoCase("case:options.primitives.f64.should_return_none_for_non_positive_value");
        Require(FindPositiveF64(-0.5) == null, "FindPositiveF64(-0.5) == None");

        // Option<String>: reference-type inner rides the same 1-byte tag
        // path; the payload is a length-prefixed UTF-8 buffer. café
        // exercises 2-byte codepoints, 🌍 exercises 4-byte ones.
        DemoCase("case:options.complex.string.should_roundtrip_some");
        Require(EchoOptionalString("hello") == "hello", "EchoOptionalString(Some ascii)");
        Require(EchoOptionalString("café") == "café", "EchoOptionalString(2-byte UTF-8)");
        Require(EchoOptionalString("🌍") == "🌍", "EchoOptionalString(4-byte UTF-8)");
        Require(EchoOptionalString("") == "", "EchoOptionalString(empty Some)");
        DemoCase("case:options.complex.string.should_roundtrip_none");
        Require(EchoOptionalString(null) == null, "EchoOptionalString(None)");

        DemoCase("case:options.complex.string.should_report_some");
        Require(IsSomeString("x"), "IsSomeString(Some)");
        DemoCase("case:options.complex.string.should_report_none");
        Require(!IsSomeString(null), "IsSomeString(None)");

        DemoCase("case:options.complex.string.should_find_name_for_positive_id");
        Require(FindName(7) == "Name_7", "FindName(positive) returns Some");
        DemoCase("case:options.complex.string.should_return_none_for_non_positive_id");
        Require(FindName(-1) == null, "FindName(non-positive) returns null");

        // Option<BlittableRecord>: Point is #[repr(C)] with two f64
        // fields, so the inner payload is 16 raw bytes written via
        // Point.WireEncodeTo and read via Point.Decode — no layout
        // shortcut, because the 1-byte tag forces the wire path.
        DemoCase("case:options.complex.point.should_roundtrip_some");
        Require(EchoOptionalPoint(new Point(1.5, 2.5)) == new Point(1.5, 2.5), "EchoOptionalPoint(Some)");
        DemoCase("case:options.complex.point.should_roundtrip_none");
        Require(EchoOptionalPoint(null) == null, "EchoOptionalPoint(None)");

        DemoCase("case:options.complex.point.should_make_some");
        Require(MakeSomePoint(3.0, 4.0) == new Point(3.0, 4.0), "MakeSomePoint returns Some");
        DemoCase("case:options.complex.point.should_make_none");
        Require(MakeNonePoint() == null, "MakeNonePoint returns null");

        // Option<CStyleEnum>: Status crosses the wire as a 4-byte i32
        // tag under an Option — the CLR can't reuse its direct
        // marshaling path because of the outer 1-byte present tag.
        DemoCase("case:options.complex.status.should_roundtrip_some");
        Require(EchoOptionalStatus(Status.Active) == Status.Active, "EchoOptionalStatus(Active)");
        Require(EchoOptionalStatus(Status.Pending) == Status.Pending, "EchoOptionalStatus(Pending)");
        DemoCase("case:options.complex.status.should_roundtrip_none");
        Require(EchoOptionalStatus(null) == null, "EchoOptionalStatus(None)");

        // Option<DataEnum>: ApiResult has unit, tuple, and struct
        // variants — the decode inside the Option's ternary must
        // still dispatch to the right variant.
        DemoCase("case:options.complex.api_result.should_find_success_variant");
        Require(
            FindApiResult(0) is ApiResult.Success,
            "FindApiResult(0) returns Success"
        );
        DemoCase("case:options.complex.api_result.should_find_error_code_variant");
        Require(
            FindApiResult(1) is ApiResult.ErrorCode ec && ec.Value0 == -1,
            "FindApiResult(1) returns ErrorCode(-1)"
        );
        DemoCase("case:options.complex.api_result.should_find_error_with_data_variant");
        Require(
            FindApiResult(2) is ApiResult.ErrorWithData ewd && ewd.Code == -1 && ewd.Detail == -2,
            "FindApiResult(2) returns ErrorWithData"
        );
        DemoCase("case:options.complex.api_result.should_return_none_for_unknown_code");
        Require(FindApiResult(9) == null, "FindApiResult(unknown) returns null");

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Records whose fields are themselves Option&lt;T&gt;. Exercises the
    /// shared-emit-context plumbing: two Option fields on one record
    /// must each pick fresh `sizeOpt{n}` / `opt{n}` pattern-binding
    /// names so the sum inside `WireEncodedSize` and the statements
    /// inside `WireEncodeTo` don't redeclare the same local. The
    /// Decode path reads each Option through the same tag-and-branch
    /// pattern used for top-level Option returns.
    /// </summary>
    private static void TestOptionsInRecords()
    {
        Console.WriteLine("Testing records with Option fields...");

        // UserProfile: one optional string field, one optional f64.
        // The record round-trip exercises encode + decode together.
        DemoCase("case:records.with_options.user_profile.should_make_with_present_options");
        UserProfile alice = MakeUserProfile("Alice", 30u, "alice@example.com", 92.5);
        Require(alice.Name == "Alice", "MakeUserProfile.Name");
        Require(alice.Age == 30u, "MakeUserProfile.Age");
        Require(alice.Email == "alice@example.com", "MakeUserProfile.Email(Some)");
        Require(alice.Score == 92.5, "MakeUserProfile.Score(Some)");

        DemoCase("case:records.with_options.user_profile.should_make_with_absent_options");
        UserProfile newUser = MakeUserProfile("Bob", 25u, null, null);
        Require(newUser.Email == null, "MakeUserProfile.Email(None)");
        Require(newUser.Score == null, "MakeUserProfile.Score(None)");

        DemoCase("case:records.with_options.user_profile.should_roundtrip_present_options");
        UserProfile echoed = EchoUserProfile(alice);
        Require(echoed == alice, "EchoUserProfile round-trip (all fields Some)");

        DemoCase("case:records.with_options.user_profile.should_roundtrip_absent_options");
        UserProfile echoedNew = EchoUserProfile(newUser);
        Require(echoedNew == newUser, "EchoUserProfile round-trip (Option fields None)");

        // Mixed present/absent: one Option field is Some, the other is None.
        UserProfile mixed = MakeUserProfile("Carol", 40u, "carol@example.com", null);
        Require(mixed.Email == "carol@example.com", "MakeUserProfile.Email(Some) with Score(None)");
        Require(mixed.Score == null, "MakeUserProfile.Score(None) with Email(Some)");
        DemoCase("case:records.with_options.user_profile.should_roundtrip_mixed_options");
        Require(EchoUserProfile(mixed) == mixed, "EchoUserProfile round-trip (mixed Option fields)");

        // UTF-8 sentinels inside the optional string field.
        UserProfile emoji = MakeUserProfile("🌍 User", 42u, "café@example.com", 3.14);
        DemoCase("case:records.with_options.user_profile.should_roundtrip_utf8_optional_string");
        UserProfile echoedEmoji = EchoUserProfile(emoji);
        Require(echoedEmoji == emoji, "EchoUserProfile round-trip (UTF-8 in Option fields)");

        DemoCase("case:records.with_options.user_profile.should_display_email_when_present");
        Require(
            UserDisplayName(alice) == "Alice <alice@example.com>",
            "UserDisplayName when Email is Some"
        );
        DemoCase("case:records.with_options.user_profile.should_display_name_when_email_absent");
        Require(UserDisplayName(newUser) == "Bob", "UserDisplayName when Email is None");

        // SearchResult: second record shape with Option fields, exercises
        // the same code path through a different record class name to
        // catch any accidental per-record coupling in the generator.
        SearchResult hits = new SearchResult("cats", 42u, "cursor_abc", 0.97);
        DemoCase("case:records.with_options.search_result.should_roundtrip_present_options");
        Require(EchoSearchResult(hits) == hits, "EchoSearchResult round-trip (all Some)");
        DemoCase("case:records.with_options.search_result.should_report_more_results_when_cursor_present");
        Require(HasMoreResults(hits), "HasMoreResults true when NextCursor is Some");

        SearchResult tail = new SearchResult("cats", 42u, null, null);
        DemoCase("case:records.with_options.search_result.should_roundtrip_absent_options");
        Require(EchoSearchResult(tail) == tail, "EchoSearchResult round-trip (Option fields None)");
        DemoCase("case:records.with_options.search_result.should_report_no_more_results_without_cursor");
        Require(!HasMoreResults(tail), "HasMoreResults false when NextCursor is None");

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Option composed with Vec in both directions. `Option&lt;Vec&lt;T&gt;&gt;`
    /// wraps the entire array in the 1-byte tag; `Vec&lt;Option&lt;T&gt;&gt;`
    /// writes the count, then a tag per element. Both ride the
    /// encoded-array path on the wire because the element width varies.
    /// </summary>
    private static void TestOptionsWithVec()
    {
        Console.WriteLine("Testing Option composed with Vec...");

        // Option<Vec<T>>: the Option tag guards an entire length-prefixed
        // array. Some(vec) and Some(empty_vec) are distinct from None.
        DemoCase("case:options.complex.vec.should_roundtrip_some");
        var numbers = EchoOptionalVec(new[] { 1, 2, 3 });
        Require(numbers != null && numbers.SequenceEqual(new[] { 1, 2, 3 }), "EchoOptionalVec(Some)");
        DemoCase("case:options.complex.vec.should_roundtrip_empty_some");
        Require(
            EchoOptionalVec(Array.Empty<int>())!.Length == 0,
            "EchoOptionalVec(Some empty) stays Some"
        );
        DemoCase("case:options.complex.vec.should_roundtrip_none");
        Require(EchoOptionalVec(null) == null, "EchoOptionalVec(None)");

        DemoCase("case:options.complex.vec.should_report_length_for_some");
        Require(OptionalVecLength(new[] { 10, 20, 30 }) == 3u, "OptionalVecLength(Some)");
        DemoCase("case:options.complex.vec.should_return_none_for_absent_length");
        Require(OptionalVecLength(null) == null, "OptionalVecLength(None)");

        // Option<Vec<_>>-returning functions: the wire return is
        // FfiBuf, decoded through ReadU8() + ReadLengthPrefixedBlittableArray
        // (primitive elements) or ReadEncodedArray (variable-width).
        DemoCase("case:options.complex.vec.should_find_numbers_for_positive_count");
        Require(
            FindNumbers(3)!.SequenceEqual(new[] { 0, 1, 2 }),
            "FindNumbers(positive) returns Some(vec)"
        );
        DemoCase("case:options.complex.vec.should_return_none_for_non_positive_number_count");
        Require(FindNumbers(-1) == null, "FindNumbers(non-positive) returns null");

        var names = FindNames(3);
        DemoCase("case:options.complex.vec_string.should_find_names_for_positive_count");
        Require(
            names != null && names.SequenceEqual(new[] { "Name_0", "Name_1", "Name_2" }),
            "FindNames(positive) returns Some(vec of strings)"
        );
        DemoCase("case:options.complex.vec_string.should_return_none_for_non_positive_name_count");
        Require(FindNames(0) == null, "FindNames(zero) returns null");

        // Vec<Option<T>>: new fixture. Each element carries its own
        // Option tag, so the wire shape is: count (i32), then for each
        // slot, 1-byte tag + optional i32 payload. Mixed Some/None
        // positions in one vec surface any off-by-one errors.
        DemoCase("case:options.complex.vec_optional_i32.should_roundtrip_mixed_presence");
        int?[] mixed = new int?[] { 1, null, 3, null, 5 };
        int?[] echoed = EchoVecOptionalI32(mixed);
        Require(echoed.Length == mixed.Length, "EchoVecOptionalI32 preserves length");
        for (int i = 0; i < mixed.Length; i++)
        {
            Require(echoed[i] == mixed[i], $"EchoVecOptionalI32[{i}] preserves presence and value");
        }

        DemoCase("case:options.complex.vec_optional_i32.should_roundtrip_empty");
        Require(EchoVecOptionalI32(Array.Empty<int?>()).Length == 0, "EchoVecOptionalI32 empty");
        DemoCase("case:options.complex.vec_optional_i32.should_roundtrip_all_none");
        Require(
            EchoVecOptionalI32(new int?[] { null, null, null }).All(v => v == null),
            "EchoVecOptionalI32 all-None preserved"
        );
        Require(
            EchoVecOptionalI32(new int?[] { 10, 20, 30 }).SequenceEqual(new int?[] { 10, 20, 30 }),
            "EchoVecOptionalI32 all-Some preserved"
        );

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Class wrappers. Each construct is an IntPtr handle to a Rust
    /// allocation; methods forward through that handle. The test
    /// covers:
    ///
    /// - Constructors: Default, NamedInit, named factories, with
    ///   parameter shapes ranging from no-args to wire-encoded records,
    ///   pinned blittable-record arrays, and data enums.
    /// - Instance methods: void, primitive return, Option return,
    ///   blittable-record return, string param + bool return,
    ///   Vec&lt;String&gt; return.
    /// - Static methods on a class: primitives, blittable records,
    ///   Option return.
    /// - Dispose: every using block forces the wrapper to hand its
    ///   IntPtr back to Rust through the matching `_free` symbol, so a
    ///   leaked or double-freed handle would surface as a segfault or
    ///   an allocator panic.
    /// </summary>
    private static void TestClasses()
    {
        Console.WriteLine("Testing class wrappers (constructors + methods)...");

        // Inventory.new() lifts to a parameterless C# instance ctor.
        // The using block forces Dispose() to run, which hands the
        // IntPtr back to Rust through boltffi_inventory_free.
        using (var inv = new Inventory())
        {
            Require(inv.Capacity() == 100u, "Inventory().Capacity() defaults to 100");
            Require(inv.Count() == 0u, "Inventory().Count() starts at 0");
            Require(inv.Add("apple"), "Inventory.Add(\"apple\") returns true under capacity");
            Require(inv.Add("banana"), "Inventory.Add(\"banana\") returns true");
            Require(inv.Count() == 2u, "Inventory.Count() reflects two adds");

            // Vec<String> return decodes via ReadEncodedArray; UTF-8
            // round-trips for both ascii and emoji.
            string[] all = inv.GetAll();
            Require(all.SequenceEqual(new[] { "apple", "banana" }), "Inventory.GetAll round-trips Vec<String>");

            // Option<String> return: Some path then None path.
            Require(inv.Remove(0) == "apple", "Inventory.Remove(0) returns Some(item)");
            Require(inv.Remove(99) == null, "Inventory.Remove(out-of-range) returns null");
            Require(inv.Count() == 1u, "Inventory.Count() decremented after remove");
        }

        // Inventory.with_capacity(u32) is a NamedInit constructor on
        // the Rust side, which the C# backend lifts to a static
        // factory rather than a second instance constructor.
        using (var inv = Inventory.WithCapacity(2))
        {
            Require(inv.Capacity() == 2u, "Inventory.WithCapacity(2).Capacity()");
            Require(inv.Add("first"), "Add up to capacity");
            Require(inv.Add("second"), "Add to fill");
            Require(!inv.Add("third"), "Add past capacity returns false");
        }

        DemoCase("case:classes.constructors.inventory.try_new.should_return_inventory_for_positive_capacity");
        using (var inv = Inventory.TryNew(1))
        {
            Require(inv.Capacity() == 1u, "Inventory.TryNew(1).Capacity()");
            Require(inv.Count() == 0u, "Inventory.TryNew(1).Count() starts at 0");
        }

        DemoCase("case:classes.constructors.inventory.try_new.should_reject_zero_capacity");
        try
        {
            using var unexpected = Inventory.TryNew(0);
            Require(false, "Inventory.TryNew(0) should throw");
        }
        catch (BoltException e)
        {
            Require(e.Message.Contains("capacity must be greater than zero"), "Inventory.TryNew(0) error message");
        }

        // Counter exercises every method-return shape that lands in
        // this PR: primitive direct, void mutator, Option<primitive>
        // through FfiBuf, and a blittable record return.
        using (var counter = new Counter(7))
        {
            Require(counter.Get() == 7, "new Counter(7).Get()");
            counter.Increment();
            Require(counter.Get() == 8, "Counter.Increment then Get");
            counter.Add(10);
            Require(counter.Get() == 18, "Counter.Add(10) then Get");
            Require(counter.MaybeDouble() == 36, "Counter.MaybeDouble() returns Some when nonzero");
            counter.Reset();
            Require(counter.Get() == 0, "Counter.Reset zeros the value");
            Require(counter.MaybeDouble() == null, "Counter.MaybeDouble() returns null when zero");
            counter.Add(3);
            Point p = counter.AsPoint();
            Require(p.X == 3.0 && p.Y == 0.0, "Counter.AsPoint() returns blittable Point");
        }

        // SharedCounter pairs a void Set with Increment / Add that
        // mutate state and return the new value in the same call. The
        // mutate-then-return shape isn't covered elsewhere in this
        // method.
        using (var shared = new SharedCounter(0))
        {
            shared.Set(10);
            Require(shared.Get() == 10, "SharedCounter.Set then Get");
            Require(shared.Increment() == 11, "SharedCounter.Increment returns new value");
            Require(shared.Get() == 11, "SharedCounter.Get reflects increment");
            Require(shared.Add(4) == 15, "SharedCounter.Add returns new value");
            Require(shared.Get() == 15, "SharedCounter.Get reflects add");
        }

        // MathUtils exercises class static methods. Add, Clamp,
        // DistanceBetween, Midpoint, and SafeSqrt have no `self` and
        // render as `public static` on the wrapper class itself. Round
        // is the only `&self` method. The bare integer literal `new
        // MathUtils(2)` (no `u` suffix) is the regression case the
        // private handle-adopting ctor exists to defend: without it,
        // overload resolution would pick the `IntPtr` ctor here.
        using (var mu = new MathUtils(2))
        {
            Require(Math.Abs(mu.Round(3.14159) - 3.14) < 1e-9, "MathUtils(2).Round(3.14159)");
        }
        Require(MathUtils.Add(2, 3) == 5, "MathUtils.Add static");
        Require(Math.Abs(MathUtils.Clamp(15.0, 0.0, 10.0) - 10.0) < 1e-9, "MathUtils.Clamp upper bound");
        Require(Math.Abs(MathUtils.Clamp(-1.0, 0.0, 10.0)) < 1e-9, "MathUtils.Clamp lower bound");
        Require(
            Math.Abs(MathUtils.DistanceBetween(new Point(0.0, 0.0), new Point(3.0, 4.0)) - 5.0) < 1e-9,
            "MathUtils.DistanceBetween 3-4-5"
        );
        Point mid = MathUtils.Midpoint(new Point(0.0, 0.0), new Point(2.0, 4.0));
        Require(mid.X == 1.0 && mid.Y == 2.0, "MathUtils.Midpoint blittable record return");
        Require(Math.Abs(MathUtils.SafeSqrt(16.0)!.Value - 4.0) < 1e-9, "MathUtils.SafeSqrt(16) Some");
        Require(MathUtils.SafeSqrt(-1.0) == null, "MathUtils.SafeSqrt(-1) None");

        // Constructing several instances back to back exercises the
        // Rust allocator path; if Box::into_raw or Box::from_raw were
        // mis-ordered this would surface as a segfault or a leak.
        for (int i = 0; i < 100; i++)
        {
            using var counter = new Counter(i);
            Require(counter.Get() == i, $"new Counter({i}).Get() iteration");
        }

        // Constructor parameter shapes the simple Inventory/Counter
        // matrix doesn't reach.

        // Primary with a string param: drives Encoding.UTF8.GetBytes
        // setup inside the private static helper, distinct from the
        // static factory body that the same string-param shape would
        // hit.
        using (var worker = new AsyncWorker("hello"))
        {
            // GetPrefix is the sync method on AsyncWorker. The async
            // methods are exercised in TestAsyncClassMethods.
            Require(worker.GetPrefix() == "hello", "AsyncWorker.GetPrefix round-trips ctor arg");
        }
        // StateHolder drives `&mut self` mutators end to end: a primary
        // ctor that takes a string, then set / increment / add_item /
        // remove_last / clear in sequence, with `&self` getters
        // observing the mutations.
        using (var holder = new StateHolder("snapshot"))
        {
            Require(holder.GetLabel() == "snapshot", "StateHolder.GetLabel returns ctor arg");
            Require(holder.GetValue() == 0, "StateHolder default value is 0");
            holder.SetValue(42);
            Require(holder.GetValue() == 42, "StateHolder.SetValue then GetValue");
            Require(holder.Increment() == 43, "StateHolder.Increment returns new value");
            Require(holder.GetValue() == 43, "StateHolder.GetValue reflects increment");
            holder.AddItem("alpha");
            holder.AddItem("beta");
            holder.AddItem("gamma");
            Require(holder.ItemCount() == 3u, "StateHolder.ItemCount after three adds");
            Require(
                holder.GetItems().SequenceEqual(new[] { "alpha", "beta", "gamma" }),
                "StateHolder.GetItems round-trips Vec<String>"
            );
            Require(holder.RemoveLast() == "gamma", "StateHolder.RemoveLast returns Some(last)");
            Require(holder.ItemCount() == 2u, "StateHolder.ItemCount decremented after pop");
            holder.Clear();
            Require(holder.GetValue() == 0, "StateHolder.Clear resets value");
            Require(holder.ItemCount() == 0u, "StateHolder.Clear empties items");
            Require(holder.RemoveLast() == null, "StateHolder.RemoveLast returns null on empty");
        }

        // MixedRecordService drives an instance method that takes a
        // wire-encoded record (echo_record) and one that takes the
        // record's parts as separate args (store_record_parts). Both
        // are `&self` methods returning a wire-encoded MixedRecord.
        using (var svc = new MixedRecordService("svc"))
        {
            Require(svc.GetLabel() == "svc", "MixedRecordService.GetLabel");
            Require(svc.StoredCount() == 0u, "MixedRecordService.StoredCount starts at 0");

            MixedRecordParameters parameters = new MixedRecordParameters(
                new[] { "alpha", "beta" },
                new[] { new Point(0.0, 0.0), new Point(1.0, 1.0) },
                new Point(2.0, 3.0),
                5u,
                true
            );
            MixedRecord record = new MixedRecord(
                "demo",
                new Point(1.0, 2.0),
                Priority.High,
                new Shape.Rectangle(3.0, 4.0),
                parameters
            );

            DemoCase("case:records.mixed.should_roundtrip_composed_record");
            MixedRecord freeEchoed = EchoMixedRecord(record);
            Require(freeEchoed.Name == "demo" && freeEchoed.Priority == Priority.High, "EchoMixedRecord free fn");

            DemoCase("case:records.mixed.should_make_from_composed_parts");
            MixedRecord freeMade = MakeMixedRecord("free-made", new Point(7.0, 8.0), Priority.Medium,
                new Shape.Circle(1.0), parameters);
            Require(freeMade.Name == "free-made" && freeMade.Anchor == new Point(7.0, 8.0)
                && freeMade.Priority == Priority.Medium, "MakeMixedRecord free fn");

            MixedRecord echoed = svc.EchoRecord(record);
            Require(echoed.Name == "demo", "EchoRecord round-trips Name");
            Require(echoed.Anchor.X == 1.0 && echoed.Anchor.Y == 2.0, "EchoRecord round-trips Anchor");
            Require(echoed.Priority == Priority.High, "EchoRecord round-trips Priority");
            Require(echoed.Shape is Shape.Rectangle echoedRect && echoedRect.Width == 3.0 && echoedRect.Height == 4.0,
                "EchoRecord round-trips Shape variant");
            Require(echoed.Parameters.Tags.SequenceEqual(parameters.Tags), "EchoRecord round-trips Parameters.Tags");
            Require(echoed.Parameters.MaxRetries == 5u, "EchoRecord round-trips Parameters.MaxRetries");
            Require(svc.StoredCount() == 0u, "EchoRecord does not bump StoredCount");

            MixedRecord stored = svc.StoreRecordParts(
                "stored",
                new Point(5.0, 6.0),
                Priority.Critical,
                new Shape.Circle(2.5),
                parameters
            );
            Require(stored.Name == "stored", "StoreRecordParts.Name");
            Require(stored.Anchor.X == 5.0 && stored.Anchor.Y == 6.0, "StoreRecordParts.Anchor");
            Require(stored.Priority == Priority.Critical, "StoreRecordParts.Priority");
            Require(stored.Shape is Shape.Circle storedCircle && storedCircle.Radius == 2.5,
                "StoreRecordParts.Shape Circle round-trip");
            Require(svc.StoredCount() == 1u, "StoreRecordParts increments StoredCount");
        }

        // No-arg static factory.
        using (var ds = DataStore.WithSampleData())
        {
            Require(ds != null, "DataStore.WithSampleData()");
        }
        // Mixed primitive static factory.
        using (var ds = DataStore.WithInitialPoint(1.5, 2.5, 1234L))
        {
            Require(ds != null, "DataStore.WithInitialPoint(double, double, long)");
        }

        // DataStore exercises an `&self` method taking a blittable
        // record (Add(DataPoint)) alongside a parts-flavored mutator
        // (AddParts) and the read-only Sum / Len / IsEmpty trio.
        using (var ds = new DataStore())
        {
            Require(ds.IsEmpty(), "new DataStore.IsEmpty");
            Require(ds.Len() == (nuint)0, "new DataStore.Len starts at 0");
            ds.Add(new DataPoint(1.0, 2.0, 100L));
            ds.Add(new DataPoint(3.0, 4.0, 200L));
            ds.AddParts(5.0, 6.0, 300L);
            Require(!ds.IsEmpty(), "DataStore.IsEmpty false after adds");
            Require(ds.Len() == (nuint)3, "DataStore.Len after three adds");
            Require(Math.Abs(ds.Sum() - 21.0) < 1e-9, "DataStore.Sum across three points");
        }

        // Static factory with primitive + bool + C-style enum:
        // exercises the `[MarshalAs(I1)]` bool path and the direct-
        // pass enum.
        using (var m = ConstructorCoverageMatrix.WithScalarMix(7u, true, Priority.High))
        {
            Require(m != null, "ConstructorCoverageMatrix.WithScalarMix(uint, bool, Priority)");
        }

        // Static factory with string + byte[]: two length-prefixed
        // args back to back.
        using (var m = ConstructorCoverageMatrix.WithStringAndBytes("label", new byte[] { 1, 2, 3 }))
        {
            Require(m.Summary() == "label=label;bytes=3", "WithStringAndBytes.Summary");
            Require(m.PayloadChecksum() == 6u, "WithStringAndBytes.PayloadChecksum (1+2+3)");
            Require(m.VectorCount() == 3u, "WithStringAndBytes.VectorCount");
        }

        // Static factory with blittable + non-blittable record:
        // direct-struct + WireEncoded paths in one call.
        using (var m = ConstructorCoverageMatrix.WithBlittableAndRecord(
            new Point(1.5, 2.5),
            new Person("Alice", 30u)))
        {
            Require(m.Summary() == "origin=1.5:2.5;person=Alice#30", "WithBlittableAndRecord.Summary");
            Require(m.PayloadChecksum() == 0u, "WithBlittableAndRecord.PayloadChecksum");
            Require(m.VectorCount() == 1u, "WithBlittableAndRecord.VectorCount");
        }

        // Static factory with `Vec<string>` + `Vec<Point>` + record:
        // the only test that drives the new `unsafe { fixed }`
        // scaffolding end to end (`Point[]` is a pinned-array param).
        using (var m = ConstructorCoverageMatrix.WithVectorsAndPolygon(
            new[] { "café", "🌍" },
            new[] { new Point(1.0, 2.0), new Point(3.0, 4.0) },
            new Polygon(new[] { new Point(0.0, 0.0), new Point(1.0, 1.0) })))
        {
            Require(m.Summary() == "tags=café|🌍;anchors=2;polygon=2", "WithVectorsAndPolygon.Summary");
            Require(m.PayloadChecksum() == 0u, "WithVectorsAndPolygon.PayloadChecksum");
            Require(m.VectorCount() == 6u, "WithVectorsAndPolygon.VectorCount (tags 2 + anchors 2 + polygon 2)");
        }

        // Static factory with three back-to-back wire-encoded records.
        using (var m = ConstructorCoverageMatrix.WithCollectionRecords(
            new Team("Alpha", new[] { "a", "b" }),
            new Classroom(new[] { new Person("p", 1u) }),
            new Polygon(new[] { new Point(0.0, 0.0) })))
        {
            Require(m.Summary() == "team=Alpha;members=2;students=1;polygon=1", "WithCollectionRecords.Summary");
            Require(m.PayloadChecksum() == 0u, "WithCollectionRecords.PayloadChecksum");
            Require(m.VectorCount() == 4u, "WithCollectionRecords.VectorCount (members 2 + students 1 + polygon 1)");
        }

        // Static factory with `Option<wire-encoded record>` +
        // `Option<string>` parameters: drives both the Some path and
        // the None path through the same setup machinery.
        using (var m = ConstructorCoverageMatrix.WithOptionalProfileAndCursor(
            new UserProfile("John", 29u, "john@example.com", 9.5),
            "cursor-7"))
        {
            Require(m.ConstructorVariant() == "with_optional_profile_and_cursor",
                "WithOptionalProfileAndCursor.ConstructorVariant");
            Require(m.Summary() == "profile=John#29#john@example.com#9.5;cursor=cursor-7",
                "WithOptionalProfileAndCursor.Summary (Some/Some)");
            Require(m.PayloadChecksum() == 0u, "WithOptionalProfileAndCursor.PayloadChecksum");
            Require(m.VectorCount() == 2u, "WithOptionalProfileAndCursor.VectorCount (Some/Some)");
        }
        using (var m = ConstructorCoverageMatrix.WithOptionalProfileAndCursor(null, null))
        {
            Require(m.Summary() == "profile=none;cursor=none",
                "WithOptionalProfileAndCursor.Summary (None/None)");
            Require(m.VectorCount() == 0u, "WithOptionalProfileAndCursor.VectorCount (None/None)");
        }

        // Seven-arg kitchen-sink ctor: stresses multiple wire writers,
        // setup-only declarations (string, byte[]), and a string array
        // back-to-back in one body.
        using (var m = ConstructorCoverageMatrix.WithEverything(
            new Person("Alice", 31u),
            new Address("Main", "AMS", "1000"),
            new UserProfile("John", 29u, "john@example.com", 9.5),
            new SearchResult("route", 5u, "next-9", 7.5),
            new byte[] { 4, 5, 6 },
            new Filter.ByRange(1.0, 3.0),
            new[] { "alpha", "beta" }))
        {
            Require(m.ConstructorVariant() == "with_everything", "WithEverything.ConstructorVariant");
            Require(
                m.Summary() == "person=Alice#31;city=AMS;profile=profile=John#29#john@example.com#9.5;query=route;filter=range:1.0-3.0;tags=alpha|beta",
                "WithEverything.Summary"
            );
            Require(m.PayloadChecksum() == 15u, "WithEverything.PayloadChecksum (4+5+6)");
            Require(m.VectorCount() == 10u, "WithEverything.VectorCount (tags 2 + payload 3 + total 5)");

            // SummarizeBorrowedInputs is the only method whose params
            // are all &Reference to non-blittable types. Lower drops the
            // references and treats them as wire-encoded; without these
            // assertions that path goes unverified. Cover both the
            // Some/Some Option path and the None/None path through the
            // same setup machinery.
            Require(
                m.SummarizeBorrowedInputs(
                    new UserProfile("John", 29u, "john@example.com", 9.5),
                    new SearchResult("route", 5u, "next-9", 7.5),
                    new Filter.ByRange(1.0, 3.0))
                    == "profile=John#29#john@example.com#9.5;query=route;filter=range:1.0-3.0",
                "SummarizeBorrowedInputs (Some options + Filter.ByRange)"
            );
            Require(
                m.SummarizeBorrowedInputs(
                    new UserProfile("Jane", 25u, null, null),
                    new SearchResult("foo", 0u, null, null),
                    new Filter.None())
                    == "profile=Jane#25#none#none;query=foo;filter=none",
                "SummarizeBorrowedInputs (None options + Filter.None)"
            );
        }

        // Static factory with two data enums + one record.
        using (var m = ConstructorCoverageMatrix.WithEnumMix(
            new Filter.ByName("query"),
            new Message.Text("hello"),
            new global::Demo.Task("title", Priority.Low, false)))
        {
            Require(m.Summary() == "filter=name:query;message=text:hello;task=title#low", "WithEnumMix.Summary");
            Require(m.PayloadChecksum() == 0u, "WithEnumMix.PayloadChecksum");
            Require(m.VectorCount() == 1u, "WithEnumMix.VectorCount");
        }

        Console.WriteLine("  PASS\n");
    }

    private static void TestResultFunctions()
    {
        Console.WriteLine("Testing result functions (String error)...");

        // Result<i32, String> ok path returns the value directly.
        DemoCase("case:results.basic.safe_divide.should_return_quotient");
        Require(SafeDivide(10, 2) == 5, "SafeDivide(10, 2) returns 5");
        // Err path throws BoltException carrying the Rust error string.
        DemoCase("case:results.basic.safe_divide.should_reject_division_by_zero");
        try
        {
            SafeDivide(10, 0);
            Require(false, "SafeDivide(10, 0) should throw");
        }
        catch (BoltException e)
        {
            Require(e.Message.Contains("division by zero"), "SafeDivide error message");
        }

        DemoCase("case:results.basic.divide.should_return_quotient");
        Require(Divide(20, 4) == 5, "Divide(20, 4) == 5");
        DemoCase("case:results.basic.divide.should_reject_division_by_zero");
        try
        {
            Divide(1, 0);
            Require(false, "Divide(1, 0) should throw");
        }
        catch (BoltException) { }

        DemoCase("case:results.basic.parse_int.should_parse_integer");
        Require(ParseInt("42") == 42, "ParseInt(42)");
        DemoCase("case:results.basic.parse_int.should_reject_invalid_integer");
        try
        {
            ParseInt("not a number");
            Require(false, "ParseInt(bad) should throw");
        }
        catch (BoltException) { }

        DemoCase("case:results.basic.safe_sqrt.should_return_square_root");
        Require(Math.Abs(SafeSqrt(9.0) - 3.0) < 1e-9, "SafeSqrt(9)");
        DemoCase("case:results.basic.safe_sqrt.should_reject_negative_input");
        try
        {
            SafeSqrt(-1.0);
            Require(false, "SafeSqrt(-1) should throw");
        }
        catch (BoltException) { }

        DemoCase("case:results.basic.validate_name.should_greet_valid_name");
        Require(ValidateName("Ada") == "Hello, Ada!", "ValidateName(Ada)");
        DemoCase("case:results.basic.validate_name.should_reject_empty_name");
        try
        {
            ValidateName("");
            Require(false, "ValidateName(empty) should throw");
        }
        catch (BoltException) { }

        DemoCase("case:results.basic.always_ok.should_return_doubled_value");
        Require(AlwaysOk(21) == 42, "AlwaysOk doubles its input");
        DemoCase("case:results.basic.always_err.should_return_message_error");
        try
        {
            AlwaysErr("boom");
            Require(false, "AlwaysErr should throw");
        }
        catch (BoltException e)
        {
            Require(e.Message.Contains("boom"), "AlwaysErr error message");
        }

        // Result<Point, String> with Ok carrying a record.
        DemoCase("case:results.basic.parse_point.should_parse_coordinates");
        Point p = ParsePoint("3.0,4.0");
        Require(p.X == 3.0 && p.Y == 4.0, "ParsePoint round-trips x,y");
        DemoCase("case:results.basic.parse_point.should_reject_malformed_input");
        try
        {
            ParsePoint("bad");
            Require(false, "ParsePoint(bad) should throw");
        }
        catch (BoltException) { }

        // Result<String, String> with Ok carrying a wire-decoded String.
        DemoCase("case:results.nested_results.string.should_return_value_for_non_negative_key");
        Require(ResultOfString(1) == "item_1", "ResultOfString ok");
        DemoCase("case:results.nested_results.string.should_reject_negative_key");
        try
        {
            ResultOfString(-1);
            Require(false, "ResultOfString(-1) should throw");
        }
        catch (BoltException) { }

        // Result<Option<i32>, String>: Some, None, then Err.
        DemoCase("case:results.nested_results.option.should_return_some_for_positive_key");
        Require(ResultOfOption(5) == 10, "ResultOfOption(5) returns Some(10)");
        DemoCase("case:results.nested_results.option.should_return_none_for_zero_key");
        Require(ResultOfOption(0) == null, "ResultOfOption(0) returns None");
        DemoCase("case:results.nested_results.option.should_reject_negative_key");
        try
        {
            ResultOfOption(-1);
            Require(false, "ResultOfOption(-1) should throw");
        }
        catch (BoltException) { }

        // Result<Vec<i32>, String> Ok and Err.
        DemoCase("case:results.nested_results.vec.should_return_values_for_non_negative_count");
        int[] vec = ResultOfVec(3);
        Require(vec.Length == 3 && vec[0] == 0 && vec[1] == 1 && vec[2] == 2, "ResultOfVec ok");
        DemoCase("case:results.nested_results.vec.should_reject_negative_count");
        try
        {
            ResultOfVec(-1);
            Require(false, "ResultOfVec(-1) should throw");
        }
        catch (BoltException) { }

        Console.WriteLine("  PASS\n");
    }

    private static void TestResultClassMethods()
    {
        Console.WriteLine("Testing result class methods...");

        using (var counter = new Counter(0))
        {
            counter.Increment();
            counter.Increment();
            counter.Increment();
            Require(counter.TryGetPositive() == 3, "Counter.TryGetPositive after 3 increments");
        }

        using (var counter = new Counter(0))
        {
            try
            {
                counter.TryGetPositive();
                Require(false, "Counter.TryGetPositive should throw at zero");
            }
            catch (BoltException) { }
        }

        Console.WriteLine("  PASS\n");
    }

    private static void TestResultEnumErrors()
    {
        Console.WriteLine("Testing result enum/record errors (typed exceptions)...");

        // C-style #[error] enum -> dedicated MathErrorException with
        // an Error property that exposes the underlying enum value.
        DemoCase("case:results.error_enums.checked_divide.should_return_quotient");
        Require(CheckedDivide(10, 2) == 5, "CheckedDivide(10, 2) ok");
        DemoCase("case:results.error_enums.checked_divide.should_reject_division_by_zero");
        try
        {
            CheckedDivide(10, 0);
            Require(false, "CheckedDivide(10, 0) should throw");
        }
        catch (MathErrorException e)
        {
            Require(e.Error == MathError.DivisionByZero, "CheckedDivide typed error");
        }

        DemoCase("case:results.error_enums.checked_sqrt.should_return_square_root");
        Require(CheckedSqrt(9.0) == 3.0, "CheckedSqrt(9) ok");
        DemoCase("case:results.error_enums.checked_sqrt.should_reject_negative_input");
        try
        {
            CheckedSqrt(-1.0);
            Require(false, "CheckedSqrt(-1) should throw");
        }
        catch (MathErrorException e)
        {
            Require(e.Error == MathError.NegativeInput, "CheckedSqrt typed error");
        }

        DemoCase("case:results.error_enums.checked_add.should_return_sum");
        Require(CheckedAdd(1, 2) == 3, "CheckedAdd(1, 2) ok");
        DemoCase("case:results.error_enums.checked_add.should_reject_overflow");
        try
        {
            CheckedAdd(int.MaxValue, 1);
            Require(false, "CheckedAdd(MAX, 1) should throw");
        }
        catch (MathErrorException e)
        {
            Require(e.Error == MathError.Overflow, "CheckedAdd typed error");
        }

        // ValidationError uses an explicit #[repr(i32)] with non-zero
        // discriminants — make sure the wire decode keeps mapping each
        // tag to the right variant on the throw path.
        DemoCase("case:results.error_enums.validate_username.should_accept_valid_name");
        Require(ValidateUsername("alice") == "alice", "ValidateUsername ok");
        DemoCase("case:results.error_enums.validate_username.should_reject_too_short_name");
        try
        {
            ValidateUsername("ab");
            Require(false, "ValidateUsername short should throw");
        }
        catch (ValidationErrorException e)
        {
            Require(e.Error == ValidationError.TooShort, "ValidateUsername TooShort");
        }
        DemoCase("case:results.error_enums.validate_username.should_reject_too_long_name");
        try
        {
            ValidateUsername("a]bcdefghijklmnopqrstu");
            Require(false, "ValidateUsername long should throw");
        }
        catch (ValidationErrorException e)
        {
            Require(e.Error == ValidationError.TooLong, "ValidateUsername TooLong");
        }
        DemoCase("case:results.error_enums.validate_username.should_reject_invalid_format");
        try
        {
            ValidateUsername("has space");
            Require(false, "ValidateUsername spaces should throw");
        }
        catch (ValidationErrorException e)
        {
            Require(e.Error == ValidationError.InvalidFormat, "ValidateUsername InvalidFormat");
        }

        // Structured (record) #[error] -> AppErrorException wraps the
        // record so the caller can both `catch` it as an exception and
        // access the original fields via the Error property.
        DemoCase("case:results.error_enums.may_fail.should_return_success_when_valid");
        Require(MayFail(true) == "Success!", "MayFail(true) ok");
        DemoCase("case:results.error_enums.may_fail.should_return_app_error_when_invalid");
        try
        {
            MayFail(false);
            Require(false, "MayFail(false) should throw");
        }
        catch (AppErrorException e)
        {
            Require(e.Error.Code == 400, "MayFail AppError.Code");
            Require(e.Error.Message == "Invalid input", "MayFail AppError.Message");
            Require(e.Message == "Invalid input", "MayFail Exception.Message mirrors AppError.Message");
        }

        DemoCase("case:results.error_enums.divide_app.should_return_quotient");
        Require(DivideApp(10, 2) == 5, "DivideApp ok");
        DemoCase("case:results.error_enums.divide_app.should_return_app_error_for_division_by_zero");
        try
        {
            DivideApp(10, 0);
            Require(false, "DivideApp(10, 0) should throw");
        }
        catch (AppErrorException e)
        {
            Require(e.Error.Code == 500, "DivideApp AppError.Code");
            Require(e.Error.Message == "Division by zero", "DivideApp AppError.Message");
        }

        DemoCase("case:results.error_enums.process_value.should_return_success_variant");
        Require(ProcessValue(5) is ApiResult.Success, "ProcessValue(5) -> Success");
        DemoCase("case:results.error_enums.process_value.should_return_error_code_variant");
        Require(ProcessValue(0) is ApiResult.ErrorCode ec && ec.Value0 == -1, "ProcessValue(0) -> ErrorCode(-1)");
        DemoCase("case:results.error_enums.process_value.should_return_error_with_data_variant");
        Require(ProcessValue(-3) is ApiResult.ErrorWithData ed && ed.Code == -3 && ed.Detail == -6,
            "ProcessValue(-3) -> ErrorWithData(-3,-6)");

        DemoCase("case:results.error_enums.api_result_is_success.should_report_success_variant");
        Require(ApiResultIsSuccess(new ApiResult.Success()), "ApiResultIsSuccess(Success)");
        DemoCase("case:results.error_enums.api_result_is_success.should_report_error_variant");
        Require(!ApiResultIsSuccess(new ApiResult.ErrorCode(1)), "ApiResultIsSuccess(ErrorCode) false");

        DemoCase("case:results.error_enums.try_compute.should_return_doubled_value");
        Require(TryCompute(7) == 14, "TryCompute(7) == 14");
        DemoCase("case:results.error_enums.try_compute.should_return_overflow_error");
        try
        {
            TryCompute(-1);
            Require(false, "TryCompute(-1) should throw");
        }
        catch (ComputeErrorException e)
        {
            Require(e.Error is ComputeError.Overflow, "TryCompute(-1) Overflow variant");
        }

        Console.WriteLine("  PASS\n");
    }

    private static async System.Threading.Tasks.Task TestAsyncFunctions()
    {
        Console.WriteLine("Testing async functions...");

        DemoCase("case:async_fns.basic.add.should_return_sum");
        Require(await AsyncAdd(3, 7) == 10, "AsyncAdd(3, 7)");
        DemoCase("case:async_fns.basic.echo.should_prefix_message");
        Require(await AsyncEcho("hello async") == "Echo: hello async", "AsyncEcho string return");
        DemoCase("case:async_fns.basic.double_all.should_double_i32_vector");
        Require((await AsyncDoubleAll(new[] { 1, 2, 3 })).SequenceEqual(new[] { 2, 4, 6 }),
            "AsyncDoubleAll primitive vec return");
        DemoCase("case:async_fns.basic.find_positive.should_return_first_positive");
        Require(await AsyncFindPositive(new[] { -1, 0, 5, 3 }) == 5, "AsyncFindPositive finds first positive");
        DemoCase("case:async_fns.basic.find_positive.should_return_none_for_all_negative");
        Require(await AsyncFindPositive(new[] { -3, -2, -1 }) == null, "AsyncFindPositive all-negative returns null");
        DemoCase("case:async_fns.basic.concat.should_join_string_vector");
        Require(await AsyncConcat(new[] { "a", "b", "c" }) == "a, b, c", "AsyncConcat Vec<String> param");
        DemoCase("case:async_fns.basic.get_numbers.should_return_counting_sequence");
        Require((await AsyncGetNumbers(4)).SequenceEqual(new[] { 0, 1, 2, 3 }), "AsyncGetNumbers(4)");

        MixedRecordParameters parameters = new MixedRecordParameters(
            new[] { "async", "record" },
            new[] { new Point(1.0, 1.0), new Point(2.0, 2.0) },
            new Point(9.0, 10.0),
            8u,
            true
        );
        MixedRecord record = new MixedRecord(
            "async-record",
            new Point(3.0, 4.0),
            Priority.Critical,
            new Shape.Circle(2.0),
            parameters
        );

        DemoCase("case:async_fns.mixed_record.echo.should_roundtrip_record");
        MixedRecord echoed = await AsyncEchoMixedRecord(record);
        Require(echoed.Name == record.Name, "AsyncEchoMixedRecord.Name");
        Require(echoed.Anchor == record.Anchor, "AsyncEchoMixedRecord.Anchor");
        Require(echoed.Priority == record.Priority, "AsyncEchoMixedRecord.Priority");
        Require(echoed.Shape is Shape.Circle echoedCircle && echoedCircle.Radius == 2.0,
            "AsyncEchoMixedRecord.Shape");
        Require(echoed.Parameters.Tags.SequenceEqual(record.Parameters.Tags),
            "AsyncEchoMixedRecord.Parameters.Tags");

        DemoCase("case:async_fns.mixed_record.make.should_construct_record");
        MixedRecord made = await AsyncMakeMixedRecord(
            "made-async",
            new Point(5.0, 6.0),
            Priority.High,
            new Shape.Rectangle(7.0, 8.0),
            parameters
        );
        Require(made.Name == "made-async", "AsyncMakeMixedRecord.Name");
        Require(made.Anchor == new Point(5.0, 6.0), "AsyncMakeMixedRecord.Anchor");
        Require(made.Priority == Priority.High, "AsyncMakeMixedRecord.Priority");
        Require(made.Shape is Shape.Rectangle rect && rect.Width == 7.0 && rect.Height == 8.0,
            "AsyncMakeMixedRecord.Shape");
        Require(made.Parameters.Tags.SequenceEqual(parameters.Tags), "AsyncMakeMixedRecord.Parameters");

        Console.WriteLine("  PASS\n");
    }

    private static async System.Threading.Tasks.Task TestAsyncResults()
    {
        Console.WriteLine("Testing async result functions...");

        DemoCase("case:async_fns.results.try_compute.should_return_doubled_value");
        Require(await TryComputeAsync(6) == 12, "TryComputeAsync success");
        DemoCase("case:async_fns.results.try_compute.should_return_invalid_input_for_zero");
        try
        {
            await TryComputeAsync(0);
            Require(false, "TryComputeAsync(0) should throw");
        }
        catch (ComputeErrorException e)
        {
            Require(e.Error is ComputeError.InvalidInput invalid && invalid.Value0 == -999,
                "TryComputeAsync typed ComputeError");
        }
        DemoCase("case:async_fns.results.try_compute.should_return_overflow_for_negative_value");
        try
        {
            await TryComputeAsync(-2);
            Require(false, "TryComputeAsync(-2) should throw");
        }
        catch (ComputeErrorException e)
        {
            Require(e.Error is ComputeError.Overflow, "TryComputeAsync(-2) Overflow variant");
        }

        DemoCase("case:async_fns.results.fetch_data.should_return_scaled_positive_id");
        Require(await FetchData(2) == 20, "FetchData(2) success");
        DemoCase("case:async_fns.results.fetch_data.should_reject_non_positive_id");
        try
        {
            await FetchData(-1);
            Require(false, "FetchData(-1) should throw");
        }
        catch (BoltException e)
        {
            Require(e.Message.Contains("invalid id"), "FetchData(-1) BoltException");
        }

        DemoCase("case:results.async_results.safe_divide.should_return_quotient");
        Require(await AsyncSafeDivide(10, 2) == 5, "AsyncSafeDivide(10, 2)");
        DemoCase("case:results.async_results.safe_divide.should_reject_division_by_zero");
        try
        {
            await AsyncSafeDivide(10, 0);
            Require(false, "AsyncSafeDivide(10, 0) should throw");
        }
        catch (MathErrorException e)
        {
            Require(e.Error == MathError.DivisionByZero, "AsyncSafeDivide typed MathError");
        }

        DemoCase("case:results.async_results.fallible_fetch.should_return_value_for_non_negative_key");
        Require(await AsyncFallibleFetch(3) == "value_3", "AsyncFallibleFetch(3)");
        DemoCase("case:results.async_results.fallible_fetch.should_reject_negative_key");
        try
        {
            await AsyncFallibleFetch(-1);
            Require(false, "AsyncFallibleFetch(-1) should throw");
        }
        catch (BoltException e)
        {
            Require(e.Message.Contains("invalid key"), "AsyncFallibleFetch negative-key BoltException");
        }

        DemoCase("case:results.async_results.find_value.should_return_some_for_positive_key");
        Require(await AsyncFindValue(2) == 20, "AsyncFindValue(2)");
        DemoCase("case:results.async_results.find_value.should_return_none_for_zero_key");
        Require(await AsyncFindValue(0) == null, "AsyncFindValue(0) returns null");
        DemoCase("case:results.async_results.find_value.should_reject_negative_key");
        try
        {
            await AsyncFindValue(-1);
            Require(false, "AsyncFindValue(-1) should throw");
        }
        catch (BoltException e)
        {
            Require(e.Message.Contains("invalid key"), "AsyncFindValue negative-key BoltException");
        }

        Console.WriteLine("  PASS\n");
    }

    private static async System.Threading.Tasks.Task TestAsyncClassMethods()
    {
        Console.WriteLine("Testing async class methods...");

        using (var worker = new AsyncWorker("worker"))
        {
            Require(await worker.Process("item") == "worker: item", "AsyncWorker.Process");
            Require(await worker.TryProcess("ok") == "worker: ok", "AsyncWorker.TryProcess success");
            try
            {
                await worker.TryProcess("");
                Require(false, "AsyncWorker.TryProcess(empty) should throw");
            }
            catch (BoltException e)
            {
                Require(e.Message.Contains("input must not be empty"), "AsyncWorker.TryProcess error");
            }
            Require(await worker.FindItem(3) == "worker_3", "AsyncWorker.FindItem Some");
            Require(await worker.FindItem(0) == null, "AsyncWorker.FindItem None");
            Require((await worker.ProcessBatch(new[] { "a", "b" })).SequenceEqual(new[] { "worker: a", "worker: b" }),
                "AsyncWorker.ProcessBatch");
        }

        using (var shared = new SharedCounter(5))
        {
            Require(await shared.AsyncGet() == 5, "SharedCounter.AsyncGet");
            Require(await shared.AsyncAdd(7) == 12, "SharedCounter.AsyncAdd");
            Require(await shared.AsyncGet() == 12, "SharedCounter.AsyncGet after add");
        }

        using (var holder = new StateHolder("async-holder"))
        {
            Require(await holder.AsyncGetValue() == 0, "StateHolder.AsyncGetValue default");
            await holder.AsyncSetValue(41);
            Require(await holder.AsyncGetValue() == 41, "StateHolder.AsyncSetValue");
            Require(await holder.AsyncAddItem("alpha") == 1u, "StateHolder.AsyncAddItem first");
            Require(await holder.AsyncAddItem("beta") == 2u, "StateHolder.AsyncAddItem second");
        }

        using (var ds = new DataStore())
        {
            ds.Add(new DataPoint(1.0, 2.0, 100L));
            ds.Add(new DataPoint(3.0, 4.0, 200L));
            Require(Math.Abs(await ds.AsyncSum() - 10.0) < 1e-9, "DataStore.AsyncSum");
            Require(await ds.AsyncLen() == (nuint)2, "DataStore.AsyncLen");
        }

        using (var svc = new MixedRecordService("async-svc"))
        {
            MixedRecordParameters parameters = new MixedRecordParameters(
                new[] { "svc", "async" },
                new[] { new Point(0.0, 0.0) },
                new Point(1.0, 2.0),
                4u,
                false
            );
            MixedRecord record = new MixedRecord(
                "svc-record",
                new Point(2.0, 3.0),
                Priority.Low,
                new Shape.Point(),
                parameters
            );
            MixedRecord echoed = await svc.AsyncEchoRecord(record);
            Require(echoed.Name == record.Name, "MixedRecordService.AsyncEchoRecord.Name");
            Require(echoed.Anchor == record.Anchor, "MixedRecordService.AsyncEchoRecord.Anchor");
            Require(echoed.Priority == record.Priority, "MixedRecordService.AsyncEchoRecord.Priority");
            Require(echoed.Shape is Shape.Point, "MixedRecordService.AsyncEchoRecord.Shape");
            Require(echoed.Parameters.Tags.SequenceEqual(record.Parameters.Tags),
                "MixedRecordService.AsyncEchoRecord.Parameters.Tags");
            MixedRecord stored = await svc.AsyncStoreRecordParts(
                "stored-async",
                new Point(8.0, 9.0),
                Priority.Critical,
                new Shape.Circle(3.0),
                parameters
            );
            Require(stored.Name == "stored-async", "MixedRecordService.AsyncStoreRecordParts.Name");
            Require(stored.Anchor == new Point(8.0, 9.0), "MixedRecordService.AsyncStoreRecordParts.Anchor");
            Require(stored.Priority == Priority.Critical, "MixedRecordService.AsyncStoreRecordParts.Priority");
            Require(stored.Shape is Shape.Circle circle && circle.Radius == 3.0,
                "MixedRecordService.AsyncStoreRecordParts.Shape");
            Require(svc.StoredCount() == 1u, "MixedRecordService.AsyncStoreRecordParts stored count");
        }

        Console.WriteLine("  PASS\n");
    }

    private static async System.Threading.Tasks.Task TestAsyncCancellation()
    {
        Console.WriteLine("Testing async cancellation...");

        using var cts = new CancellationTokenSource();
        cts.Cancel();
        try
        {
            await AsyncAdd(1, 2, cts.Token);
            Require(false, "AsyncAdd with pre-canceled token should throw");
        }
        catch (OperationCanceledException) { }

        Console.WriteLine("  PASS\n");
    }

    private static void TestCallbackTraits()
    {
        Console.WriteLine("Testing callback traits...");

        ValueCallback doubler = new ValueCallbackImpl(v => v * 2);
        Require(InvokeValueCallback(doubler, 4) == 8, "InvokeValueCallback local");
        Require(InvokeValueCallbackTwice(doubler, 3, 4) == 14, "InvokeValueCallbackTwice local");
        Require(InvokeBoxedValueCallback(doubler, 5) == 10, "InvokeBoxedValueCallback local");
        Require(InvokeTwoCallbacks(doubler, new ValueCallbackImpl(v => v * 3), 5) == 25,
            "InvokeTwoCallbacks local");
        Require(InvokeOptionalValueCallback(null, 4) == 4, "InvokeOptionalValueCallback null");

        // Returned callbacks are owning proxies; `using` releases the native
        // callback handle deterministically instead of waiting for finalization.
        using ValueCallbackProxy incrementer = MakeIncrementingCallback(5);
        Require(InvokeValueCallback(incrementer, 4) == 9, "returned ValueCallback proxy");

        MessageFormatter formatter = new MessageFormatterImpl();
        Require(FormatMessageWithCallback(formatter, "sync", "formatter") == "sync::formatter",
            "FormatMessageWithCallback local");
        Require(FormatMessageWithOptionalCallback(null, "fallback", "message") == "fallback::message",
            "FormatMessageWithOptionalCallback null");
        // Same ownership contract for returned multi-method callback proxies.
        using MessageFormatterProxy prefixer = MakeMessagePrefixer("prefix");
        Require(FormatMessageWithCallback(prefixer, "sync", "formatter") == "prefix::sync::formatter",
            "returned MessageFormatter proxy");

        Require(ProcessVec(new VecProcessorImpl(), new[] { 1, 2, 3 }).SequenceEqual(new[] { 2, 4, 6 }),
            "ProcessVec callback");
        Require(InvokeOptionCallback(new OptionCallbackImpl(), 7) == 70, "InvokeOptionCallback Some");
        Require(InvokeOptionCallback(new OptionCallbackImpl(), 0) == null, "InvokeOptionCallback None");
        Require(InvokeResultCallback(new ResultCallbackImpl(), 7) == 70, "InvokeResultCallback Ok");
        try
        {
            InvokeResultCallback(new ResultCallbackImpl(), -1);
            Require(false, "InvokeResultCallback Err should throw");
        }
        catch (MathErrorException e)
        {
            Require(e.Error == MathError.NegativeInput, "InvokeResultCallback Err type");
        }

        Require(InvokeOffsetCallback(new OffsetCallbackImpl(), (nint)(-5), (nuint)8) == (nint)3,
            "InvokeOffsetCallback pointer-sized params");

        using (var consumer = new DataConsumer())
        {
            consumer.SetProvider(new DataProviderImpl());
            Require(consumer.ComputeSum() == 10UL, "stored DataProvider callback");
        }

        Console.WriteLine("  PASS\n");
    }

    private static void TestClosures()
    {
        Console.WriteLine("Testing closures...");

        Require(ApplyClosure(v => v * 3, 4) == 12, "ApplyClosure");
        Require(ApplyBinaryClosure((a, b) => a + b, 3, 4) == 7, "ApplyBinaryClosure");
        int observed = 0;
        ApplyVoidClosure(v => observed = v, 42);
        Require(observed == 42, "ApplyVoidClosure");
        Require(ApplyNullaryClosure(() => 7) == 7, "ApplyNullaryClosure");
        Require(ApplyPointClosure(p => new Point(p.X + 1.0, p.Y + 2.0), new Point(3.0, 4.0))
                == new Point(4.0, 6.0),
            "ApplyPointClosure");
        Require(ApplyStringClosure(s => s + "!", "hello") == "hello!", "ApplyStringClosure");
        Require(!ApplyBoolClosure(v => !v, true), "ApplyBoolClosure");
        Require(Math.Abs(ApplyF64Closure(v => v + 0.5, 1.25) - 1.75) < 1e-9, "ApplyF64Closure");
        Require(MapVecWithClosure(v => v * 2, new[] { 1, 2, 3 }).SequenceEqual(new[] { 2, 4, 6 }),
            "MapVecWithClosure");
        Require(FilterVecWithClosure(v => v > 1, new[] { 0, 1, 2, 3 }).SequenceEqual(new[] { 2, 3 }),
            "FilterVecWithClosure");
        Require(ApplyOffsetClosure((value, delta) => value + (nint)delta, (nint)10, (nuint)4) == (nint)14,
            "ApplyOffsetClosure");
        Require(ApplyStatusClosure(status => status == Status.Active ? Status.Inactive : Status.Active,
                Status.Active) == Status.Inactive,
            "ApplyStatusClosure");
        Require(ApplyOptionalPointClosure(point => point is null ? null : new Point(point.Value.X + 1.0, point.Value.Y),
                new Point(1.0, 2.0)) == new Point(2.0, 2.0),
            "ApplyOptionalPointClosure Some");
        Require(ApplyOptionalPointClosure(point => point, null) == null, "ApplyOptionalPointClosure None");
        Require(ApplyResultClosure(v => v * 2, 6) == 12, "ApplyResultClosure Ok");
        try
        {
            ApplyResultClosure(_ => throw new MathErrorException(MathError.NegativeInput), 6);
            Require(false, "ApplyResultClosure Err should throw");
        }
        catch (MathErrorException e)
        {
            Require(e.Error == MathError.NegativeInput, "ApplyResultClosure Err type");
        }

        Console.WriteLine("  PASS\n");
    }

    private static async System.Threading.Tasks.Task TestAsyncCallbackTraits()
    {
        Console.WriteLine("Testing async callback traits...");

        AsyncFetcher fetcher = new AsyncFetcherImpl();
        Require(await FetchWithAsyncCallback(fetcher, 5) == 15, "FetchWithAsyncCallback");
        Require(await FetchStringWithAsyncCallback(fetcher, "hello") == "HELLO", "FetchStringWithAsyncCallback");
        Require(await FetchJoinedMessageWithAsyncCallback(fetcher, "async", "callback") == "async::callback",
            "FetchJoinedMessageWithAsyncCallback");

        Require(await TransformPointWithAsyncCallback(new AsyncPointTransformerImpl(), new Point(1.0, 2.0))
                == new Point(2.0, 4.0),
            "TransformPointWithAsyncCallback");

        AsyncResultFormatter resultFormatter = new AsyncResultFormatterImpl();
        Require(await RenderMessageWithAsyncResultCallback(resultFormatter, "async", "result") == "async::result",
            "RenderMessageWithAsyncResultCallback Ok");
        Require(await TransformPointWithAsyncResultCallback(resultFormatter, new Point(3.0, 4.0), Status.Active)
                == new Point(4.0, 5.0),
            "TransformPointWithAsyncResultCallback Ok");
        try
        {
            await RenderMessageWithAsyncResultCallback(resultFormatter, "", "result");
            Require(false, "RenderMessageWithAsyncResultCallback Err should throw");
        }
        catch (MathErrorException e)
        {
            Require(e.Error == MathError.NegativeInput, "RenderMessageWithAsyncResultCallback Err type");
        }

        Console.WriteLine("  PASS\n");
    }

    private static async System.Threading.Tasks.Task TestStreams()
    {
        Console.WriteLine("Testing streams (async mode)...");
        using (var bus = new EventBus())
        {
            global::System.Threading.Tasks.Task<List<int>> receivedTask =
                CollectStreamItems(bus.SubscribeValues(), 3, "async stream");

            bus.EmitValue(10);
            bus.EmitValue(20);
            bus.EmitValue(30);

            List<int> received = await receivedTask;
            Require(received.Count >= 3, $"async stream received {received.Count} items, expected >= 3");
            Require(received.Contains(10), "async stream should contain 10");
            Require(received.Contains(20), "async stream should contain 20");
            Require(received.Contains(30), "async stream should contain 30");
        }
        Console.WriteLine("  PASS\n");

        Console.WriteLine("Testing streams (batch mode)...");
        using (var bus = new EventBus())
        {
            global::System.Threading.Tasks.Task<List<int>> receivedTask =
                CollectStreamItems(bus.SubscribeValuesBatch(), 3, "batch stream");

            bus.EmitValue(100);
            bus.EmitValue(200);
            bus.EmitValue(300);

            List<int> received = await receivedTask;
            Require(received.Count >= 3, $"batch stream received {received.Count} items, expected >= 3");
            Require(received.Contains(100), "batch stream should contain 100");
            Require(received.Contains(200), "batch stream should contain 200");
            Require(received.Contains(300), "batch stream should contain 300");
        }
        Console.WriteLine("  PASS\n");

        Console.WriteLine("Testing streams (callback mode)...");
        using (var bus = new EventBus())
        {
            global::System.Threading.Tasks.Task<List<int>> receivedTask =
                CollectStreamItems(bus.SubscribeValuesCallback(), 3, "callback stream");

            bus.EmitValue(1000);
            bus.EmitValue(2000);
            bus.EmitValue(3000);

            List<int> received = await receivedTask;
            Require(received.Count >= 3, $"callback stream received {received.Count} items, expected >= 3");
            Require(received.Contains(1000), "callback stream should contain 1000");
            Require(received.Contains(2000), "callback stream should contain 2000");
            Require(received.Contains(3000), "callback stream should contain 3000");
        }
        Console.WriteLine("  PASS\n");

        Console.WriteLine("Testing streams (record items)...");
        using (var bus = new EventBus())
        {
            Point first = new Point(1.0, 2.0);
            Point second = new Point(3.0, 4.0);
            global::System.Threading.Tasks.Task<List<Point>> receivedTask =
                CollectStreamItems(bus.SubscribePoints(), 2, "point stream");

            bus.EmitPoint(first);
            bus.EmitPoint(second);

            List<Point> received = await receivedTask;
            Require(received.Count >= 2, $"point stream received {received.Count} items, expected >= 2");
            Require(received.Contains(first), "point stream should contain first point");
            Require(received.Contains(second), "point stream should contain second point");
        }
        Console.WriteLine("  PASS\n");

        Console.WriteLine("Testing streams (cancellation mid-stream)...");
        using (var bus = new EventBus())
        {
            using var cts = new CancellationTokenSource();
            var received = new List<int>();

            var pump = global::System.Threading.Tasks.Task.Run(async () =>
            {
                try
                {
                    await foreach (int v in bus.SubscribeValues().WithCancellation(cts.Token))
                    {
                        received.Add(v);
                        if (received.Count == 1) cts.Cancel();
                    }
                }
                catch (OperationCanceledException) { /* expected */ }
            });

            await global::System.Threading.Tasks.Task.Delay(50);
            bus.EmitValue(7);
            bus.EmitValue(8);
            bus.EmitValue(9);

            var completed = await global::System.Threading.Tasks.Task.WhenAny(
                pump, global::System.Threading.Tasks.Task.Delay(TimeSpan.FromSeconds(5)));
            Require(completed == pump, "cancelled stream pump should terminate within 5 seconds");
            Require(received.Count >= 1, "cancelled stream should have observed at least 1 item");
        }
        Console.WriteLine("  PASS\n");

        Console.WriteLine("Testing streams (early break)...");
        using (var bus = new EventBus())
        {
            var received = new List<int>();
            var pump = global::System.Threading.Tasks.Task.Run(async () =>
            {
                await foreach (int v in bus.SubscribeValues())
                {
                    received.Add(v);
                    if (received.Count == 1) break;
                }
            });

            await global::System.Threading.Tasks.Task.Delay(50);
            bus.EmitValue(11);
            bus.EmitValue(12);
            bus.EmitValue(13);

            var completed = await global::System.Threading.Tasks.Task.WhenAny(
                pump, global::System.Threading.Tasks.Task.Delay(TimeSpan.FromSeconds(5)));
            Require(completed == pump, "early-break stream pump should terminate within 5 seconds");
            Require(received.Count == 1, "early-break stream should have observed exactly 1 item");
        }
        Console.WriteLine("  PASS\n");
    }

    private static async System.Threading.Tasks.Task<List<T>> CollectStreamItems<T>(
        IAsyncEnumerable<T> stream,
        int expectedCount,
        string label)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        List<T> received = new List<T>();

        try
        {
            await foreach (T item in stream.WithCancellation(timeout.Token))
            {
                received.Add(item);
                if (received.Count >= expectedCount) break;
            }
        }
        catch (OperationCanceledException ex) when (timeout.IsCancellationRequested)
        {
            throw new TimeoutException($"{label} should deliver {expectedCount} items within 5 seconds", ex);
        }

        return received;
    }

    private sealed class ValueCallbackImpl : ValueCallback
    {
        private readonly Func<int, int> _onValue;

        internal ValueCallbackImpl(Func<int, int> onValue)
        {
            _onValue = onValue;
        }

        public int OnValue(int value) => _onValue(value);
    }

    private sealed class MessageFormatterImpl : MessageFormatter
    {
        public string FormatMessage(string scope, string message) => $"{scope}::{message}";
    }

    private sealed class VecProcessorImpl : VecProcessor
    {
        public int[] Process(int[] values) => values.Select(v => v * 2).ToArray();
    }

    private sealed class OptionCallbackImpl : OptionCallback
    {
        public int? FindValue(int key) => key == 0 ? null : key * 10;
    }

    private sealed class ResultCallbackImpl : ResultCallback
    {
        public int Compute(int value)
        {
            if (value < 0) throw new MathErrorException(MathError.NegativeInput);
            return value * 10;
        }
    }

    private sealed class OffsetCallbackImpl : OffsetCallback
    {
        public nint Offset(nint value, nuint delta) => value + (nint)delta;
    }

    private sealed class DataProviderImpl : DataProvider
    {
        public uint GetCount() => 2u;

        public DataPoint GetItem(uint index)
        {
            return index switch
            {
                0u => new DataPoint(1.0, 2.0, 100L),
                1u => new DataPoint(3.0, 4.0, 200L),
                _ => new DataPoint(0.0, 0.0, 0L),
            };
        }
    }

    private sealed class AsyncFetcherImpl : AsyncFetcher
    {
        public async global::System.Threading.Tasks.Task<int> FetchValue(int key)
        {
            await global::System.Threading.Tasks.Task.Yield();
            return key + 10;
        }

        public async global::System.Threading.Tasks.Task<string> FetchString(string input)
        {
            await global::System.Threading.Tasks.Task.Yield();
            return input.ToUpperInvariant();
        }

        public async global::System.Threading.Tasks.Task<string> FetchJoinedMessage(string scope, string message)
        {
            await global::System.Threading.Tasks.Task.Yield();
            return $"{scope}::{message}";
        }
    }

    private sealed class AsyncPointTransformerImpl : AsyncPointTransformer
    {
        public async global::System.Threading.Tasks.Task<Point> TransformPoint(Point point)
        {
            await global::System.Threading.Tasks.Task.Yield();
            return new Point(point.X + 1.0, point.Y + 2.0);
        }
    }

    private sealed class AsyncResultFormatterImpl : AsyncResultFormatter
    {
        public async global::System.Threading.Tasks.Task<string> RenderMessage(string scope, string message)
        {
            await global::System.Threading.Tasks.Task.Yield();
            if (scope.Length == 0) throw new MathErrorException(MathError.NegativeInput);
            return $"{scope}::{message}";
        }

        public async global::System.Threading.Tasks.Task<Point> TransformPoint(Point point, Status status)
        {
            await global::System.Threading.Tasks.Task.Yield();
            if (status == Status.Inactive) throw new MathErrorException(MathError.NegativeInput);
            return new Point(point.X + 1.0, point.Y + 1.0);
        }
    }

    private static void Require(bool condition, string label)
    {
        if (!condition) throw new InvalidOperationException($"FAIL: {label}");
    }

    private static void DemoCase(string caseId)
    {
        currentDemoCase = caseId;
    }

    private static string DescribeFailure(Exception ex)
    {
        if (currentDemoCase is null || ex.ToString().Contains("case:"))
        {
            return ex.ToString();
        }

        return $"{currentDemoCase}: {ex}";
    }
}
