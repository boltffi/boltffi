using System;
using Demo;
using static Demo.Demo;

namespace BoltFFI.Demo.Tests;

public static class DemoTest
{
    public static int Main()
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
        TestBlittableRecords();
        TestRecordsWithStrings();
        TestNestedRecords();
        TestCStyleEnums();
        TestDataEnums();
        TestRecordsWithEnumFields();
        TestPrimitiveVecs();
        TestStringAndNestedVecs();
        TestBlittableRecordVecs();
        TestEnumVecs();
        TestVecFields();
        Console.WriteLine("All tests passed!");
        return 0;
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
        Require(Shape.VariantCount() == 4u, "Shape.VariantCount() == 4");
        Require(Shape.New(3.0) is Shape.Circle sn && sn.Radius == 3.0, "Shape.New(3)");

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
        };
        Shape[] echoedShapes = EchoVecShape(shapes);
        Require(echoedShapes.Length == shapes.Length, "echoVecShape length");
        Require(echoedShapes.SequenceEqual(shapes), "echoVecShape round-trip preserves each variant");
        Require(EchoVecShape(Array.Empty<Shape>()).Length == 0, "echoVecShape empty");

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

    private static void Require(bool condition, string label)
    {
        if (!condition) throw new InvalidOperationException($"FAIL: {label}");
    }
}
