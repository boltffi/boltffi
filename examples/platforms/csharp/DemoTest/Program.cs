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
            TestBlittableRecords();
            TestRecordsWithStrings();
            TestRecordsWithDefaults();
            TestNestedRecords();
            TestCStyleEnums();
            TestDataEnums();
            TestRecordsWithEnumFields();
            TestPrimitiveVecs();
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
            Console.Error.WriteLine($"FAIL: {ex}");
            return 1;
        }
    }

    private static void TestBool()
    {
        Console.WriteLine("Testing bool...");
        Require(EchoBool(true), "echoBool(true)");
        Require(!EchoBool(false), "echoBool(false)");
        Require(!NegateBool(true), "negateBool(true)");
        Require(NegateBool(false), "negateBool(false)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestI8()
    {
        Console.WriteLine("Testing i8...");
        Require(EchoI8(42) == 42, "echoI8(42)");
        Require(EchoI8(-128) == -128, "echoI8(min)");
        Require(EchoI8(127) == 127, "echoI8(max)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestU8()
    {
        Console.WriteLine("Testing u8...");
        Require(EchoU8(0) == 0, "echoU8(0)");
        Require(EchoU8(255) == 255, "echoU8(max)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestI16()
    {
        Console.WriteLine("Testing i16...");
        Require(EchoI16(-32768) == -32768, "echoI16(min)");
        Require(EchoI16(32767) == 32767, "echoI16(max)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestU16()
    {
        Console.WriteLine("Testing u16...");
        Require(EchoU16(0) == 0, "echoU16(0)");
        Require(EchoU16(65535) == 65535, "echoU16(max)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestI32()
    {
        Console.WriteLine("Testing i32...");
        Require(EchoI32(42) == 42, "echoI32(42)");
        Require(EchoI32(-100) == -100, "echoI32(-100)");
        Require(AddI32(10, 20) == 30, "addI32(10, 20)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestU32()
    {
        Console.WriteLine("Testing u32...");
        Require(EchoU32(0u) == 0u, "echoU32(0)");
        Require(EchoU32(uint.MaxValue) == uint.MaxValue, "echoU32(max)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestI64()
    {
        Console.WriteLine("Testing i64...");
        Require(EchoI64(9999999999L) == 9999999999L, "echoI64(large)");
        Require(EchoI64(-9999999999L) == -9999999999L, "echoI64(negative large)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestU64()
    {
        Console.WriteLine("Testing u64...");
        Require(EchoU64(0UL) == 0UL, "echoU64(0)");
        Require(EchoU64(ulong.MaxValue) == ulong.MaxValue, "echoU64(max)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestF32()
    {
        Console.WriteLine("Testing f32...");
        Require(Math.Abs(EchoF32(3.14f) - 3.14f) < 0.001f, "echoF32(3.14)");
        Require(Math.Abs(AddF32(1.5f, 2.5f) - 4.0f) < 0.001f, "addF32(1.5, 2.5)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestF64()
    {
        Console.WriteLine("Testing f64...");
        Require(Math.Abs(EchoF64(3.14159265359) - 3.14159265359) < 0.0000001, "echoF64(pi)");
        Require(Math.Abs(AddF64(1.5, 2.5) - 4.0) < 0.0000001, "addF64(1.5, 2.5)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestUsize()
    {
        Console.WriteLine("Testing usize...");
        Require(EchoUsize((nuint)42) == (nuint)42, "echoUsize(42)");
        Require(EchoUsize((nuint)0) == (nuint)0, "echoUsize(0)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestIsize()
    {
        Console.WriteLine("Testing isize...");
        Require(EchoIsize((nint)42) == (nint)42, "echoIsize(42)");
        Require(EchoIsize((nint)(-100)) == (nint)(-100), "echoIsize(-100)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestStrings()
    {
        Console.WriteLine("Testing strings...");
        Require(EchoString("hello") == "hello", "echoString(hello)");
        Require(EchoString("") == "", "echoString(empty)");
        Require(EchoString("café") == "café", "echoString(unicode)");
        Require(EchoString("日本語") == "日本語", "echoString(cjk)");
        Require(EchoString("hello 🌍 world") == "hello 🌍 world", "echoString(emoji)");

        Require(ConcatStrings("foo", "bar") == "foobar", "concatStrings(foo, bar)");
        Require(ConcatStrings("", "bar") == "bar", "concatStrings(empty, bar)");
        Require(ConcatStrings("foo", "") == "foo", "concatStrings(foo, empty)");
        Require(ConcatStrings("🎉", "🎊") == "🎉🎊", "concatStrings(emoji)");

        Require(StringLength("hello") == 5u, "stringLength(hello)");
        Require(StringLength("") == 0u, "stringLength(empty)");
        Require(StringLength("café") == 5u, "stringLength(utf8 bytes)");
        Require(StringLength("🌍") == 4u, "stringLength(emoji 4 bytes)");

        Require(StringIsEmpty(""), "stringIsEmpty(empty)");
        Require(!StringIsEmpty("x"), "stringIsEmpty(nonempty)");

        Require(RepeatString("ab", 3u) == "ababab", "repeatString(ab, 3)");
        Require(RepeatString("x", 0u) == "", "repeatString(x, 0)");
        Require(RepeatString("🌟", 2u) == "🌟🌟", "repeatString(emoji, 2)");
        Console.WriteLine("  PASS\n");
    }

    private static void TestCustomTypes()
    {
        Console.WriteLine("Testing custom types (Email, UtcDateTime, Event)...");

        string email = "café@example.com";
        Require(EchoEmail(email) == email, "EchoEmail roundtrip");
        Require(EmailDomain(email) == "example.com", "EmailDomain");

        long ts = 1_710_000_000_000L;
        Require(EchoDatetime(ts) == ts, "EchoDatetime");
        Require(DatetimeToMillis(ts) == ts, "DatetimeToMillis");
        Require(FormatTimestamp(ts).StartsWith("2024-03-"), "FormatTimestamp");

        Event evt = new Event("launch", ts);
        Event echoed = EchoEvent(evt);
        Require(echoed.Name == "launch", "EchoEvent.Name");
        Require(echoed.Timestamp == ts, "EchoEvent.Timestamp");
        Require(EventTimestamp(evt) == ts, "EventTimestamp");

        string[] emails = new[] { "café@example.com", "user@example.org" };
        string[] echoedEmails = EchoEmails(emails);
        Require(echoedEmails.Length == 2, "EchoEmails length");
        Require(echoedEmails[0] == "café@example.com", "EchoEmails[0] roundtrip (utf-8)");
        Require(echoedEmails[1] == "user@example.org", "EchoEmails[1] roundtrip");

        long[] dts = new[] { 1_710_000_000_000L, 1_710_000_001_000L, 1_710_000_002_000L };
        long[] echoedDts = EchoDatetimes(dts);
        Require(echoedDts.Length == 3, "EchoDatetimes length");
        Require(echoedDts[0] == dts[0] && echoedDts[1] == dts[1] && echoedDts[2] == dts[2],
            "EchoDatetimes roundtrip (blittable)");

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

        Point p = MakePoint(1.5, 2.5);
        Require(p.X == 1.5, "MakePoint.X");
        Require(p.Y == 2.5, "MakePoint.Y");

        Point echoed = EchoPoint(new Point(3.0, 4.0));
        Require(echoed == new Point(3.0, 4.0), "EchoPoint value equality");

        Point sum = AddPoints(new Point(1.0, 2.0), new Point(3.0, 4.0));
        Require(sum == new Point(4.0, 6.0), "AddPoints");

        Color c = MakeColor(10, 20, 30, 255);
        Require(c.R == 10 && c.G == 20 && c.B == 30 && c.A == 255, "MakeColor fields");

        Color echoedColor = EchoColor(new Color(255, 0, 0, 128));
        Require(echoedColor == new Color(255, 0, 0, 128), "EchoColor value equality");

        // Static factories on a blittable record — return by value across
        // the ABI as a [StructLayout(Sequential)] struct.
        Require(Point.Origin() == new Point(0.0, 0.0), "Point.Origin()");
        Point fromPolar = Point.FromPolar(2.0, Math.PI / 2.0);
        Require(Math.Abs(fromPolar.X) < 1e-9 && Math.Abs(fromPolar.Y - 2.0) < 1e-9, "Point.FromPolar");
        Require(Point.Dimensions() == 2u, "Point.Dimensions() == 2");

        // Instance methods on a blittable record — `this` passes by value
        // through P/Invoke (no wire encode), exercising the
        // owner_is_blittable branch of CSharpReceiver::InstanceNative.
        Require(Math.Abs(new Point(3.0, 4.0).Distance() - 5.0) < 1e-9, "Point(3,4).Distance() == 5");
        Require(new Point(0.0, 0.0).Distance() == 0.0, "Point.Origin.Distance() == 0");
        Require(
            new Point(1.0, 2.0).Add(new Point(10.0, 20.0)) == new Point(11.0, 22.0),
            "Point.Add returns Point"
        );
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

        Person alice = MakePerson("Alice", 30);
        Require(alice.Name == "Alice", "MakePerson.Name");
        Require(alice.Age == 30u, "MakePerson.Age");

        Person echoed = EchoPerson(new Person("Bob", 42));
        Require(echoed == new Person("Bob", 42), "EchoPerson value equality");

        // Empty string boundary — the wire length prefix is 0.
        Person empty = EchoPerson(new Person("", 0));
        Require(empty.Name == "", "EchoPerson empty name");

        // Multi-byte UTF-8 boundary — one code point that encodes as 4 bytes.
        Person emoji = EchoPerson(new Person("\ud83c\udf89 Party", 25));
        Require(emoji.Name == "\ud83c\udf89 Party", "EchoPerson emoji round-trip");

        Require(
            GreetPerson(new Person("Alice", 30)) == "Hello, Alice! You are 30 years old.",
            "GreetPerson format"
        );

        // Address has three string fields back-to-back — exercises multiple
        // length-prefixed slices in one wire buffer.
        Address home = new Address("221B Baker Street", "London", "NW1 6XE");
        Address echoedAddress = EchoAddress(home);
        Require(echoedAddress == home, "EchoAddress round-trip");

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

        ServiceConfig config = new ServiceConfig("worker", 3, "standard", null, "https://default");
        ServiceConfig echoed = EchoServiceConfig(config);
        Require(echoed == config, "EchoServiceConfig round-trip");

        Require(
            config.Describe() == "worker:3:standard:none:https://default",
            "ServiceConfig.Describe() with defaults"
        );
        Require(
            config.DescribeWithPrefix("cfg") == "cfg:worker:3:standard:none:https://default",
            "ServiceConfig.DescribeWithPrefix() string param"
        );

        ServiceConfig withEndpoint = new ServiceConfig("api", 5, "us-east", "https://primary", "https://backup");
        Require(
            withEndpoint.Describe() == "api:5:us-east:https://primary:https://backup",
            "ServiceConfig.Describe() with endpoints"
        );

        Console.WriteLine("  PASS\n");
    }

    /// <summary>
    /// Nested records: Line holds two Points, Rect holds Point + Dimensions.
    /// Exercises the record-inside-record wire encode/decode path.
    /// </summary>
    private static void TestNestedRecords()
    {
        Console.WriteLine("Testing nested records (Line, Rect)...");

        Line line = MakeLine(0.0, 0.0, 3.0, 4.0);
        Require(line.Start == new Point(0.0, 0.0), "MakeLine.Start");
        Require(line.End == new Point(3.0, 4.0), "MakeLine.End");

        Line echoed = EchoLine(line);
        Require(echoed == line, "EchoLine round-trip");

        Require(Math.Abs(LineLength(line) - 5.0) < 1e-9, "LineLength 3-4-5");

        Rect rect = new Rect(
            new Point(1.0, 2.0),
            new Dimensions(10.0, 20.0)
        );
        Rect echoedRect = EchoRect(rect);
        Require(echoedRect == rect, "EchoRect round-trip");

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
        Require(EchoStatus(Status.Active) == Status.Active, "EchoStatus(Active)");
        Require(EchoStatus(Status.Pending) == Status.Pending, "EchoStatus(Pending)");
        Require(StatusToString(Status.Active) == "active", "StatusToString(Active)");
        Require(IsActive(Status.Active), "IsActive(Active)");
        Require(!IsActive(Status.Inactive), "IsActive(Inactive) false");

        Require(EchoDirection(Direction.North) == Direction.North, "EchoDirection(North)");
        Require(
            OppositeDirection(Direction.East) == Direction.West,
            "OppositeDirection(East) == West"
        );

        // Extension methods generated on the Methods companion class.
        Require(Direction.North.Opposite() == Direction.South, "North.Opposite()");
        Require(Direction.East.IsHorizontal(), "East.IsHorizontal()");
        Require(!Direction.North.IsHorizontal(), "!North.IsHorizontal()");
        Require(Direction.South.Label() == "S", "South.Label()");

        // Static factories on the companion class.
        Require(DirectionMethods.Cardinal() == Direction.North, "Cardinal() == North");
        Require(DirectionMethods.FromDegrees(90.0) == Direction.East, "FromDegrees(90) == East");
        Require(DirectionMethods.FromDegrees(180.0) == Direction.South, "FromDegrees(180) == South");
        Require(DirectionMethods.Count() == 4u, "Count() == 4");
        Require(DirectionMethods.New(2) == Direction.East, "New(2) == East");

        // Non-default backing type: LogLevel is #[repr(u8)] on the Rust side,
        // so these direct P/Invoke calls catch any accidental `enum : int`
        // projection in the generated C# surface.
        Require(EchoLogLevel(LogLevel.Trace) == LogLevel.Trace, "EchoLogLevel(Trace)");
        Require(EchoLogLevel(LogLevel.Error) == LogLevel.Error, "EchoLogLevel(Error)");
        Require(ShouldLog(LogLevel.Error, LogLevel.Warn), "ShouldLog(Error, Warn)");
        Require(!ShouldLog(LogLevel.Debug, LogLevel.Info), "!ShouldLog(Debug, Info)");

        // HttpCode has gapped #[repr(u16)] discriminants (200, 404, 500).
        // The raw value of each C# member must equal the Rust discriminant,
        // and a value constructed on the Rust side must map back to the
        // corresponding named member on the C# side.
        Require((ushort)HttpCode.Ok == 200, "HttpCode.Ok == 200");
        Require((ushort)HttpCode.NotFound == 404, "HttpCode.NotFound == 404");
        Require((ushort)HttpCode.ServerError == 500, "HttpCode.ServerError == 500");
        Require(HttpCodeNotFound() == HttpCode.NotFound, "Rust NotFound == C# NotFound");
        Require(EchoHttpCode(HttpCode.Ok) == HttpCode.Ok, "EchoHttpCode(Ok)");
        Require(EchoHttpCode(HttpCode.ServerError) == HttpCode.ServerError, "EchoHttpCode(ServerError)");

        // Sign has a #[repr(i8)] with a negative discriminant. The CLR
        // marshals sbyte across P/Invoke; the bit pattern must stay signed
        // in both directions.
        Require((sbyte)Sign.Negative == -1, "Sign.Negative == -1");
        Require((sbyte)Sign.Zero == 0, "Sign.Zero == 0");
        Require((sbyte)Sign.Positive == 1, "Sign.Positive == 1");
        Require(SignNegative() == Sign.Negative, "Rust Negative == C# Negative");
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
        Shape apexSome = new Shape.Apex(new Point(3.0, 4.0));
        Shape echoedApexSome = EchoShape(apexSome);
        Require(
            echoedApexSome is Shape.Apex asome && asome.Tip == new Point(3.0, 4.0),
            "EchoShape(Apex with Some(Point))"
        );

        Shape apexNone = new Shape.Apex(null);
        Shape echoedApexNone = EchoShape(apexNone);
        Require(
            echoedApexNone is Shape.Apex anone && anone.Tip is null,
            "EchoShape(Apex with None)"
        );

        // Cluster — Vec<Point> as a variant field, same shadow setup.
        // Drives the scoped rendering of the ReadEncodedArray / blittable
        // array element type inside the Shape scope.
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
        Require(MakeCircle(2.0) is Shape.Circle c2 && c2.Radius == 2.0, "MakeCircle");
        Require(
            MakeRectangle(5.0, 10.0) is Shape.Rectangle r2 && r2.Width == 5.0 && r2.Height == 10.0,
            "MakeRectangle"
        );

        // Instance methods on the data enum — wire-encode self, call
        // native, decode return.
        Require(Math.Abs(new Shape.Circle(1.0).Area() - Math.PI) < 1e-9, "Circle(1).Area() == PI");
        Require(new Shape.Rectangle(3.0, 4.0).Area() == 12.0, "Rectangle(3,4).Area()");
        Require(new Shape.Point().Area() == 0.0, "Point.Area() == 0");
        Require(new Shape.Circle(2.0).Describe() == "circle r=2", "Circle.Describe()");
        Require(new Shape.Point().Describe() == "point", "Point.Describe()");

        // Static methods / factories on the data enum.
        Require(Shape.UnitCircle() is Shape.Circle uc && uc.Radius == 1.0, "Shape.UnitCircle()");
        Require(
            Shape.Square(7.0) is Shape.Rectangle sq && sq.Width == 7.0 && sq.Height == 7.0,
            "Shape.Square(7)"
        );
        Require(Shape.VariantCount() == 6u, "Shape.VariantCount() == 6");
        Require(Shape.New(3.0) is Shape.Circle sn && sn.Radius == 3.0, "Shape.New(3)");

        // TryApexPoint — static method whose return type is Option<Point>
        // where Point is shadowed by a sibling variant. Drives scoped
        // rendering of the Option decode inside the Shape scope.
        Point? apexPt = Shape.TryApexPoint(2.5);
        Require(apexPt is { } pt && pt.X == 0.0 && pt.Y == 2.5, "Shape.TryApexPoint(positive)");
        Require(Shape.TryApexPoint(-1.0) is null, "Shape.TryApexPoint(negative) == null");

        // Message — mixes string, primitive, and unit variants.
        Message text = new Message.Text("hello");
        Require(
            EchoMessage(text) is Message.Text et && et.Body == "hello",
            "EchoMessage(Text)"
        );

        Message image = new Message.Image("https://example.com/a.png", 1920, 1080);
        Require(
            EchoMessage(image) is Message.Image ei
                && ei.Url == "https://example.com/a.png"
                && ei.Width == 1920u
                && ei.Height == 1080u,
            "EchoMessage(Image)"
        );

        Message ping = new Message.Ping();
        Require(EchoMessage(ping) is Message.Ping, "EchoMessage(Ping)");

        Require(
            MessageSummary(new Message.Text("hi")) == "text: hi",
            "MessageSummary(Text)"
        );
        Require(MessageSummary(new Message.Ping()) == "ping", "MessageSummary(Ping)");

        // Animal — three struct variants, one with a bool field.
        Animal dog = new Animal.Dog("Rex", "Labrador");
        Require(
            EchoAnimal(dog) is Animal.Dog d && d.Name == "Rex" && d.Breed == "Labrador",
            "EchoAnimal(Dog)"
        );

        Animal cat = new Animal.Cat("Whiskers", true);
        Require(
            EchoAnimal(cat) is Animal.Cat ca && ca.Name == "Whiskers" && ca.Indoor,
            "EchoAnimal(Cat indoor)"
        );

        Animal fish = new Animal.Fish(3u);
        Require(
            EchoAnimal(fish) is Animal.Fish f && f.Count == 3u,
            "EchoAnimal(Fish)"
        );

        Require(AnimalName(new Animal.Dog("Rex", "Lab")) == "Rex", "AnimalName(Dog)");
        Require(AnimalName(new Animal.Fish(5u)) == "5 fish", "AnimalName(Fish)");

        // LifecycleEvent — a data enum whose variant payload carries a
        // C-style enum (Priority). The codec must wire-encode the outer
        // variant tag and the inner enum's backing integer together.
        LifecycleEvent started = MakeCriticalLifecycleEvent(7);
        Require(
            started is LifecycleEvent.TaskStarted ts
                && ts.Priority == Priority.Critical
                && ts.Id == 7,
            "MakeCriticalLifecycleEvent returns TaskStarted with Critical priority"
        );
        LifecycleEvent echoedStarted = EchoLifecycleEvent(started);
        Require(echoedStarted == started, "EchoLifecycleEvent(TaskStarted) round-trip");
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
        global::Demo.Task echoedTask = EchoTask(task);
        Require(echoedTask == task, "EchoTask round-trip");
        Require(echoedTask.Priority == Priority.High, "Task.Priority preserved");

        Notification notification = new Notification("Build failed", Priority.Critical, false);
        Notification echoedNotification = EchoNotification(notification);
        Require(echoedNotification == notification, "EchoNotification round-trip");
        Require(echoedNotification.Priority == Priority.Critical, "Notification.Priority preserved");
        Require(!echoedNotification.Read, "Notification.Read preserved");

        // Holder is #[repr(C)] but wraps a data enum (Shape). Data enums
        // have a variable-width on-the-wire representation — this record
        // must ride the wire codec, not direct P/Invoke, despite the
        // repr(C) decoration.
        Holder triangle = MakeTriangleHolder();
        Require(
            triangle.Shape is Shape.Triangle t
                && t.A == new Point(0.0, 0.0)
                && t.B == new Point(4.0, 0.0)
                && t.C == new Point(0.0, 3.0),
            "MakeTriangleHolder returns Triangle"
        );
        Holder echoedHolder = EchoHolder(triangle);
        Require(echoedHolder == triangle, "EchoHolder round-trip");

        // TaskHeader is #[repr(C)] with primitive + C-style enum fields,
        // but rides the wire codec like any record with a non-primitive
        // field: the Rust #[export] macro doesn't yet admit C-style enums
        // as layout-compatible primitives, so both sides agree on wire
        // encoding. Follow-up work (see TaskHeader doc) can widen both
        // sides together to lift this onto direct P/Invoke.
        TaskHeader header = MakeCriticalTaskHeader(42);
        Require(header.Id == 42, "MakeCriticalTaskHeader.Id");
        Require(header.Priority == Priority.Critical, "MakeCriticalTaskHeader.Priority");
        Require(!header.Completed, "MakeCriticalTaskHeader.Completed");
        TaskHeader echoedHeader = EchoTaskHeader(header);
        Require(echoedHeader == header, "EchoTaskHeader round-trip");

        // LogEntry — same family as TaskHeader but the C-style enum field
        // is u8-backed, so field alignment matters. Wire-encoded today for
        // the same reason TaskHeader is.
        LogEntry entry = MakeErrorLogEntry(1234567890, 42);
        Require(entry.Timestamp == 1234567890, "MakeErrorLogEntry.Timestamp");
        Require(entry.Level == LogLevel.Error, "MakeErrorLogEntry.Level");
        Require(entry.Code == 42, "MakeErrorLogEntry.Code");
        LogEntry echoedEntry = EchoLogEntry(entry);
        Require(echoedEntry == entry, "EchoLogEntry round-trip");

        Console.WriteLine("  PASS\n");
    }

    private static void TestPrimitiveVecs()
    {
        Console.WriteLine("Testing primitive vecs...");

        int[] echoedI32 = EchoVecI32(new int[] { 1, 2, 3 });
        Require(echoedI32.SequenceEqual(new[] { 1, 2, 3 }), "echoVecI32");
        Require(EchoVecI32(Array.Empty<int>()).Length == 0, "echoVecI32 empty");

        Require(EchoVecI8(new sbyte[] { -1, 0, 7 }).SequenceEqual(new sbyte[] { -1, 0, 7 }), "echoVecI8");
        Require(EchoVecU8(new byte[] { 0, 1, 2, 3 }).SequenceEqual(new byte[] { 0, 1, 2, 3 }), "echoVecU8");
        Require(EchoVecI16(new short[] { -3, 0, 9 }).SequenceEqual(new short[] { -3, 0, 9 }), "echoVecI16");
        Require(EchoVecU16(new ushort[] { 0, 10, 20 }).SequenceEqual(new ushort[] { 0, 10, 20 }), "echoVecU16");
        Require(EchoVecU32(new uint[] { 0, 10, 20 }).SequenceEqual(new uint[] { 0, 10, 20 }), "echoVecU32");
        Require(EchoVecI64(new long[] { -5L, 0L, 8L }).SequenceEqual(new long[] { -5L, 0L, 8L }), "echoVecI64");
        Require(EchoVecU64(new ulong[] { 0UL, 1UL, 2UL }).SequenceEqual(new ulong[] { 0UL, 1UL, 2UL }), "echoVecU64");
        Require(EchoVecIsize(new nint[] { -2, 0, 5 }).SequenceEqual(new nint[] { -2, 0, 5 }), "echoVecIsize");
        Require(EchoVecUsize(new nuint[] { 0, 2, 4 }).SequenceEqual(new nuint[] { 0, 2, 4 }), "echoVecUsize");
        Require(EchoVecF32(new float[] { 1.25f, -2.5f }).SequenceEqual(new float[] { 1.25f, -2.5f }), "echoVecF32");
        Require(EchoVecF64(new double[] { 1.5, 2.5 }).SequenceEqual(new double[] { 1.5, 2.5 }), "echoVecF64");
        Require(EchoVecBool(new bool[] { true, false, true }).SequenceEqual(new bool[] { true, false, true }), "echoVecBool");

        Require(SumVecI32(new int[] { 10, 20, 30 }) == 60L, "sumVecI32");
        Require(SumVecI32(Array.Empty<int>()) == 0L, "sumVecI32 empty");

        Require(MakeRange(0, 5).SequenceEqual(new int[] { 0, 1, 2, 3, 4 }), "makeRange");
        Require(ReverseVecI32(new int[] { 1, 2, 3 }).SequenceEqual(new int[] { 3, 2, 1 }), "reverseVecI32");
        Require(GenerateI32Vec(4).SequenceEqual(new int[] { 0, 1, 2, 3 }), "generateI32Vec");
        Require(GenerateF64Vec(3).Length == 3, "generateF64Vec length");
        Require(Math.Abs(SumF64Vec(new double[] { 0.5, 1.5, 2.0 }) - 4.0) < 1e-9, "sumF64Vec");

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
        string[] echoedWords = EchoVecString(words);
        Require(echoedWords.SequenceEqual(words), "echoVecString round-trip");
        Require(EchoVecString(Array.Empty<string>()).Length == 0, "echoVecString empty");

        uint[] lengths = VecStringLengths(new[] { "", "a", "café", "🌍" });
        Require(lengths.SequenceEqual(new uint[] { 0u, 1u, 5u, 4u }), "vecStringLengths UTF-8 byte counts");

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
        Require(EchoVecVecI32(Array.Empty<int[]>()).Length == 0, "echoVecVecI32 empty outer");

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

        int[] flattened = FlattenVecVecI32(nestedInts);
        Require(flattened.SequenceEqual(new[] { 1, 2, 3, -1 }), "flattenVecVecI32");

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

        Location[] locations = GenerateLocations(3);
        Require(locations.Length == 3, "generateLocations length");
        Require(locations[0].Id == 0L, "locations[0].Id");
        Require(locations[0].Rating == 3.0, "locations[0].Rating");
        Require(locations[0].IsOpen, "locations[0].IsOpen");
        Require(locations[1].Id == 1L, "locations[1].Id");
        Require(!locations[1].IsOpen, "locations[1].IsOpen");
        Require(locations[2].ReviewCount == 20, "locations[2].ReviewCount");

        Require(ProcessLocations(locations) == 3, "processLocations roundtrip");
        Require(ProcessLocations(Array.Empty<Location>()) == 0, "processLocations empty");
        Require(Math.Abs(SumRatings(locations) - (3.0 + 3.1 + 3.2)) < 1e-9, "sumRatings roundtrip");

        Trade[] trades = GenerateTrades(3);
        Require(trades.Length == 3, "generateTrades length");
        Require(trades[0].Volume == 0L && trades[1].Volume == 1000L && trades[2].Volume == 2000L, "trades volumes");
        Require(SumTradeVolumes(trades) == 3000L, "sumTradeVolumes roundtrip");
        Require(AggregateLocationTradeStats(locations, trades) == 3002L, "aggregateLocationTradeStats two pinned arrays");

        Particle[] particles = GenerateParticles(3);
        Require(particles.Length == 3, "generateParticles length");
        Require(Math.Abs(SumParticleMasses(particles) - (1.0 + 1.001 + 1.002)) < 1e-9, "sumParticleMasses roundtrip");

        SensorReading[] readings = GenerateSensorReadings(3);
        Require(readings.Length == 3, "generateSensorReadings length");
        Require(Math.Abs(AvgSensorTemperature(readings) - 21.0) < 1e-9, "avgSensorTemperature roundtrip");
        Require(AvgSensorTemperature(Array.Empty<SensorReading>()) == 0.0, "avgSensorTemperature empty");

        // Construct a Location[] in C# and pass it to native code. Exercises
        // the param direction independently of the round-trip: if the CLR's
        // struct layout drifts from Rust's, SumRatings will see garbage.
        Location[] handmade = new[]
        {
            new Location(100L, 40.0, -70.0, 2.5, 5, true),
            new Location(101L, 40.5, -70.5, 4.0, 50, false),
        };
        Require(ProcessLocations(handmade) == 2, "processLocations handmade");
        Require(Math.Abs(SumRatings(handmade) - 6.5) < 1e-9, "sumRatings handmade");

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
        Status[] echoedStatuses = EchoVecStatus(statuses);
        Require(echoedStatuses.SequenceEqual(statuses), "echoVecStatus round-trip");
        Require(EchoVecStatus(Array.Empty<Status>()).Length == 0, "echoVecStatus empty");

        Direction[] generated = GenerateDirections(6);
        Require(generated.Length == 6, "generateDirections length");
        Require(generated[0] == Direction.North && generated[4] == Direction.North, "generateDirections wraps the 4-direction cycle");
        Require(CountNorth(generated) == 2, "countNorth on generateDirections(6)");
        Require(CountNorth(Array.Empty<Direction>()) == 0, "countNorth empty");

        LogLevel[] levels = new[] { LogLevel.Trace, LogLevel.Warn, LogLevel.Error, LogLevel.Debug };
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

        Polygon triangle = new Polygon(new[]
        {
            new Point(0.0, 0.0),
            new Point(4.0, 0.0),
            new Point(0.0, 3.0),
        });
        Polygon echoedTriangle = EchoPolygon(triangle);
        Require(echoedTriangle.Points.SequenceEqual(triangle.Points), "echoPolygon round-trip");
        Require(PolygonVertexCount(triangle) == 3u, "polygonVertexCount");
        Point centroid = PolygonCentroid(triangle);
        Require(Math.Abs(centroid.X - 4.0 / 3.0) < 1e-9 && Math.Abs(centroid.Y - 1.0) < 1e-9, "polygonCentroid");
        Polygon built = MakePolygon(triangle.Points);
        Require(built.Points.SequenceEqual(triangle.Points), "makePolygon");
        Require(EchoPolygon(new Polygon(Array.Empty<Point>())).Points.Length == 0, "echoPolygon empty");

        Team team = new Team("Alpha", new[] { "café", "🌍", "common" });
        Team echoedTeam = EchoTeam(team);
        Require(echoedTeam.Name == team.Name, "echoTeam name");
        Require(echoedTeam.Members.SequenceEqual(team.Members), "echoTeam members utf-8 round-trip");
        Require(TeamSize(team) == 3u, "teamSize");
        Team built2 = MakeTeam("Beta", new[] { "x", "y" });
        Require(built2.Name == "Beta" && built2.Members.SequenceEqual(new[] { "x", "y" }), "makeTeam");
        Require(EchoTeam(new Team("Empty", Array.Empty<string>())).Members.Length == 0, "echoTeam empty members");

        Classroom classroom = new Classroom(new[]
        {
            new Person("café", 7u),
            new Person("🌍", 42u),
        });
        Classroom echoedClass = EchoClassroom(classroom);
        Require(echoedClass.Students.SequenceEqual(classroom.Students), "echoClassroom utf-8 round-trip");
        Classroom built3 = MakeClassroom(classroom.Students);
        Require(built3.Students.SequenceEqual(classroom.Students), "makeClassroom (Vec<NonBlittableRecord> param)");
        Require(EchoClassroom(new Classroom(Array.Empty<Person>())).Students.Length == 0, "echoClassroom empty");

        TaggedScores scores = new TaggedScores("quiz", new[] { 10.0, 20.0, 30.0 });
        TaggedScores echoedScores = EchoTaggedScores(scores);
        Require(echoedScores.Label == "quiz" && echoedScores.Scores.SequenceEqual(scores.Scores), "echoTaggedScores");
        Require(Math.Abs(AverageScore(scores) - 20.0) < 1e-9, "averageScore");
        Require(AverageScore(new TaggedScores("empty", Array.Empty<double>())) == 0.0, "averageScore empty");

        Filter byTags = new Filter.ByTags(new[] { "café", "🌍" });
        Filter echoedTags = EchoFilter(byTags);
        Require(echoedTags is Filter.ByTags t && t.Tags.SequenceEqual(((Filter.ByTags)byTags).Tags), "echoFilter ByTags");
        Require(DescribeFilter(byTags) == "filter by 2 tags", "describeFilter ByTags");

        Filter byGroups = new Filter.ByGroups(
            new[]
            {
                new[] { "café", "🌍" },
                Array.Empty<string>(),
                new[] { "common" },
            }
        );
        Filter echoedGroups = EchoFilter(byGroups);
        Require(echoedGroups is Filter.ByGroups g && g.Groups.Length == 3, "echoFilter ByGroups outer length");
        Require(
            echoedGroups is Filter.ByGroups g0
                && g0.Groups[0].SequenceEqual(((Filter.ByGroups)byGroups).Groups[0])
                && g0.Groups[1].SequenceEqual(((Filter.ByGroups)byGroups).Groups[1])
                && g0.Groups[2].SequenceEqual(((Filter.ByGroups)byGroups).Groups[2]),
            "echoFilter ByGroups nested strings"
        );
        Require(DescribeFilter(byGroups) == "filter by 3 groups", "describeFilter ByGroups");

        Filter byPoints = new Filter.ByPoints(new[] { new Point(1.0, 2.0), new Point(3.0, 4.0) });
        Filter echoedPts = EchoFilter(byPoints);
        Require(echoedPts is Filter.ByPoints p2 && p2.Anchors.SequenceEqual(((Filter.ByPoints)byPoints).Anchors), "echoFilter ByPoints");
        Require(DescribeFilter(byPoints) == "filter by 2 anchor points", "describeFilter ByPoints");

        BenchmarkUserProfile[] profiles = GenerateUserProfiles(4);
        Require(profiles.Length == 4, "generateUserProfiles length");
        Require(profiles[0].Tags.Length == 3 && profiles[0].Scores.Length == 3, "generateUserProfiles inner vec shapes");
        Require(profiles[0].IsActive && !profiles[1].IsActive, "generateUserProfiles is_active pattern");
        double expectedSum = 0.0 + 1.5 + 3.0 + 4.5;
        Require(Math.Abs(SumUserScores(profiles) - expectedSum) < 1e-9, "sumUserScores round-trip");
        Require(CountActiveUsers(profiles) == 2, "countActiveUsers (even indices active)");
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

        Require(EchoOptionalI32(42) == 42, "EchoOptionalI32(Some)");
        Require(EchoOptionalI32(null) == null, "EchoOptionalI32(None)");
        Require(EchoOptionalI32(int.MinValue) == int.MinValue, "EchoOptionalI32(min)");
        Require(EchoOptionalI32(int.MaxValue) == int.MaxValue, "EchoOptionalI32(max)");

        Require(EchoOptionalF64(3.14) == 3.14, "EchoOptionalF64(Some)");
        Require(EchoOptionalF64(null) == null, "EchoOptionalF64(None)");

        Require(EchoOptionalBool(true) == true, "EchoOptionalBool(true)");
        Require(EchoOptionalBool(false) == false, "EchoOptionalBool(false)");
        Require(EchoOptionalBool(null) == null, "EchoOptionalBool(None)");

        Require(UnwrapOrDefaultI32(10, 99) == 10, "UnwrapOrDefaultI32(Some)");
        Require(UnwrapOrDefaultI32(null, 99) == 99, "UnwrapOrDefaultI32(None) falls back");

        Require(MakeSomeI32(7) == 7, "MakeSomeI32 returns Some");
        Require(MakeNoneI32() == null, "MakeNoneI32 returns null");

        Require(DoubleIfSome(5) == 10, "DoubleIfSome(Some)");
        Require(DoubleIfSome(null) == null, "DoubleIfSome(None) stays None");

        Require(FindEven(4) == 4, "FindEven(4) == Some(4)");
        Require(FindEven(3) == null, "FindEven(3) == None");

        Require(FindPositiveI64(100L) == 100L, "FindPositiveI64(100)");
        Require(FindPositiveI64(-1L) == null, "FindPositiveI64(-1) == None");
        Require(FindPositiveI64(0L) == null, "FindPositiveI64(0) == None");

        Require(FindPositiveF64(1.5) == 1.5, "FindPositiveF64(1.5)");
        Require(FindPositiveF64(-0.5) == null, "FindPositiveF64(-0.5) == None");

        // Option<String>: reference-type inner rides the same 1-byte tag
        // path; the payload is a length-prefixed UTF-8 buffer. café
        // exercises 2-byte codepoints, 🌍 exercises 4-byte ones.
        Require(EchoOptionalString("hello") == "hello", "EchoOptionalString(Some ascii)");
        Require(EchoOptionalString("café") == "café", "EchoOptionalString(2-byte UTF-8)");
        Require(EchoOptionalString("🌍") == "🌍", "EchoOptionalString(4-byte UTF-8)");
        Require(EchoOptionalString("") == "", "EchoOptionalString(empty Some)");
        Require(EchoOptionalString(null) == null, "EchoOptionalString(None)");

        Require(IsSomeString("x"), "IsSomeString(Some)");
        Require(!IsSomeString(null), "IsSomeString(None)");

        Require(FindName(7) == "Name_7", "FindName(positive) returns Some");
        Require(FindName(-1) == null, "FindName(non-positive) returns null");

        // Option<BlittableRecord>: Point is #[repr(C)] with two f64
        // fields, so the inner payload is 16 raw bytes written via
        // Point.WireEncodeTo and read via Point.Decode — no layout
        // shortcut, because the 1-byte tag forces the wire path.
        Require(EchoOptionalPoint(new Point(1.5, 2.5)) == new Point(1.5, 2.5), "EchoOptionalPoint(Some)");
        Require(EchoOptionalPoint(null) == null, "EchoOptionalPoint(None)");

        Require(MakeSomePoint(3.0, 4.0) == new Point(3.0, 4.0), "MakeSomePoint returns Some");
        Require(MakeNonePoint() == null, "MakeNonePoint returns null");

        // Option<CStyleEnum>: Status crosses the wire as a 4-byte i32
        // tag under an Option — the CLR can't reuse its direct
        // marshaling path because of the outer 1-byte present tag.
        Require(EchoOptionalStatus(Status.Active) == Status.Active, "EchoOptionalStatus(Active)");
        Require(EchoOptionalStatus(Status.Pending) == Status.Pending, "EchoOptionalStatus(Pending)");
        Require(EchoOptionalStatus(null) == null, "EchoOptionalStatus(None)");

        // Option<DataEnum>: ApiResult has unit, tuple, and struct
        // variants — the decode inside the Option's ternary must
        // still dispatch to the right variant.
        Require(
            FindApiResult(0) is ApiResult.Success,
            "FindApiResult(0) returns Success"
        );
        Require(
            FindApiResult(1) is ApiResult.ErrorCode ec && ec.Value0 == -1,
            "FindApiResult(1) returns ErrorCode(-1)"
        );
        Require(
            FindApiResult(2) is ApiResult.ErrorWithData ewd && ewd.Code == -1 && ewd.Detail == -2,
            "FindApiResult(2) returns ErrorWithData"
        );
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
        UserProfile alice = MakeUserProfile("Alice", 30u, "alice@example.com", 92.5);
        Require(alice.Name == "Alice", "MakeUserProfile.Name");
        Require(alice.Age == 30u, "MakeUserProfile.Age");
        Require(alice.Email == "alice@example.com", "MakeUserProfile.Email(Some)");
        Require(alice.Score == 92.5, "MakeUserProfile.Score(Some)");

        UserProfile newUser = MakeUserProfile("Bob", 25u, null, null);
        Require(newUser.Email == null, "MakeUserProfile.Email(None)");
        Require(newUser.Score == null, "MakeUserProfile.Score(None)");

        UserProfile echoed = EchoUserProfile(alice);
        Require(echoed == alice, "EchoUserProfile round-trip (all fields Some)");

        UserProfile echoedNew = EchoUserProfile(newUser);
        Require(echoedNew == newUser, "EchoUserProfile round-trip (Option fields None)");

        // Mixed present/absent: one Option field is Some, the other is None.
        UserProfile mixed = MakeUserProfile("Carol", 40u, "carol@example.com", null);
        Require(mixed.Email == "carol@example.com", "MakeUserProfile.Email(Some) with Score(None)");
        Require(mixed.Score == null, "MakeUserProfile.Score(None) with Email(Some)");
        Require(EchoUserProfile(mixed) == mixed, "EchoUserProfile round-trip (mixed Option fields)");

        // UTF-8 sentinels inside the optional string field.
        UserProfile emoji = MakeUserProfile("🌍 User", 42u, "café@example.com", 3.14);
        UserProfile echoedEmoji = EchoUserProfile(emoji);
        Require(echoedEmoji == emoji, "EchoUserProfile round-trip (UTF-8 in Option fields)");

        Require(
            UserDisplayName(alice) == "Alice <alice@example.com>",
            "UserDisplayName when Email is Some"
        );
        Require(UserDisplayName(newUser) == "Bob", "UserDisplayName when Email is None");

        // SearchResult: second record shape with Option fields, exercises
        // the same code path through a different record class name to
        // catch any accidental per-record coupling in the generator.
        SearchResult hits = new SearchResult("cats", 42u, "cursor_abc", 0.97);
        Require(EchoSearchResult(hits) == hits, "EchoSearchResult round-trip (all Some)");
        Require(HasMoreResults(hits), "HasMoreResults true when NextCursor is Some");

        SearchResult tail = new SearchResult("cats", 42u, null, null);
        Require(EchoSearchResult(tail) == tail, "EchoSearchResult round-trip (Option fields None)");
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
        var numbers = EchoOptionalVec(new[] { 1, 2, 3 });
        Require(numbers != null && numbers.SequenceEqual(new[] { 1, 2, 3 }), "EchoOptionalVec(Some)");
        Require(
            EchoOptionalVec(Array.Empty<int>())!.Length == 0,
            "EchoOptionalVec(Some empty) stays Some"
        );
        Require(EchoOptionalVec(null) == null, "EchoOptionalVec(None)");

        Require(OptionalVecLength(new[] { 10, 20, 30 }) == 3u, "OptionalVecLength(Some)");
        Require(OptionalVecLength(null) == null, "OptionalVecLength(None)");

        // Option<Vec<_>>-returning functions: the wire return is
        // FfiBuf, decoded through ReadU8() + ReadLengthPrefixedBlittableArray
        // (primitive elements) or ReadEncodedArray (variable-width).
        Require(
            FindNumbers(3)!.SequenceEqual(new[] { 0, 1, 2 }),
            "FindNumbers(positive) returns Some(vec)"
        );
        Require(FindNumbers(-1) == null, "FindNumbers(non-positive) returns null");

        var names = FindNames(3);
        Require(
            names != null && names.SequenceEqual(new[] { "Name_0", "Name_1", "Name_2" }),
            "FindNames(positive) returns Some(vec of strings)"
        );
        Require(FindNames(0) == null, "FindNames(zero) returns null");

        // Vec<Option<T>>: new fixture. Each element carries its own
        // Option tag, so the wire shape is: count (i32), then for each
        // slot, 1-byte tag + optional i32 payload. Mixed Some/None
        // positions in one vec surface any off-by-one errors.
        int?[] mixed = new int?[] { 1, null, 3, null, 5 };
        int?[] echoed = EchoVecOptionalI32(mixed);
        Require(echoed.Length == mixed.Length, "EchoVecOptionalI32 preserves length");
        for (int i = 0; i < mixed.Length; i++)
        {
            Require(echoed[i] == mixed[i], $"EchoVecOptionalI32[{i}] preserves presence and value");
        }

        Require(EchoVecOptionalI32(Array.Empty<int?>()).Length == 0, "EchoVecOptionalI32 empty");
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
        Require(SafeDivide(10, 2) == 5, "SafeDivide(10, 2) returns 5");
        // Err path throws BoltException carrying the Rust error string.
        try
        {
            SafeDivide(10, 0);
            Require(false, "SafeDivide(10, 0) should throw");
        }
        catch (BoltException e)
        {
            Require(e.Message.Contains("division by zero"), "SafeDivide error message");
        }

        Require(AlwaysOk(21) == 42, "AlwaysOk doubles its input");
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
        Point p = ParsePoint("3.0,4.0");
        Require(p.X == 3.0 && p.Y == 4.0, "ParsePoint round-trips x,y");
        try
        {
            ParsePoint("bad");
            Require(false, "ParsePoint(bad) should throw");
        }
        catch (BoltException) { }

        // Result<String, String> with Ok carrying a wire-decoded String.
        Require(ResultOfString(1) == "item_1", "ResultOfString ok");
        try
        {
            ResultOfString(-1);
            Require(false, "ResultOfString(-1) should throw");
        }
        catch (BoltException) { }

        // Result<Option<i32>, String>: Some, None, then Err.
        Require(ResultOfOption(5) == 10, "ResultOfOption(5) returns Some(10)");
        Require(ResultOfOption(0) == null, "ResultOfOption(0) returns None");
        try
        {
            ResultOfOption(-1);
            Require(false, "ResultOfOption(-1) should throw");
        }
        catch (BoltException) { }

        // Result<Vec<i32>, String> Ok and Err.
        int[] vec = ResultOfVec(3);
        Require(vec.Length == 3 && vec[0] == 0 && vec[1] == 1 && vec[2] == 2, "ResultOfVec ok");
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
        Require(CheckedDivide(10, 2) == 5, "CheckedDivide(10, 2) ok");
        try
        {
            CheckedDivide(10, 0);
            Require(false, "CheckedDivide(10, 0) should throw");
        }
        catch (MathErrorException e)
        {
            Require(e.Error == MathError.DivisionByZero, "CheckedDivide typed error");
        }

        Require(CheckedSqrt(9.0) == 3.0, "CheckedSqrt(9) ok");
        try
        {
            CheckedSqrt(-1.0);
            Require(false, "CheckedSqrt(-1) should throw");
        }
        catch (MathErrorException e)
        {
            Require(e.Error == MathError.NegativeInput, "CheckedSqrt typed error");
        }

        Require(CheckedAdd(1, 2) == 3, "CheckedAdd(1, 2) ok");
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
        Require(ValidateUsername("alice") == "alice", "ValidateUsername ok");
        try
        {
            ValidateUsername("ab");
            Require(false, "ValidateUsername short should throw");
        }
        catch (ValidationErrorException e)
        {
            Require(e.Error == ValidationError.TooShort, "ValidateUsername TooShort");
        }
        try
        {
            ValidateUsername("a]bcdefghijklmnopqrstu");
            Require(false, "ValidateUsername long should throw");
        }
        catch (ValidationErrorException e)
        {
            Require(e.Error == ValidationError.TooLong, "ValidateUsername TooLong");
        }
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
        Require(MayFail(true) == "Success!", "MayFail(true) ok");
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

        Require(DivideApp(10, 2) == 5, "DivideApp ok");
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

        Console.WriteLine("  PASS\n");
    }

    private static async System.Threading.Tasks.Task TestAsyncFunctions()
    {
        Console.WriteLine("Testing async functions...");

        Require(await AsyncAdd(3, 7) == 10, "AsyncAdd(3, 7)");
        Require(await AsyncEcho("hello async") == "Echo: hello async", "AsyncEcho string return");
        Require((await AsyncDoubleAll(new[] { 1, 2, 3 })).SequenceEqual(new[] { 2, 4, 6 }),
            "AsyncDoubleAll primitive vec return");
        Require(await AsyncFindPositive(new[] { -1, 0, 5, 3 }) == 5, "AsyncFindPositive finds first positive");
        Require(await AsyncFindPositive(new[] { -3, -2, -1 }) == null, "AsyncFindPositive all-negative returns null");
        Require(await AsyncConcat(new[] { "a", "b", "c" }) == "a, b, c", "AsyncConcat Vec<String> param");
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

        MixedRecord echoed = await AsyncEchoMixedRecord(record);
        Require(echoed.Name == record.Name, "AsyncEchoMixedRecord.Name");
        Require(echoed.Anchor == record.Anchor, "AsyncEchoMixedRecord.Anchor");
        Require(echoed.Priority == record.Priority, "AsyncEchoMixedRecord.Priority");
        Require(echoed.Shape is Shape.Circle echoedCircle && echoedCircle.Radius == 2.0,
            "AsyncEchoMixedRecord.Shape");
        Require(echoed.Parameters.Tags.SequenceEqual(record.Parameters.Tags),
            "AsyncEchoMixedRecord.Parameters.Tags");

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

        Require(await TryComputeAsync(6) == 12, "TryComputeAsync success");
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

        Require(await FetchData(2) == 20, "FetchData(2) success");
        try
        {
            await FetchData(-1);
            Require(false, "FetchData(-1) should throw");
        }
        catch (BoltException e)
        {
            Require(e.Message.Contains("invalid id"), "FetchData(-1) BoltException");
        }

        Require(await AsyncSafeDivide(10, 2) == 5, "AsyncSafeDivide(10, 2)");
        try
        {
            await AsyncSafeDivide(10, 0);
            Require(false, "AsyncSafeDivide(10, 0) should throw");
        }
        catch (MathErrorException e)
        {
            Require(e.Error == MathError.DivisionByZero, "AsyncSafeDivide typed MathError");
        }

        Require(await AsyncFallibleFetch(3) == "value_3", "AsyncFallibleFetch(3)");
        try
        {
            await AsyncFallibleFetch(-1);
            Require(false, "AsyncFallibleFetch(-1) should throw");
        }
        catch (BoltException e)
        {
            Require(e.Message.Contains("invalid key"), "AsyncFallibleFetch negative-key BoltException");
        }

        Require(await AsyncFindValue(2) == 20, "AsyncFindValue(2)");
        Require(await AsyncFindValue(0) == null, "AsyncFindValue(0) returns null");
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
}
