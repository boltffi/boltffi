package com.boltffi.demo;

import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Optional;
import java.util.concurrent.CompletableFuture;

public final class DemoTest {
    public static void main(String[] args) {
        System.out.println("Testing Java bindings...\n");
        testBool();
        testI32();
        testI64();
        testF32();
        testF64();
        testStrings();
        testCustomTypes();
        testPointRecords();
        testLineRecords();
        testPersonRecords();
        testRecordDefaultValues();
        testCStyleEnums();
        testDataEnums();
        testCStyleEnumVecs();
        testDataEnumVecs();
        testBytesVecs();
        testPrimitiveVecs();
        testVecStrings();
        testNestedVecs();
        testBlittableRecordVecs();
        testOptions();
        testRecordsWithVecs();
        testConstructorCoverageMatrix();
        testClosures();
        testSyncCallbacks();
        testAsyncCallbacks();
        testAsyncFunctions();
        testAsyncClassMethods();
        testSingleThreadedStateHolder();
        testResultFunctions();
        testResultClassMethods();
        testResultEnumErrors();
        testStreams();
        System.out.println("All tests passed!");
    }

    private static void testBool() {
        System.out.println("Testing bool...");
        assert Demo.echoBool(true);
        assert !Demo.echoBool(false);
        assert !Demo.negateBool(true);
        assert Demo.negateBool(false);
        System.out.println("  PASS\n");
    }

    private static void testI32() {
        System.out.println("Testing i32...");
        assert Demo.echoI32(42) == 42 : "echoI32(42)";
        assert Demo.echoI32(-100) == -100 : "echoI32(-100)";
        assert Demo.addI32(10, 20) == 30 : "addI32(10, 20)";
        System.out.println("  PASS\n");
    }

    private static void testI64() {
        System.out.println("Testing i64...");
        assert Demo.echoI64(9999999999L) == 9999999999L : "echoI64(large)";
        assert Demo.echoI64(-9999999999L) == -9999999999L : "echoI64(negative large)";
        System.out.println("  PASS\n");
    }

    private static void testF32() {
        System.out.println("Testing f32...");
        assert Math.abs(Demo.echoF32(3.14f) - 3.14f) < 0.001f : "echoF32(3.14)";
        assert Math.abs(Demo.addF32(1.5f, 2.5f) - 4.0f) < 0.001f : "addF32(1.5, 2.5)";
        System.out.println("  PASS\n");
    }

    private static void testF64() {
        System.out.println("Testing f64...");
        assert Math.abs(Demo.echoF64(3.14159265359) - 3.14159265359) < 0.0000001 : "echoF64(pi)";
        assert Math.abs(Demo.addF64(1.5, 2.5) - 4.0) < 0.0000001 : "addF64(1.5, 2.5)";
        System.out.println("  PASS\n");
    }

    private static void testStrings() {
        System.out.println("Testing strings...");
        assert Demo.echoString("hello").equals("hello") : "echoString(hello)";
        assert Demo.echoString("").equals("") : "echoString(empty)";
        assert Demo.echoString("café").equals("café") : "echoString(unicode)";
        assert Demo.echoString("日本語").equals("日本語") : "echoString(cjk)";
        assert Demo.echoString("hello 🌍 world").equals("hello 🌍 world") : "echoString(emoji)";
        assert Demo.concatStrings("foo", "bar").equals("foobar") : "concatStrings(foo, bar)";
        assert Demo.concatStrings("", "bar").equals("bar") : "concatStrings(empty, bar)";
        assert Demo.concatStrings("foo", "").equals("foo") : "concatStrings(foo, empty)";
        assert Demo.concatStrings("🎉", "🎊").equals("🎉🎊") : "concatStrings(emoji)";
        assert Demo.stringLength("hello") == 5 : "stringLength(hello)";
        assert Demo.stringLength("") == 0 : "stringLength(empty)";
        assert Demo.stringLength("café") == 5 : "stringLength(utf8 bytes)";
        assert Demo.stringLength("🌍") == 4 : "stringLength(emoji 4 bytes)";
        System.out.println("  PASS\n");
    }

    private static void testCustomTypes() {
        System.out.println("Testing custom types...");
        long timestamp = 1_710_000_000_000L;
        assert Demo.echoDatetime(timestamp) == timestamp : "echoDatetime";
        assert Demo.datetimeToMillis(timestamp) == timestamp : "datetimeToMillis";
        assert Demo.formatTimestamp(timestamp).startsWith("2024-03-") : "formatTimestamp";

        Event event = new Event("launch", timestamp);
        assert event.name().equals("launch") : "Event.name";
        assert event.timestamp() == timestamp : "Event.timestamp";

        Event echoed = Demo.echoEvent(event);
        assert echoed.name().equals("launch") : "echoEvent.name";
        assert echoed.timestamp() == timestamp : "echoEvent.timestamp";
        assert Demo.eventTimestamp(event) == timestamp : "eventTimestamp";
        System.out.println("  PASS\n");
    }

    private static void testPointRecords() {
        System.out.println("Testing records (Point)...");
        Point point = Demo.makePoint(1.0, 2.0);
        assert point.x() == 1.0 : "makePoint.x";
        assert point.y() == 2.0 : "makePoint.y";
        Point origin = Point.origin();
        assert origin.x() == 0.0 : "Point.origin.x";
        assert origin.y() == 0.0 : "Point.origin.y";
        Point fromPolar = Point.fromPolar(2.0, Math.PI / 2.0);
        assert Math.abs(fromPolar.x()) < 0.0001 : "Point.fromPolar.x";
        assert Math.abs(fromPolar.y() - 2.0) < 0.0001 : "Point.fromPolar.y";
        Point unit = Point.tryUnit(3.0, 4.0);
        assert Math.abs(unit.x() - 0.6) < 0.0001 : "Point.tryUnit.x";
        assert Math.abs(unit.y() - 0.8) < 0.0001 : "Point.tryUnit.y";
        try {
            Point.tryUnit(0.0, 0.0);
            assert false : "Point.tryUnit should throw for zero vector";
        } catch (RuntimeException expected) {
            assert expected.getMessage().contains("cannot normalize zero vector") : "Point.tryUnit error";
        }
        assert Point.checkedUnit(3.0, 4.0).isPresent() : "Point.checkedUnit some";
        assert !Point.checkedUnit(0.0, 0.0).isPresent() : "Point.checkedUnit none";
        assert Math.abs(point.distance() - Math.sqrt(5.0)) < 0.0001 : "Point.distance";
        Point scaledPoint = point.scale(2.5);
        assert scaledPoint.x() == 2.5 : "Point.scale.x";
        assert scaledPoint.y() == 5.0 : "Point.scale.y";
        Point addedPoint = point.add(new Point(10.0, 20.0));
        assert addedPoint.x() == 11.0 : "Point.add.x";
        assert addedPoint.y() == 22.0 : "Point.add.y";
        assert Point.dimensions() == 2 : "Point.dimensions";
        Point echoedPoint = Demo.echoPoint(point);
        assert echoedPoint.x() == 1.0 : "echoPoint.x";
        assert echoedPoint.y() == 2.0 : "echoPoint.y";
        Point sumPoint = Demo.addPoints(new Point(3.0, 4.0), new Point(5.0, 6.0));
        assert sumPoint.x() == 8.0 : "addPoints.x";
        assert sumPoint.y() == 10.0 : "addPoints.y";
        assert Math.abs(MathUtils.distanceBetween(new Point(0.0, 0.0), new Point(3.0, 4.0)) - 5.0) < 0.0001 : "MathUtils.distanceBetween";
        Point midpoint = MathUtils.midpoint(new Point(0.0, 0.0), new Point(2.0, 4.0));
        assert midpoint.x() == 1.0 : "MathUtils.midpoint.x";
        assert midpoint.y() == 2.0 : "MathUtils.midpoint.y";
        System.out.println("  PASS\n");
    }

    private static void testLineRecords() {
        System.out.println("Testing records (Line)...");
        Line line = Demo.makeLine(0.0, 0.0, 3.0, 4.0);
        assert line.start().x() == 0.0 : "makeLine.start.x";
        assert line.end().y() == 4.0 : "makeLine.end.y";
        Line echoedLine = Demo.echoLine(line);
        assert echoedLine.start().x() == 0.0 : "echoLine.start.x";
        assert echoedLine.end().x() == 3.0 : "echoLine.end.x";
        assert Math.abs(Demo.lineLength(line) - 5.0) < 0.0001 : "lineLength";
        System.out.println("  PASS\n");
    }

    private static void testPersonRecords() {
        System.out.println("Testing records (Person)...");
        Person person = Demo.makePerson("Alice", 30);
        assert person.name().equals("Alice") : "makePerson.name";
        assert person.age() == 30 : "makePerson.age";
        Person echoedPerson = Demo.echoPerson(person);
        assert echoedPerson.name().equals("Alice") : "echoPerson.name";
        assert echoedPerson.age() == 30 : "echoPerson.age";
        assert Demo.greetPerson(person).equals("Hello, Alice! You are 30 years old.") : "greetPerson";
        Person emojiPerson = Demo.makePerson("🎉 Party", 25);
        assert emojiPerson.name().equals("🎉 Party") : "makePerson(emoji)";
        Person echoedEmojiPerson = Demo.echoPerson(emojiPerson);
        assert echoedEmojiPerson.name().equals("🎉 Party") : "echoPerson(emoji)";
        System.out.println("  PASS\n");
    }

    private static void testRecordDefaultValues() {
        System.out.println("Testing records (default values)...");
        ServiceConfig implicitDefaults = new ServiceConfig("worker");
        assert implicitDefaults.name().equals("worker") : "ServiceConfig(name).name";
        assert implicitDefaults.retries() == 3 : "ServiceConfig(name).retries";
        assert implicitDefaults.region().equals("standard") : "ServiceConfig(name).region";
        assert !implicitDefaults.endpoint().isPresent() : "ServiceConfig(name).endpoint";
        assert implicitDefaults.backupEndpoint().isPresent() : "ServiceConfig(name).backupEndpoint";
        assert implicitDefaults.backupEndpoint().get().equals("https://default") : "ServiceConfig(name).backupEndpoint.value";

        ServiceConfig customRetries = new ServiceConfig("worker", 7);
        assert customRetries.name().equals("worker") : "ServiceConfig(name,retries).name";
        assert customRetries.retries() == 7 : "ServiceConfig(name,retries).retries";
        assert customRetries.region().equals("standard") : "ServiceConfig(name,retries).region";
        assert !customRetries.endpoint().isPresent() : "ServiceConfig(name,retries).endpoint";
        assert customRetries.backupEndpoint().isPresent() : "ServiceConfig(name,retries).backupEndpoint";
        assert customRetries.backupEndpoint().get().equals("https://default") : "ServiceConfig(name,retries).backupEndpoint.value";

        ServiceConfig explicitRegion = new ServiceConfig("worker", 9, "eu-west");
        assert !explicitRegion.endpoint().isPresent() : "ServiceConfig(name,retries,region).endpoint";
        assert explicitRegion.backupEndpoint().isPresent() : "ServiceConfig(name,retries,region).backupEndpoint";
        assert explicitRegion.backupEndpoint().get().equals("https://default") : "ServiceConfig(name,retries,region).backupEndpoint.value";

        ServiceConfig explicitEndpoint = new ServiceConfig("worker", 9, "eu-west", Optional.of("https://edge"));
        assert explicitEndpoint.backupEndpoint().isPresent() : "ServiceConfig(name,retries,region,endpoint).backupEndpoint";
        assert explicitEndpoint.backupEndpoint().get().equals("https://default") : "ServiceConfig(name,retries,region,endpoint).backupEndpoint.value";

        ServiceConfig explicitBackupEndpoint = new ServiceConfig(
            "worker",
            9,
            "eu-west",
            Optional.of("https://edge"),
            Optional.of("https://backup")
        );
        assert Demo.echoServiceConfig(explicitBackupEndpoint).equals(explicitBackupEndpoint) : "echoServiceConfig";
        assert implicitDefaults.describe().equals("worker:3:standard:none:https://default") : "ServiceConfig.describe(defaults)";
        assert customRetries.describe().equals("worker:7:standard:none:https://default") : "ServiceConfig.describe(customRetries)";
        assert explicitRegion.describe().equals("worker:9:eu-west:none:https://default") : "ServiceConfig.describe(explicitRegion)";
        assert explicitEndpoint.describe().equals("worker:9:eu-west:https://edge:https://default") : "ServiceConfig.describe(explicitEndpoint)";
        assert explicitBackupEndpoint.describe().equals("worker:9:eu-west:https://edge:https://backup") : "ServiceConfig.describe(explicitBackupEndpoint)";
        System.out.println("  PASS\n");
    }

    private static void testCStyleEnums() {
        System.out.println("Testing C-style enums...");

        assert Demo.echoStatus(Status.ACTIVE) == Status.ACTIVE : "echoStatus(Active)";
        assert Demo.echoStatus(Status.INACTIVE) == Status.INACTIVE : "echoStatus(Inactive)";
        assert Demo.echoStatus(Status.PENDING) == Status.PENDING : "echoStatus(Pending)";
        assert Demo.statusToString(Status.ACTIVE).equals("active") : "statusToString(Active)";
        assert Demo.statusToString(Status.INACTIVE).equals("inactive") : "statusToString(Inactive)";
        assert Demo.isActive(Status.ACTIVE) : "isActive(Active)";
        assert !Demo.isActive(Status.PENDING) : "isActive(Pending)";

        assert Demo.echoDirection(Direction.NORTH) == Direction.NORTH : "echoDirection(North)";
        assert Demo.oppositeDirection(Direction.NORTH) == Direction.SOUTH : "oppositeDirection(North)";
        assert Demo.oppositeDirection(Direction.EAST) == Direction.WEST : "oppositeDirection(East)";
        assert Direction.cardinal() == Direction.NORTH : "Direction.cardinal";
        assert Direction.fromDegrees(90.0) == Direction.EAST : "Direction.fromDegrees(90)";
        assert Direction.fromDegrees(225.0) == Direction.WEST : "Direction.fromDegrees(225)";
        assert Direction.NORTH.opposite() == Direction.SOUTH : "Direction.opposite";
        assert Direction.WEST.isHorizontal() : "Direction.isHorizontal(West)";
        assert !Direction.NORTH.isHorizontal() : "Direction.isHorizontal(North)";
        assert Direction.SOUTH.label().equals("S") : "Direction.label";
        assert Direction.count() == 4 : "Direction.count";

        assert Demo.echoPriority(Priority.HIGH) == Priority.HIGH : "echoPriority(High)";
        assert Demo.priorityLabel(Priority.LOW).equals("low") : "priorityLabel(Low)";
        assert Demo.isHighPriority(Priority.CRITICAL) : "isHighPriority(Critical)";
        assert !Demo.isHighPriority(Priority.LOW) : "isHighPriority(Low)";

        assert Demo.echoLogLevel(LogLevel.INFO) == LogLevel.INFO : "echoLogLevel(Info)";
        assert Demo.shouldLog(LogLevel.ERROR, LogLevel.WARN) : "shouldLog(Error >= Warn)";
        assert !Demo.shouldLog(LogLevel.DEBUG, LogLevel.INFO) : "shouldLog(Debug < Info)";

        assert HttpCode.OK.value == (short) 200 : "HttpCode.OK.value == 200";
        assert HttpCode.NOT_FOUND.value == (short) 404 : "HttpCode.NOT_FOUND.value == 404";
        assert HttpCode.SERVER_ERROR.value == (short) 500 : "HttpCode.SERVER_ERROR.value == 500";
        assert Demo.httpCodeNotFound() == HttpCode.NOT_FOUND : "httpCodeNotFound() == NOT_FOUND";
        assert Demo.echoHttpCode(HttpCode.OK) == HttpCode.OK : "echoHttpCode(OK)";
        assert Demo.echoHttpCode(HttpCode.SERVER_ERROR) == HttpCode.SERVER_ERROR : "echoHttpCode(SERVER_ERROR)";

        assert Sign.NEGATIVE.value == (byte) -1 : "Sign.NEGATIVE.value == -1";
        assert Sign.ZERO.value == (byte) 0 : "Sign.ZERO.value == 0";
        assert Sign.POSITIVE.value == (byte) 1 : "Sign.POSITIVE.value == 1";
        assert Demo.signNegative() == Sign.NEGATIVE : "signNegative() == NEGATIVE";
        assert Demo.echoSign(Sign.NEGATIVE) == Sign.NEGATIVE : "echoSign(NEGATIVE)";
        assert Demo.echoSign(Sign.POSITIVE) == Sign.POSITIVE : "echoSign(POSITIVE)";

        System.out.println("  PASS\n");
    }

    private static void testDataEnums() {
        System.out.println("Testing data enums...");

        Holder triangleHolder = Demo.makeTriangleHolder();
        assert triangleHolder.shape() instanceof Shape.Triangle : "Holder.shape is Triangle";
        Holder echoedHolder = Demo.echoHolder(triangleHolder);
        assert echoedHolder.equals(triangleHolder) : "echoHolder round-trip";

        TaskHeader header = Demo.makeCriticalTaskHeader(42L);
        assert header.id() == 42L : "TaskHeader.id";
        assert header.priority() == Priority.CRITICAL : "TaskHeader.priority";
        assert !header.completed() : "TaskHeader.completed";
        TaskHeader echoedHeader = Demo.echoTaskHeader(header);
        assert echoedHeader.equals(header) : "echoTaskHeader round-trip";

        LifecycleEvent started = Demo.makeCriticalLifecycleEvent(7L);
        assert started instanceof LifecycleEvent.TaskStarted : "LifecycleEvent.TaskStarted variant";
        LifecycleEvent.TaskStarted startedTs = (LifecycleEvent.TaskStarted) started;
        assert startedTs.priority == Priority.CRITICAL : "LifecycleEvent.TaskStarted.priority";
        assert startedTs.id == 7L : "LifecycleEvent.TaskStarted.id";
        assert Demo.echoLifecycleEvent(started).equals(started) : "echoLifecycleEvent(TaskStarted)";
        assert Demo.echoLifecycleEvent(LifecycleEvent.Tick.INSTANCE) instanceof LifecycleEvent.Tick : "echoLifecycleEvent(Tick)";

        LogEntry logEntry = Demo.makeErrorLogEntry(1234567890L, (short) 42);
        assert logEntry.timestamp() == 1234567890L : "LogEntry.timestamp";
        assert logEntry.level() == LogLevel.ERROR : "LogEntry.level";
        assert logEntry.code() == (short) 42 : "LogEntry.code";
        assert Demo.echoLogEntry(logEntry).equals(logEntry) : "echoLogEntry round-trip";

        Filter groupFilter = new Filter.ByGroups(
            Arrays.asList(
                Arrays.asList("café", "🌍"),
                Collections.emptyList(),
                Arrays.asList("common")
            )
        );
        assert Demo.echoFilter(groupFilter).equals(groupFilter) : "echoFilter(ByGroups)";
        assert Demo.describeFilter(groupFilter).equals("filter by 3 groups") : "describeFilter(ByGroups)";

        Shape circle = Demo.makeCircle(5.0);
        assert circle instanceof Shape.Circle : "makeCircle returns Circle";
        assert ((Shape.Circle) circle).radius == 5.0 : "makeCircle.radius";

        Shape rect = Demo.makeRectangle(3.0, 4.0);
        assert rect instanceof Shape.Rectangle : "makeRectangle returns Rectangle";
        Shape unitCircle = Shape.unitCircle();
        assert unitCircle instanceof Shape.Circle : "Shape.unitCircle type";
        assert Math.abs(((Shape.Circle) unitCircle).radius - 1.0) < 0.0001 : "Shape.unitCircle.radius";
        Shape square = Shape.square(3.0);
        assert square instanceof Shape.Rectangle : "Shape.square type";
        assert Math.abs(((Shape.Rectangle) square).width - 3.0) < 0.0001 : "Shape.square.width";
        assert Math.abs(((Shape.Rectangle) square).height - 3.0) < 0.0001 : "Shape.square.height";
        Shape checkedCircle = Shape.tryCircle(2.0);
        assert checkedCircle instanceof Shape.Circle : "Shape.tryCircle type";
        try {
            Shape.tryCircle(0.0);
            assert false : "Shape.tryCircle should throw on non-positive radius";
        } catch (RuntimeException expected) {
            assert expected.getMessage().contains("radius must be positive") : "Shape.tryCircle error";
        }
        assert Math.abs(circle.area() - (Math.PI * 25.0)) < 0.0001 : "Shape.area circle";
        assert Math.abs(rect.area() - 12.0) < 0.0001 : "Shape.area rectangle";
        assert circle.describe().equals("circle r=5") : "Shape.describe circle";
        assert rect.describe().equals("rect 3x4") : "Shape.describe rectangle";
        assert Shape.variantCount() == 4 : "Shape.variantCount";

        Shape echoedCircle = Demo.echoShape(circle);
        assert echoedCircle instanceof Shape.Circle : "echoShape(circle) type";
        assert ((Shape.Circle) echoedCircle).radius == 5.0 : "echoShape(circle).radius";

        Shape echoedRect = Demo.echoShape(rect);
        assert echoedRect instanceof Shape.Rectangle : "echoShape(rect) type";

        Shape triangle = Demo.echoShape(new Shape.Triangle(
            new Point(0.0, 0.0), new Point(3.0, 0.0), new Point(0.0, 4.0)
        ));
        assert triangle instanceof Shape.Triangle : "echoShape(triangle) type";

        Shape point = Demo.echoShape(Shape.Point.INSTANCE);
        assert point instanceof Shape.Point : "echoShape(point) type";

        Message text = Demo.echoMessage(new Message.Text("hello"));
        assert text instanceof Message.Text : "echoMessage(Text) type";
        assert ((Message.Text) text).body.equals("hello") : "echoMessage(Text).body";
        assert Demo.messageSummary(new Message.Text("hi")).equals("text: hi") : "messageSummary(Text)";
        assert Demo.messageSummary(Message.Ping.INSTANCE).equals("ping") : "messageSummary(Ping)";

        Animal dog = Demo.echoAnimal(new Animal.Dog("Rex", "Labrador"));
        assert dog instanceof Animal.Dog : "echoAnimal(Dog) type";
        assert ((Animal.Dog) dog).name.equals("Rex") : "echoAnimal(Dog).name";
        assert Demo.animalName(new Animal.Fish(5)).equals("5 fish") : "animalName(Fish)";

        ApiResponse success = Demo.echoApiResponse(new ApiResponse.Success("ok"));
        assert success instanceof ApiResponse.Success : "echoApiResponse(Success) type";
        assert Demo.isSuccess(new ApiResponse.Success("data")) : "isSuccess(Success)";
        assert !Demo.isSuccess(ApiResponse.Empty.INSTANCE) : "isSuccess(Empty)";

        System.out.println("  PASS\n");
    }

    private static void testCStyleEnumVecs() {
        System.out.println("Testing vec C-style enums...");

        List<Status> statuses = Demo.echoVecStatus(
            Arrays.asList(Status.ACTIVE, Status.PENDING, Status.INACTIVE)
        );
        assert statuses.size() == 3 : "echoVecStatus size";
        assert statuses.get(0) == Status.ACTIVE : "echoVecStatus[0]";
        assert statuses.get(1) == Status.PENDING : "echoVecStatus[1]";
        assert statuses.get(2) == Status.INACTIVE : "echoVecStatus[2]";

        List<LogLevel> levels = Demo.echoVecLogLevel(
            Arrays.asList(LogLevel.TRACE, LogLevel.INFO, LogLevel.ERROR)
        );
        assert levels.size() == 3 : "echoVecLogLevel size";
        assert levels.get(0) == LogLevel.TRACE : "echoVecLogLevel[0]";
        assert levels.get(1) == LogLevel.INFO : "echoVecLogLevel[1]";
        assert levels.get(2) == LogLevel.ERROR : "echoVecLogLevel[2]";

        System.out.println("  PASS\n");
    }

    private static void testDataEnumVecs() {
        System.out.println("Testing vec data enums...");

        List<Shape> shapes = Demo.echoVecShape(Arrays.asList(
            new Shape.Circle(2.0),
            new Shape.Rectangle(3.0, 4.0),
            Shape.Point.INSTANCE
        ));

        assert shapes.size() == 3 : "echoVecShape size";
        assert shapes.get(0) instanceof Shape.Circle : "echoVecShape[0] type";
        assert Math.abs(((Shape.Circle) shapes.get(0)).radius - 2.0) < 0.0001 : "echoVecShape[0].radius";
        assert shapes.get(1) instanceof Shape.Rectangle : "echoVecShape[1] type";
        assert shapes.get(2) instanceof Shape.Point : "echoVecShape[2] type";

        System.out.println("  PASS\n");
    }

    private static void testBytesVecs() {
        System.out.println("Testing vec bytes...\n");

        byte[] echoed = Demo.echoBytes(new byte[]{1, 2, 3, 4});
        assert echoed.length == 4 : "echoBytes length";
        assert echoed[0] == 1 && echoed[3] == 4 : "echoBytes values";

        assert Demo.bytesLength(new byte[]{10, 20, 30}) == 3 : "bytesLength";
        assert Demo.bytesSum(new byte[]{1, 2, 3, 4}) == 10 : "bytesSum";

        byte[] made = Demo.makeBytes(5);
        assert made.length == 5 : "makeBytes length";
        assert made[0] == 0 && made[4] == 4 : "makeBytes values";

        byte[] reversed = Demo.reverseBytes(new byte[]{5, 6, 7});
        assert reversed.length == 3 : "reverseBytes length";
        assert reversed[0] == 7 && reversed[2] == 5 : "reverseBytes values";

        System.out.println("  PASS\n");
    }

    private static void testPrimitiveVecs() {
        System.out.println("Testing primitive vecs...");

        int[] ints = Demo.echoVecI32(new int[]{1, 2, 3});
        assert ints.length == 3 : "echoVecI32 length";
        assert ints[0] == 1 && ints[1] == 2 && ints[2] == 3 : "echoVecI32 values";

        int[] empty = Demo.echoVecI32(new int[0]);
        assert empty.length == 0 : "echoVecI32 empty";

        assert Demo.sumVecI32(new int[]{10, 20, 30}) == 60L : "sumVecI32";
        assert Demo.sumVecI32(new int[0]) == 0L : "sumVecI32 empty";

        double[] doubles = Demo.echoVecF64(new double[]{1.5, 2.5});
        assert doubles.length == 2 : "echoVecF64 length";
        assert Math.abs(doubles[0] - 1.5) < 0.0001 : "echoVecF64[0]";
        assert Math.abs(doubles[1] - 2.5) < 0.0001 : "echoVecF64[1]";

        boolean[] bools = Demo.echoVecBool(new boolean[]{true, false, true});
        assert bools.length == 3 : "echoVecBool length";
        assert bools[0] && !bools[1] && bools[2] : "echoVecBool values";

        byte[] i8s = Demo.echoVecI8(new byte[]{-1, 0, 7});
        assert i8s.length == 3 : "echoVecI8 length";
        assert i8s[0] == -1 && i8s[2] == 7 : "echoVecI8 values";

        byte[] u8s = Demo.echoVecU8(new byte[]{0, 1, 2, 3});
        assert u8s.length == 4 : "echoVecU8 length";
        assert u8s[0] == 0 && u8s[3] == 3 : "echoVecU8 values";

        short[] i16s = Demo.echoVecI16(new short[]{-3, 0, 9});
        assert i16s.length == 3 : "echoVecI16 length";
        assert i16s[0] == -3 && i16s[2] == 9 : "echoVecI16 values";

        short[] u16s = Demo.echoVecU16(new short[]{0, 10, 20});
        assert u16s.length == 3 : "echoVecU16 length";
        assert u16s[0] == 0 && u16s[2] == 20 : "echoVecU16 values";

        int[] u32s = Demo.echoVecU32(new int[]{0, 10, 20});
        assert u32s.length == 3 : "echoVecU32 length";
        assert u32s[0] == 0 && u32s[2] == 20 : "echoVecU32 values";

        long[] i64s = Demo.echoVecI64(new long[]{-5L, 0L, 8L});
        assert i64s.length == 3 : "echoVecI64 length";
        assert i64s[0] == -5L && i64s[2] == 8L : "echoVecI64 values";

        long[] u64s = Demo.echoVecU64(new long[]{0L, 1L, 2L});
        assert u64s.length == 3 : "echoVecU64 length";
        assert u64s[0] == 0L && u64s[2] == 2L : "echoVecU64 values";

        long[] isizes = Demo.echoVecIsize(new long[]{-2L, 0L, 5L});
        assert isizes.length == 3 : "echoVecIsize length";
        assert isizes[0] == -2L && isizes[2] == 5L : "echoVecIsize values";

        long[] usizes = Demo.echoVecUsize(new long[]{0L, 2L, 4L});
        assert usizes.length == 3 : "echoVecUsize length";
        assert usizes[0] == 0L && usizes[2] == 4L : "echoVecUsize values";

        float[] f32s = Demo.echoVecF32(new float[]{1.25f, -2.5f});
        assert f32s.length == 2 : "echoVecF32 length";
        assert Math.abs(f32s[0] - 1.25f) < 0.0001f : "echoVecF32[0]";
        assert Math.abs(f32s[1] + 2.5f) < 0.0001f : "echoVecF32[1]";

        int[] range = Demo.makeRange(0, 5);
        assert range.length == 5 : "makeRange length";
        assert range[0] == 0 && range[4] == 4 : "makeRange values";

        int[] reversed = Demo.reverseVecI32(new int[]{1, 2, 3});
        assert reversed[0] == 3 && reversed[1] == 2 && reversed[2] == 1 : "reverseVecI32";

        System.out.println("  PASS\n");
    }

    private static void testVecStrings() {
        System.out.println("Testing vec strings...");

        List<String> strings = Demo.echoVecString(Arrays.asList("hello", "world"));
        assert strings.size() == 2 : "echoVecString size";
        assert strings.get(0).equals("hello") : "echoVecString[0]";
        assert strings.get(1).equals("world") : "echoVecString[1]";

        List<String> emptyStrings = Demo.echoVecString(Collections.emptyList());
        assert emptyStrings.isEmpty() : "echoVecString empty";

        int[] lengths = Demo.vecStringLengths(Arrays.asList("hi", "café"));
        assert lengths.length == 2 : "vecStringLengths size";
        assert lengths[0] == 2 : "vecStringLengths[0]";
        assert lengths[1] == 5 : "vecStringLengths[1] (utf8)";

        System.out.println("  PASS\n");
    }

    private static void testNestedVecs() {
        System.out.println("Testing nested vecs...");

        List<int[]> vvi = Demo.echoVecVecI32(Arrays.asList(new int[]{1, 2, 3}, new int[]{}, new int[]{4, 5}));
        assert vvi.size() == 3 : "echoVecVecI32 outer size";
        assert vvi.get(0).length == 3 && vvi.get(0)[0] == 1 && vvi.get(0)[2] == 3 : "echoVecVecI32[0]";
        assert vvi.get(1).length == 0 : "echoVecVecI32[1] empty";
        assert vvi.get(2).length == 2 && vvi.get(2)[0] == 4 && vvi.get(2)[1] == 5 : "echoVecVecI32[2]";

        List<int[]> vviEmpty = Demo.echoVecVecI32(Collections.emptyList());
        assert vviEmpty.isEmpty() : "echoVecVecI32 empty outer";

        List<boolean[]> vvb = Demo.echoVecVecBool(Arrays.asList(
                new boolean[]{true, false, true},
                new boolean[]{},
                new boolean[]{false}));
        assert vvb.size() == 3 : "echoVecVecBool outer size";
        assert vvb.get(0).length == 3 && vvb.get(0)[0] && !vvb.get(0)[1] && vvb.get(0)[2] : "echoVecVecBool[0]";
        assert vvb.get(1).length == 0 : "echoVecVecBool[1] empty";
        assert vvb.get(2).length == 1 && !vvb.get(2)[0] : "echoVecVecBool[2]";

        List<long[]> vvisize = Demo.echoVecVecIsize(Arrays.asList(
                new long[]{-2L, 0L, 5L},
                new long[]{},
                new long[]{9L}));
        assert vvisize.size() == 3 : "echoVecVecIsize outer size";
        assert vvisize.get(0).length == 3 && vvisize.get(0)[0] == -2L && vvisize.get(0)[2] == 5L : "echoVecVecIsize[0]";
        assert vvisize.get(1).length == 0 : "echoVecVecIsize[1] empty";
        assert vvisize.get(2).length == 1 && vvisize.get(2)[0] == 9L : "echoVecVecIsize[2]";

        List<long[]> vvusize = Demo.echoVecVecUsize(Arrays.asList(
                new long[]{0L, 2L, 4L},
                new long[]{},
                new long[]{8L}));
        assert vvusize.size() == 3 : "echoVecVecUsize outer size";
        assert vvusize.get(0).length == 3 && vvusize.get(0)[0] == 0L && vvusize.get(0)[2] == 4L : "echoVecVecUsize[0]";
        assert vvusize.get(1).length == 0 : "echoVecVecUsize[1] empty";
        assert vvusize.get(2).length == 1 && vvusize.get(2)[0] == 8L : "echoVecVecUsize[2]";

        List<List<String>> vvs = Demo.echoVecVecString(Arrays.asList(
                Arrays.asList("hello", "world"),
                Collections.emptyList(),
                Arrays.asList("café", "🌍")));
        assert vvs.size() == 3 : "echoVecVecString outer size";
        assert vvs.get(0).equals(Arrays.asList("hello", "world")) : "echoVecVecString[0]";
        assert vvs.get(1).isEmpty() : "echoVecVecString[1] empty";
        assert vvs.get(2).equals(Arrays.asList("café", "🌍")) : "echoVecVecString[2]";

        int[] flat = Demo.flattenVecVecI32(Arrays.asList(new int[]{1, 2}, new int[]{3}, new int[]{}, new int[]{4, 5}));
        assert flat.length == 5 : "flattenVecVecI32 length";
        assert flat[0] == 1 && flat[1] == 2 && flat[2] == 3 && flat[3] == 4 && flat[4] == 5 : "flattenVecVecI32 values";

        assert Demo.flattenVecVecI32(Collections.emptyList()).length == 0 : "flattenVecVecI32 empty";

        System.out.println("  PASS\n");
    }

    private static void testBlittableRecordVecs() {
        System.out.println("Testing blittable record vecs...");

        List<Location> locations = Demo.generateLocations(3);
        assert locations.size() == 3 : "generateLocations size";
        assert Demo.processLocations(locations) == 3 : "processLocations";
        assert Math.abs(Demo.sumRatings(locations) - 9.3) < 0.0001 : "sumRatings";

        List<Trade> trades = Demo.generateTrades(3);
        assert trades.size() == 3 : "generateTrades size";
        assert Demo.sumTradeVolumes(trades) == 3000L : "sumTradeVolumes";
        assert Demo.aggregateLocationTradeStats(locations, trades) == 3002L : "aggregateLocationTradeStats";

        List<Particle> particles = Demo.generateParticles(3);
        assert particles.size() == 3 : "generateParticles size";
        assert Math.abs(Demo.sumParticleMasses(particles) - 3.003) < 0.0001 : "sumParticleMasses";

        List<SensorReading> readings = Demo.generateSensorReadings(3);
        assert readings.size() == 3 : "generateSensorReadings size";
        assert Math.abs(Demo.avgSensorTemperature(readings) - 21.0) < 0.0001 : "avgSensorTemperature";

        System.out.println("  PASS\n");
    }

    private static void testOptions() {
        System.out.println("Testing options...");

        Optional<Integer> optI32 = Demo.echoOptionalI32(Optional.of(7));
        assert optI32.isPresent() && optI32.get() == 7 : "echoOptionalI32 some";
        assert !Demo.echoOptionalI32(Optional.empty()).isPresent() : "echoOptionalI32 none";

        assert Demo.unwrapOrDefaultI32(Optional.of(9), 4) == 9 : "unwrapOrDefaultI32 some";
        assert Demo.unwrapOrDefaultI32(Optional.empty(), 4) == 4 : "unwrapOrDefaultI32 none";

        assert Demo.makeSomeI32(12).orElse(-1) == 12 : "makeSomeI32";
        assert !Demo.makeNoneI32().isPresent() : "makeNoneI32";

        assert Demo.doubleIfSome(Optional.of(8)).orElse(-1) == 16 : "doubleIfSome some";
        assert !Demo.doubleIfSome(Optional.empty()).isPresent() : "doubleIfSome none";

        Optional<String> optString = Demo.echoOptionalString(Optional.of("hello"));
        assert optString.isPresent() && optString.get().equals("hello") : "echoOptionalString some";
        assert !Demo.echoOptionalString(Optional.empty()).isPresent() : "echoOptionalString none";
        assert Demo.isSomeString(Optional.of("x")) : "isSomeString some";
        assert !Demo.isSomeString(Optional.empty()) : "isSomeString none";

        Optional<Point> optPoint = Demo.echoOptionalPoint(Optional.of(new Point(1.0, 2.0)));
        assert optPoint.isPresent() : "echoOptionalPoint some";
        assert optPoint.get().x() == 1.0 && optPoint.get().y() == 2.0 : "echoOptionalPoint value";
        assert Demo.makeSomePoint(3.0, 4.0).isPresent() : "makeSomePoint";
        assert !Demo.makeNonePoint().isPresent() : "makeNonePoint";

        Optional<Status> optStatus = Demo.echoOptionalStatus(Optional.of(Status.ACTIVE));
        assert optStatus.isPresent() && optStatus.get() == Status.ACTIVE : "echoOptionalStatus some";
        assert !Demo.echoOptionalStatus(Optional.empty()).isPresent() : "echoOptionalStatus none";

        Optional<int[]> optVec = Demo.echoOptionalVec(Optional.of(new int[]{1, 2, 3}));
        assert optVec.isPresent() : "echoOptionalVec some";
        assert optVec.get().length == 3 && optVec.get()[0] == 1 && optVec.get()[2] == 3 : "echoOptionalVec value";
        assert !Demo.echoOptionalVec(Optional.empty()).isPresent() : "echoOptionalVec none";

        Optional<Integer> optVecLen = Demo.optionalVecLength(Optional.of(new int[]{9, 8}));
        assert optVecLen.isPresent() && optVecLen.get() == 2 : "optionalVecLength some";
        assert !Demo.optionalVecLength(Optional.empty()).isPresent() : "optionalVecLength none";

        UserProfile withEmail = Demo.makeUserProfile(
            "Alice",
            30,
            Optional.of("alice@example.com"),
            Optional.of(98.5)
        );
        assert withEmail.email().isPresent() : "makeUserProfile email present";
        assert withEmail.score().isPresent() : "makeUserProfile score present";
        assert Demo.userDisplayName(withEmail).equals("Alice <alice@example.com>") : "userDisplayName with email";

        UserProfile noEmail = Demo.makeUserProfile(
            "Bob",
            22,
            Optional.empty(),
            Optional.empty()
        );
        assert !noEmail.email().isPresent() : "makeUserProfile email none";
        assert !noEmail.score().isPresent() : "makeUserProfile score none";
        assert Demo.userDisplayName(noEmail).equals("Bob") : "userDisplayName without email";

        UserProfile echoedProfile = Demo.echoUserProfile(withEmail);
        assert echoedProfile.email().isPresent() : "echoUserProfile email";
        assert echoedProfile.email().get().equals("alice@example.com") : "echoUserProfile email value";

        SearchResult withCursor = Demo.echoSearchResult(
            new SearchResult("rust ffi", 12, Optional.of("cursor-1"), Optional.of(0.99))
        );
        assert withCursor.nextCursor().isPresent() : "echoSearchResult cursor present";
        assert withCursor.maxScore().isPresent() : "echoSearchResult score present";
        assert Demo.hasMoreResults(withCursor) : "hasMoreResults true";

        SearchResult withoutCursor = Demo.echoSearchResult(
            new SearchResult("rust ffi", 12, Optional.empty(), Optional.empty())
        );
        assert !withoutCursor.nextCursor().isPresent() : "echoSearchResult cursor none";
        assert !withoutCursor.maxScore().isPresent() : "echoSearchResult score none";
        assert !Demo.hasMoreResults(withoutCursor) : "hasMoreResults false";

        System.out.println("  PASS\n");
    }

    private static void testRecordsWithVecs() {
        System.out.println("Testing records with vecs...");

        Polygon polygon = Demo.makePolygon(Arrays.asList(
            new Point(0.0, 0.0), new Point(1.0, 0.0), new Point(0.0, 1.0)
        ));
        assert Demo.polygonVertexCount(polygon) == 3 : "polygonVertexCount";

        Polygon echoed = Demo.echoPolygon(polygon);
        assert echoed.points().size() == 3 : "echoPolygon size";
        assert echoed.points().get(0).x() == 0.0 : "echoPolygon[0].x";

        Point centroid = Demo.polygonCentroid(polygon);
        assert Math.abs(centroid.x() - 1.0 / 3.0) < 0.0001 : "polygonCentroid.x";
        assert Math.abs(centroid.y() - 1.0 / 3.0) < 0.0001 : "polygonCentroid.y";

        Team team = Demo.makeTeam("devs", Arrays.asList("Alice", "Bob"));
        assert team.name().equals("devs") : "makeTeam.name";
        assert team.members().size() == 2 : "makeTeam.members.size";

        Team echoedTeam = Demo.echoTeam(team);
        assert echoedTeam.members().get(0).equals("Alice") : "echoTeam.members[0]";
        assert Demo.teamSize(team) == 2 : "teamSize";

        Classroom classroom = Demo.makeClassroom(Arrays.asList(
            new Person("Mia", 10),
            new Person("Leo", 11)
        ));
        assert classroom.students().size() == 2 : "makeClassroom.students.size";
        assert classroom.students().get(0).name().equals("Mia") : "makeClassroom.students[0].name";

        Classroom echoedClassroom = Demo.echoClassroom(classroom);
        assert echoedClassroom.students().size() == 2 : "echoClassroom.students.size";
        assert echoedClassroom.students().get(1).name().equals("Leo") : "echoClassroom.students[1].name";

        TaggedScores ts = Demo.echoTaggedScores(new TaggedScores("math", new double[]{90.0, 85.5}));
        assert ts.label().equals("math") : "echoTaggedScores.label";
        assert ts.scores().length == 2 : "echoTaggedScores.scores.length";
        assert Math.abs(Demo.averageScore(new TaggedScores("x", new double[]{80.0, 100.0})) - 90.0) < 0.0001 : "averageScore";

        System.out.println("  PASS\n");
    }

    private static void testConstructorCoverageMatrix() {
        System.out.println("Testing constructor coverage matrix...");

        try (ConstructorCoverageMatrix base = new ConstructorCoverageMatrix()) {
            assert base.constructorVariant().equals("new") : "ConstructorCoverageMatrix() variant";
            assert base.summary().equals("default") : "ConstructorCoverageMatrix() summary";
            assert base.payloadChecksum() == 0 : "ConstructorCoverageMatrix() checksum";
            assert base.vectorCount() == 0 : "ConstructorCoverageMatrix() vectorCount";
        }

        try (ConstructorCoverageMatrix scalarMix = new ConstructorCoverageMatrix(7, true, Priority.HIGH)) {
            assert scalarMix.constructorVariant().equals("with_scalar_mix") : "with_scalar_mix variant";
            assert scalarMix.summary().equals("version=7;enabled=true;priority=high") : "with_scalar_mix summary";
            assert scalarMix.payloadChecksum() == 0 : "with_scalar_mix checksum";
            assert scalarMix.vectorCount() == 0 : "with_scalar_mix vectorCount";
        }

        try (ConstructorCoverageMatrix stringAndBytes = new ConstructorCoverageMatrix("bolt", new byte[]{1, 2, 3, 4})) {
            assert stringAndBytes.constructorVariant().equals("with_string_and_bytes") : "with_string_and_bytes variant";
            assert stringAndBytes.summary().equals("label=bolt;bytes=4") : "with_string_and_bytes summary";
            assert stringAndBytes.payloadChecksum() == 10 : "with_string_and_bytes checksum";
            assert stringAndBytes.vectorCount() == 4 : "with_string_and_bytes vectorCount";
        }

        try (ConstructorCoverageMatrix blittableAndRecord = new ConstructorCoverageMatrix(
            new Point(1.5, 2.5),
            new Person("Alice", 31)
        )) {
            assert blittableAndRecord.constructorVariant().equals("with_blittable_and_record") : "with_blittable_and_record variant";
            assert blittableAndRecord.summary().equals("origin=1.5:2.5;person=Alice#31") : "with_blittable_and_record summary";
            assert blittableAndRecord.payloadChecksum() == 0 : "with_blittable_and_record checksum";
            assert blittableAndRecord.vectorCount() == 1 : "with_blittable_and_record vectorCount";
        }

        try (ConstructorCoverageMatrix optionalProfileAndCursor = new ConstructorCoverageMatrix(
            Optional.of(new UserProfile("John", 29, Optional.of("john@example.com"), Optional.of(9.5))),
            Optional.of("cursor-7")
        )) {
            assert optionalProfileAndCursor.constructorVariant().equals("with_optional_profile_and_cursor") : "with_optional_profile_and_cursor variant";
            assert optionalProfileAndCursor.summary().equals("profile=John#29#john@example.com#9.5;cursor=cursor-7") : "with_optional_profile_and_cursor summary";
            assert optionalProfileAndCursor.payloadChecksum() == 0 : "with_optional_profile_and_cursor checksum";
            assert optionalProfileAndCursor.vectorCount() == 2 : "with_optional_profile_and_cursor vectorCount";
        }

        try (ConstructorCoverageMatrix vectorsAndPolygon = new ConstructorCoverageMatrix(
            Arrays.asList("ffi", "swift"),
            Arrays.asList(new Point(0.0, 0.0), new Point(1.0, 1.0)),
            new Polygon(Arrays.asList(new Point(0.0, 0.0), new Point(2.0, 0.0), new Point(1.0, 1.0)))
        )) {
            assert vectorsAndPolygon.constructorVariant().equals("with_vectors_and_polygon") : "with_vectors_and_polygon variant";
            assert vectorsAndPolygon.summary().equals("tags=ffi|swift;anchors=2;polygon=3") : "with_vectors_and_polygon summary";
            assert vectorsAndPolygon.payloadChecksum() == 0 : "with_vectors_and_polygon checksum";
            assert vectorsAndPolygon.vectorCount() == 7 : "with_vectors_and_polygon vectorCount";
        }

        try (ConstructorCoverageMatrix collectionRecords = new ConstructorCoverageMatrix(
            new Team("Platform", Arrays.asList("Alice", "John")),
            new Classroom(Arrays.asList(new Person("Alice", 20), new Person("John", 21))),
            new Polygon(Arrays.asList(new Point(0.0, 0.0), new Point(1.0, 0.0), new Point(1.0, 1.0)))
        )) {
            assert collectionRecords.constructorVariant().equals("with_collection_records") : "with_collection_records variant";
            assert collectionRecords.summary().equals("team=Platform;members=2;students=2;polygon=3") : "with_collection_records summary";
            assert collectionRecords.payloadChecksum() == 0 : "with_collection_records checksum";
            assert collectionRecords.vectorCount() == 7 : "with_collection_records vectorCount";
        }

        try (ConstructorCoverageMatrix enumMix = new ConstructorCoverageMatrix(
            new Filter.ByGroups(Arrays.asList(
                Arrays.asList("café", "🌍"),
                Collections.emptyList(),
                Arrays.asList("common")
            )),
            new Message.Image("https://example.com/image.png", 640, 480),
            new Task("ship", Priority.CRITICAL, false)
        )) {
            assert enumMix.constructorVariant().equals("with_enum_mix") : "with_enum_mix variant";
            assert enumMix.summary().equals(
                "filter=groups:3;message=image:https://example.com/image.png#640x480;task=ship#critical"
            ) : "with_enum_mix summary";
            assert enumMix.payloadChecksum() == 0 : "with_enum_mix checksum";
            assert enumMix.vectorCount() == 1 : "with_enum_mix vectorCount";
        }

        try (ConstructorCoverageMatrix everything = new ConstructorCoverageMatrix(
            new Person("Alice", 31),
            new Address("Main", "AMS", "1000"),
            new UserProfile("John", 29, Optional.of("john@example.com"), Optional.of(9.5)),
            new SearchResult("route", 5, Optional.of("next-9"), Optional.of(7.5)),
            new byte[]{4, 5, 6},
            new Filter.ByRange(1.0, 3.0),
            Arrays.asList("alpha", "beta")
        )) {
            assert everything.constructorVariant().equals("with_everything") : "with_everything variant";
            assert everything.summary().equals(
                "person=Alice#31;city=AMS;profile=profile=John#29#john@example.com#9.5;query=route;filter=range:1.0-3.0;tags=alpha|beta"
            ) : "with_everything summary";
            assert everything.payloadChecksum() == 15 : "with_everything checksum";
            assert everything.vectorCount() == 10 : "with_everything vectorCount";
            assert everything.summarizeBorrowedInputs(
                new UserProfile("John", 29, Optional.of("john@example.com"), Optional.of(9.5)),
                new SearchResult("route", 5, Optional.of("next-9"), Optional.of(7.5)),
                new Filter.ByRange(1.0, 3.0)
            ).equals(
                "profile=John#29#john@example.com#9.5;query=route;filter=range:1.0-3.0"
            ) : "summarizeBorrowedInputs";
        }

        try (ConstructorCoverageMatrix fallible = new ConstructorCoverageMatrix(
            new byte[]{7, 8},
            new SearchResult("search", 4, Optional.of("cursor-4"), Optional.empty()),
            new Filter.ByName("ali")
        )) {
            assert fallible.constructorVariant().equals("try_with_payload_and_search_result") : "try_with_payload_and_search_result variant";
            assert fallible.summary().equals("query=search;cursor=cursor-4;filter=name:ali") : "try_with_payload_and_search_result summary";
            assert fallible.payloadChecksum() == 15 : "try_with_payload_and_search_result checksum";
            assert fallible.vectorCount() == 6 : "try_with_payload_and_search_result vectorCount";
        }

        try {
            new ConstructorCoverageMatrix(
                new byte[0],
                new SearchResult("search", 4, Optional.empty(), Optional.empty()),
                Filter.None.INSTANCE
            );
            assert false : "try_with_payload_and_search_result should fail for empty payload";
        } catch (RuntimeException expected) {
            assert expected.getMessage().contains("Constructor failed") : "try_with_payload_and_search_result error";
        }

        System.out.println("  PASS\n");
    }

    private static void testClosures() {
        System.out.println("Testing closures...");

        final int[] observedValue = new int[]{0};

        assert Demo.applyClosure(value -> value * 2, 5) == 10 : "applyClosure";
        Demo.applyVoidClosure(value -> observedValue[0] = value, 42);
        assert observedValue[0] == 42 : "applyVoidClosure";
        assert Demo.applyNullaryClosure(() -> 99) == 99 : "applyNullaryClosure";
        assert Demo.applyStringClosure(String::toUpperCase, "hello").equals("HELLO") : "applyStringClosure";
        assert !Demo.applyBoolClosure(value -> !value, true) : "applyBoolClosure";
        assert Math.abs(Demo.applyF64Closure(value -> value * value, 3.0) - 9.0) < 0.0001 : "applyF64Closure";
        assert Demo.applyBinaryClosure((left, right) -> left + right, 3, 4) == 7 : "applyBinaryClosure";
        assert Demo.applyOffsetClosure((value, delta) -> value + delta, -5L, 8L) == 3L : "applyOffsetClosure";
        assert Demo.applyStatusClosure(status -> status == Status.ACTIVE ? Status.PENDING : Status.ACTIVE, Status.ACTIVE) == Status.PENDING : "applyStatusClosure";

        Optional<Point> optionalPoint = Demo.applyOptionalPointClosure(
            point -> point.map(value -> new Point(value.x() + 2.0, value.y() + 3.0)),
            Optional.of(new Point(1.0, 2.0))
        );
        assert optionalPoint.isPresent() : "applyOptionalPointClosure some";
        assert optionalPoint.get().x() == 3.0 : "applyOptionalPointClosure.x";
        assert optionalPoint.get().y() == 5.0 : "applyOptionalPointClosure.y";
        assert !Demo.applyOptionalPointClosure(point -> point, Optional.empty()).isPresent() : "applyOptionalPointClosure none";

        assert Demo.applyResultClosure(value -> {
            if (value < 0) {
                throw new MathError.Exception(MathError.NEGATIVE_INPUT);
            }
            return value * 4;
        }, 6) == 24 : "applyResultClosure ok";
        try {
            Demo.applyResultClosure(value -> {
                throw new MathError.Exception(MathError.NEGATIVE_INPUT);
            }, -1);
            assert false : "applyResultClosure should throw";
        } catch (MathError.Exception e) {
            assert e.getError() == MathError.NEGATIVE_INPUT : "applyResultClosure error type";
        }

        Point transformedPoint = Demo.applyPointClosure(
            point -> new Point(point.x() + 1.0, point.y() + 1.0),
            new Point(1.0, 2.0)
        );
        assert transformedPoint.x() == 2.0 : "applyPointClosure.x";
        assert transformedPoint.y() == 3.0 : "applyPointClosure.y";

        int[] mapped = Demo.mapVecWithClosure(value -> value * 2, new int[]{1, 2, 3});
        assert mapped.length == 3 : "mapVecWithClosure length";
        assert mapped[0] == 2 && mapped[1] == 4 && mapped[2] == 6 : "mapVecWithClosure values";

        int[] filtered = Demo.filterVecWithClosure(value -> value % 2 == 0, new int[]{1, 2, 3, 4});
        assert filtered.length == 2 : "filterVecWithClosure length";
        assert filtered[0] == 2 && filtered[1] == 4 : "filterVecWithClosure values";

        System.out.println("  PASS\n");
    }

    private static void testSyncCallbacks() {
        System.out.println("Testing sync callbacks...");

        ValueCallback doubler = value -> value * 2;
        ValueCallback tripler = value -> value * 3;
        ValueCallback incrementer = Demo.makeIncrementingCallback(5);
        PointTransformer pointTransformer = point -> new Point(point.x() + 10.0, point.y() + 20.0);
        StatusMapper statusMapper = status -> status == Status.PENDING ? Status.ACTIVE : Status.INACTIVE;
        StatusMapper flipper = Demo.makeStatusFlipper();
        MultiMethodCallback multiMethod = new MultiMethodCallback() {
            @Override
            public int methodA(int x) {
                return x + 1;
            }

            @Override
            public int methodB(int x, int y) {
                return x * y;
            }

            @Override
            public int methodC() {
                return 5;
            }
        };
        OptionCallback optionCallback = key -> key > 0 ? Optional.of(key * 10) : Optional.empty();
        ResultCallback resultCallback = value -> {
            if (value < 0) {
                throw new MathError.Exception(MathError.NEGATIVE_INPUT);
            }
            return value * 10;
        };
        FalliblePointTransformer falliblePointTransformer = (point, status) -> {
            if (status == Status.INACTIVE) {
                throw new MathError.Exception(MathError.NEGATIVE_INPUT);
            }
            return new Point(point.x() + 100.0, point.y() + 200.0);
        };
        OffsetCallback offsetCallback = (value, delta) -> value + delta;
        VecProcessor vecProcessor = values -> Arrays.stream(values).map(value -> value * value).toArray();
        MessageFormatter messageFormatter = (scope, message) -> scope + "::" + message.toUpperCase();
        OptionalMessageCallback optionalMessageCallback = key -> key > 0 ? Optional.of("message:" + key) : Optional.empty();
        ResultMessageCallback resultMessageCallback = key -> {
            if (key < 0) {
                throw new MathError.Exception(MathError.NEGATIVE_INPUT);
            }
            return "message:" + key;
        };

        assert Demo.invokeValueCallback(doubler, 4) == 8 : "invokeValueCallback";
        assert Demo.invokeValueCallbackTwice(doubler, 3, 4) == 14 : "invokeValueCallbackTwice";
        assert Demo.invokeBoxedValueCallback(doubler, 5) == 10 : "invokeBoxedValueCallback";
        assert incrementer.onValue(4) == 9 : "makeIncrementingCallback direct";
        assert Demo.invokeValueCallback(incrementer, 4) == 9 : "makeIncrementingCallback bridged";
        assert Demo.invokeOptionalValueCallback(Optional.of(doubler), 4) == 8 : "invokeOptionalValueCallback some";
        assert Demo.invokeOptionalValueCallback(Optional.empty(), 4) == 4 : "invokeOptionalValueCallback none";
        assert Demo.mapStatus(statusMapper, Status.PENDING) == Status.ACTIVE : "mapStatus";
        assert flipper.mapStatus(Status.ACTIVE) == Status.INACTIVE : "makeStatusFlipper direct";
        assert Demo.mapStatus(flipper, Status.INACTIVE) == Status.PENDING : "makeStatusFlipper bridged";
        assert Demo.formatMessageWithCallback(messageFormatter, "sync", "borrowed strings").equals("sync::BORROWED STRINGS")
            : "formatMessageWithCallback";
        assert Demo.formatMessageWithBoxedCallback(messageFormatter, "boxed", "borrowed strings").equals("boxed::BORROWED STRINGS")
            : "formatMessageWithBoxedCallback";
        assert Demo.formatMessageWithOptionalCallback(Optional.of(messageFormatter), "optional", "borrowed strings")
            .equals("optional::BORROWED STRINGS") : "formatMessageWithOptionalCallback some";
        assert Demo.formatMessageWithOptionalCallback(Optional.empty(), "fallback", "message").equals("fallback::message")
            : "formatMessageWithOptionalCallback none";
        MessageFormatter prefixer = Demo.makeMessagePrefixer("prefix");
        assert prefixer.formatMessage("scope", "message").equals("prefix::scope::message") : "makeMessagePrefixer direct";
        assert Demo.formatMessageWithCallback(prefixer, "sync", "formatter").equals("prefix::sync::formatter")
            : "makeMessagePrefixer bridged";
        Optional<String> optionalMessage = Demo.invokeOptionalMessageCallback(optionalMessageCallback, 7);
        assert optionalMessage.isPresent() && optionalMessage.get().equals("message:7") : "invokeOptionalMessageCallback some";
        assert !Demo.invokeOptionalMessageCallback(optionalMessageCallback, 0).isPresent() : "invokeOptionalMessageCallback none";
        assert Demo.invokeResultMessageCallback(resultMessageCallback, 8).equals("message:8") : "invokeResultMessageCallback ok";
        try {
            Demo.invokeResultMessageCallback(resultMessageCallback, -1);
            assert false : "invokeResultMessageCallback should throw";
        } catch (MathError.Exception e) {
            assert e.getError() == MathError.NEGATIVE_INPUT : "invokeResultMessageCallback error type";
        }

        int[] processed = Demo.processVec(vecProcessor, new int[]{1, 2, 3});
        assert processed.length == 3 : "processVec length";
        assert processed[0] == 1 && processed[1] == 4 && processed[2] == 9 : "processVec values";

        assert Demo.invokeMultiMethod(multiMethod, 3, 4) == 21 : "invokeMultiMethod";
        assert Demo.invokeMultiMethodBoxed(multiMethod, 3, 4) == 21 : "invokeMultiMethodBoxed";
        assert Demo.invokeTwoCallbacks(doubler, tripler, 5) == 25 : "invokeTwoCallbacks";

        Optional<Integer> optionResult = Demo.invokeOptionCallback(optionCallback, 7);
        assert optionResult.isPresent() && optionResult.get() == 70 : "invokeOptionCallback some";
        assert !Demo.invokeOptionCallback(optionCallback, 0).isPresent() : "invokeOptionCallback none";
        assert Demo.invokeResultCallback(resultCallback, 7) == 70 : "invokeResultCallback ok";
        try {
            Demo.invokeResultCallback(resultCallback, -1);
            assert false : "invokeResultCallback should throw";
        } catch (MathError.Exception e) {
            assert e.getError() == MathError.NEGATIVE_INPUT : "invokeResultCallback error type";
        }
        assert Demo.invokeOffsetCallback(offsetCallback, -5L, 8L) == 3L : "invokeOffsetCallback";
        assert Demo.invokeBoxedOffsetCallback(offsetCallback, 10L, 4L) == 14L : "invokeBoxedOffsetCallback";
        Point richPoint = Demo.invokeFalliblePointTransformer(
            falliblePointTransformer,
            new Point(2.0, 3.0),
            Status.ACTIVE
        );
        assert richPoint.x() == 102.0 : "invokeFalliblePointTransformer.x";
        assert richPoint.y() == 203.0 : "invokeFalliblePointTransformer.y";
        try {
            Demo.invokeFalliblePointTransformer(falliblePointTransformer, new Point(2.0, 3.0), Status.INACTIVE);
            assert false : "invokeFalliblePointTransformer should throw";
        } catch (MathError.Exception e) {
            assert e.getError() == MathError.NEGATIVE_INPUT : "invokeFalliblePointTransformer error type";
        }

        Point transformed = Demo.transformPoint(pointTransformer, new Point(1.0, 2.0));
        assert transformed.x() == 11.0 : "transformPoint.x";
        assert transformed.y() == 22.0 : "transformPoint.y";

        Point transformedBoxed = Demo.transformPointBoxed(pointTransformer, new Point(3.0, 4.0));
        assert transformedBoxed.x() == 13.0 : "transformPointBoxed.x";
        assert transformedBoxed.y() == 24.0 : "transformPointBoxed.y";

        System.out.println("  PASS\n");
    }

    private static void testAsyncCallbacks() {
        System.out.println("Testing async callbacks...");
        try {
            AsyncFetcher asyncFetcher = new AsyncFetcher() {
                @Override
                public CompletableFuture<Integer> fetchValue(int key) {
                    return CompletableFuture.completedFuture(key * 100);
                }

                @Override
                public CompletableFuture<String> fetchString(String input) {
                    return CompletableFuture.completedFuture(input.toUpperCase());
                }

                @Override
                public CompletableFuture<String> fetchJoinedMessage(String scope, String message) {
                    return CompletableFuture.completedFuture(scope + "::" + message.toUpperCase());
                }
            };
            AsyncPointTransformer asyncPointTransformer =
                point -> CompletableFuture.completedFuture(new Point(point.x() + 50.0, point.y() + 60.0));
            AsyncOptionFetcher asyncOptionFetcher = key -> CompletableFuture.completedFuture(
                key > 0 ? Optional.of(key * 1000L) : Optional.empty()
            );
            AsyncOptionalMessageFetcher asyncOptionalMessageFetcher = key -> CompletableFuture.completedFuture(
                key > 0 ? Optional.of("async-message:" + key) : Optional.empty()
            );
            AsyncResultFormatter asyncResultFormatter = new AsyncResultFormatter() {
                @Override
                public CompletableFuture<String> renderMessage(String scope, String message) {
                    if (scope.isEmpty()) {
                        CompletableFuture<String> failed = new CompletableFuture<>();
                        failed.completeExceptionally(new MathError.Exception(MathError.NEGATIVE_INPUT));
                        return failed;
                    }
                    return CompletableFuture.completedFuture(scope + "::" + message.toUpperCase());
                }

                @Override
                public CompletableFuture<Point> transformPoint(Point point, Status status) {
                    if (status == Status.INACTIVE) {
                        CompletableFuture<Point> failed = new CompletableFuture<>();
                        failed.completeExceptionally(new MathError.Exception(MathError.NEGATIVE_INPUT));
                        return failed;
                    }
                    return CompletableFuture.completedFuture(new Point(point.x() + 500.0, point.y() + 600.0));
                }
            };

            assert Demo.fetchWithAsyncCallback(asyncFetcher, 5).get() == 500 : "fetchWithAsyncCallback";
            assert Demo.fetchStringWithAsyncCallback(asyncFetcher, "boltffi").get().equals("BOLTFFI") : "fetchStringWithAsyncCallback";
            assert Demo.fetchJoinedMessageWithAsyncCallback(asyncFetcher, "async", "borrowed strings").get()
                .equals("async::BORROWED STRINGS") : "fetchJoinedMessageWithAsyncCallback";
            Point asyncPoint = Demo.transformPointWithAsyncCallback(asyncPointTransformer, new Point(1.0, 2.0)).get();
            assert asyncPoint.x() == 51.0 : "transformPointWithAsyncCallback.x";
            assert asyncPoint.y() == 62.0 : "transformPointWithAsyncCallback.y";

            Optional<Long> some = Demo.invokeAsyncOptionFetcher(asyncOptionFetcher, 7).get();
            assert some.isPresent() && some.get() == 7000L : "invokeAsyncOptionFetcher some";

            Optional<Long> none = Demo.invokeAsyncOptionFetcher(asyncOptionFetcher, 0).get();
            assert !none.isPresent() : "invokeAsyncOptionFetcher none";
            Optional<String> someMessage = Demo.invokeAsyncOptionalMessageFetcher(asyncOptionalMessageFetcher, 9).get();
            assert someMessage.isPresent() && someMessage.get().equals("async-message:9")
                : "invokeAsyncOptionalMessageFetcher some";
            assert !Demo.invokeAsyncOptionalMessageFetcher(asyncOptionalMessageFetcher, 0).get().isPresent()
                : "invokeAsyncOptionalMessageFetcher none";
            assert Demo.renderMessageWithAsyncResultCallback(asyncResultFormatter, "async", "result").get()
                .equals("async::RESULT") : "renderMessageWithAsyncResultCallback ok";
            Point asyncResultPoint = Demo.transformPointWithAsyncResultCallback(
                asyncResultFormatter,
                new Point(3.0, 4.0),
                Status.ACTIVE
            ).get();
            assert asyncResultPoint.x() == 503.0 : "transformPointWithAsyncResultCallback.x";
            assert asyncResultPoint.y() == 604.0 : "transformPointWithAsyncResultCallback.y";
            try {
                Demo.renderMessageWithAsyncResultCallback(asyncResultFormatter, "", "result").get();
                assert false : "renderMessageWithAsyncResultCallback should throw";
            } catch (Exception e) {
                Throwable cause = e instanceof java.util.concurrent.ExecutionException ? e.getCause() : e;
                assert cause instanceof MathError.Exception : "renderMessageWithAsyncResultCallback error type";
                assert ((MathError.Exception) cause).getError() == MathError.NEGATIVE_INPUT
                    : "renderMessageWithAsyncResultCallback error value";
            }
            try {
                Demo.transformPointWithAsyncResultCallback(asyncResultFormatter, new Point(3.0, 4.0), Status.INACTIVE).get();
                assert false : "transformPointWithAsyncResultCallback should throw";
            } catch (Exception e) {
                Throwable cause = e instanceof java.util.concurrent.ExecutionException ? e.getCause() : e;
                assert cause instanceof MathError.Exception : "transformPointWithAsyncResultCallback error type";
                assert ((MathError.Exception) cause).getError() == MathError.NEGATIVE_INPUT
                    : "transformPointWithAsyncResultCallback error value";
            }
        } catch (Exception exception) {
            throw new RuntimeException("async callback test failed", exception);
        }
        System.out.println("  PASS\n");
    }

    private static void testSingleThreadedStateHolder() {
        System.out.println("Testing single-threaded state holder...");
        try {
            StateHolder holder = new StateHolder("local");
            ValueCallback doubler = value -> value * 2;

            assert holder.getLabel().equals("local") : "StateHolder.getLabel";
            assert holder.getValue() == 0 : "StateHolder.getValue";
            holder.setValue(5);
            assert holder.getValue() == 5 : "StateHolder.setValue";
            assert holder.increment() == 6 : "StateHolder.increment";
            holder.addItem("a");
            holder.addItem("b");
            assert holder.itemCount() == 2 : "StateHolder.itemCount";
            assert holder.getItems().equals(Arrays.asList("a", "b")) : "StateHolder.getItems";
            assert holder
                .removeLast()
                .orElseThrow(() -> new AssertionError("expected removed item"))
                .equals("b") : "StateHolder.removeLast";
            assert holder.transformValue(value -> value / 2) == 3 : "StateHolder.transformValue";
            assert holder.applyValueCallback(doubler) == 6 : "StateHolder.applyValueCallback";
            assert holder.asyncGetValue().get() == 6 : "StateHolder.asyncGetValue";
            holder.asyncSetValue(9).get();
            assert holder.getValue() == 9 : "StateHolder.asyncSetValue";
            assert holder.asyncAddItem("z").get() == 2 : "StateHolder.asyncAddItem";
            assert holder.getItems().equals(Arrays.asList("a", "z")) : "StateHolder.itemsAfterAsyncAdd";
            holder.clear();
            assert holder.getValue() == 0 : "StateHolder.clear value";
            assert holder.getItems().equals(Collections.emptyList()) : "StateHolder.clear items";
            holder.close();
        } catch (Exception e) {
            throw new RuntimeException("single-threaded state holder test failed", e);
        }
        System.out.println("  PASS\n");
    }

    private static void testAsyncFunctions() {
        System.out.println("Testing async functions...");
        try {
            CompletableFuture<Integer> addFuture = Demo.asyncAdd(3, 7);
            assert addFuture.get() == 10 : "asyncAdd(3, 7)";

            CompletableFuture<String> echoFuture = Demo.asyncEcho("hello async");
            assert echoFuture.get().equals("Echo: hello async") : "asyncEcho";

            CompletableFuture<int[]> doubleFuture = Demo.asyncDoubleAll(new int[]{1, 2, 3});
            int[] doubled = doubleFuture.get();
            assert doubled.length == 3 : "asyncDoubleAll length";
            assert doubled[0] == 2 && doubled[1] == 4 && doubled[2] == 6 : "asyncDoubleAll values";

            CompletableFuture<Optional<Integer>> findSome = Demo.asyncFindPositive(new int[]{-1, 0, 5, 3});
            assert findSome.get().isPresent() && findSome.get().get() == 5 : "asyncFindPositive some";

            CompletableFuture<Optional<Integer>> findNone = Demo.asyncFindPositive(new int[]{-1, -2, -3});
            assert !findNone.get().isPresent() : "asyncFindPositive none";

            CompletableFuture<String> concatFuture = Demo.asyncConcat(Arrays.asList("a", "b", "c"));
            assert concatFuture.get().equals("a, b, c") : "asyncConcat";
        } catch (Exception e) {
            throw new RuntimeException("async function test failed", e);
        }
        System.out.println("  PASS\n");
    }

    private static void testAsyncClassMethods() {
        System.out.println("Testing async class methods...");
        try {
            AsyncWorker worker = new AsyncWorker("test");
            assert worker.getPrefix().equals("test") : "AsyncWorker.getPrefix";

            String processed = worker.process("data").get();
            assert processed.equals("test: data") : "AsyncWorker.process";

            Optional<String> found = worker.findItem(42).get();
            assert found.isPresent() : "AsyncWorker.findItem some";
            assert found.get().equals("test_42") : "AsyncWorker.findItem value";

            Optional<String> notFound = worker.findItem(-1).get();
            assert !notFound.isPresent() : "AsyncWorker.findItem none";

            List<String> batch = worker.processBatch(Arrays.asList("x", "y")).get();
            assert batch.size() == 2 : "AsyncWorker.processBatch size";
            assert batch.get(0).equals("test: x") : "AsyncWorker.processBatch[0]";
            assert batch.get(1).equals("test: y") : "AsyncWorker.processBatch[1]";

            worker.close();
        } catch (Exception e) {
            throw new RuntimeException("async class method test failed", e);
        }
        System.out.println("  PASS\n");
    }

    private static void testResultFunctions() {
        System.out.println("Testing result functions...");

        assert Demo.safeDivide(10, 2) == 5 : "safeDivide ok";
        try {
            Demo.safeDivide(10, 0);
            assert false : "safeDivide should throw on zero divisor";
        } catch (RuntimeException e) {
            assert e.getMessage().contains("division by zero") : "safeDivide error message";
        }

        assert Demo.alwaysOk(21) == 42 : "alwaysOk";
        try {
            Demo.alwaysErr("boom");
            assert false : "alwaysErr should throw";
        } catch (RuntimeException e) {
            assert e.getMessage().contains("boom") : "alwaysErr error message";
        }

        Point p = Demo.parsePoint("3.0,4.0");
        assert p.x() == 3.0 : "parsePoint x";
        assert p.y() == 4.0 : "parsePoint y";
        try {
            Demo.parsePoint("bad");
            assert false : "parsePoint should throw on bad input";
        } catch (RuntimeException ignored) {}

        assert Demo.resultOfString(1).equals("item_1") : "resultOfString ok";
        try {
            Demo.resultOfString(-1);
            assert false : "resultOfString should throw on negative key";
        } catch (RuntimeException ignored) {}

        Optional<Integer> some = Demo.resultOfOption(5);
        assert some.isPresent() && some.get() == 10 : "resultOfOption present";
        Optional<Integer> none = Demo.resultOfOption(0);
        assert !none.isPresent() : "resultOfOption empty";
        try {
            Demo.resultOfOption(-1);
            assert false : "resultOfOption should throw on negative key";
        } catch (RuntimeException ignored) {}

        int[] vec = Demo.resultOfVec(3);
        assert vec.length == 3 : "resultOfVec length";
        assert vec[0] == 0 && vec[1] == 1 && vec[2] == 2 : "resultOfVec values";
        try {
            Demo.resultOfVec(-1);
            assert false : "resultOfVec should throw on negative count";
        } catch (RuntimeException ignored) {}

        System.out.println("  PASS\n");
    }

    private static void testResultClassMethods() {
        System.out.println("Testing result class methods...");

        try (Counter counter = new Counter(0)) {
            counter.increment();
            counter.increment();
            counter.increment();
            int val = counter.tryGetPositive();
            assert val == 3 : "tryGetPositive ok: " + val;
        }

        try (Counter counter = new Counter(0)) {
            try {
                counter.tryGetPositive();
                assert false : "tryGetPositive should throw when zero";
            } catch (RuntimeException ignored) {}
        }

        System.out.println("  PASS\n");
    }

    private static void testResultEnumErrors() {
        System.out.println("Testing result enum errors...");

        assert Demo.checkedDivide(10, 2) == 5 : "checkedDivide ok";
        try {
            Demo.checkedDivide(10, 0);
            assert false : "checkedDivide should throw on zero divisor";
        } catch (MathError.Exception e) {
            assert e.getError() == MathError.DIVISION_BY_ZERO : "checkedDivide typed error";
        }

        assert Demo.checkedSqrt(9.0) == 3.0 : "checkedSqrt ok";
        try {
            Demo.checkedSqrt(-1.0);
            assert false : "checkedSqrt should throw on negative";
        } catch (MathError.Exception e) {
            assert e.getError() == MathError.NEGATIVE_INPUT : "checkedSqrt typed error";
        }

        assert Demo.checkedAdd(1, 2) == 3 : "checkedAdd ok";
        try {
            Demo.checkedAdd(Integer.MAX_VALUE, 1);
            assert false : "checkedAdd should throw on overflow";
        } catch (MathError.Exception e) {
            assert e.getError() == MathError.OVERFLOW : "checkedAdd typed error";
        }

        assert Demo.validateUsername("alice").equals("alice") : "validateUsername ok";
        try {
            Demo.validateUsername("ab");
            assert false : "validateUsername should throw on short name";
        } catch (ValidationError.Exception e) {
            assert e.getError() == ValidationError.TOO_SHORT : "validateUsername typed error";
        }
        try {
            Demo.validateUsername("a]bcdefghijklmnopqrstu");
            assert false : "validateUsername should throw on long name";
        } catch (ValidationError.Exception e) {
            assert e.getError() == ValidationError.TOO_LONG : "validateUsername typed error";
        }
        try {
            Demo.validateUsername("has space");
            assert false : "validateUsername should throw on spaces";
        } catch (ValidationError.Exception e) {
            assert e.getError() == ValidationError.INVALID_FORMAT : "validateUsername typed error";
        }

        assert Demo.mayFail(true).equals("Success!") : "mayFail ok";
        try {
            Demo.mayFail(false);
            assert false : "mayFail should throw structured AppError";
        } catch (AppError e) {
            assert e.code() == 400 : "mayFail code";
            assert e.message().equals("Invalid input") : "mayFail message field";
            assert e.getMessage().equals("Invalid input") : "mayFail exception message";
        }

        assert Demo.divideApp(10, 2) == 5 : "divideApp ok";
        try {
            Demo.divideApp(10, 0);
            assert false : "divideApp should throw structured AppError";
        } catch (AppError e) {
            assert e.code() == 500 : "divideApp code";
            assert e.message().equals("Division by zero") : "divideApp message field";
            assert e.getMessage().equals("Division by zero") : "divideApp exception message";
        }

        System.out.println("  PASS\n");
    }

    private static void testStreams() {
        System.out.println("Testing streams (async mode)...");
        try {
            java.util.concurrent.CountDownLatch latch = new java.util.concurrent.CountDownLatch(3);
            java.util.concurrent.CopyOnWriteArrayList<Integer> received = new java.util.concurrent.CopyOnWriteArrayList<>();

            EventBus bus = new EventBus();
            StreamSubscription<Integer> subscription = bus.subscribeValues(value -> {
                received.add(value);
                latch.countDown();
            });

            bus.emitValue(10);
            bus.emitValue(20);
            bus.emitValue(30);

            boolean done = latch.await(5, java.util.concurrent.TimeUnit.SECONDS);
            assert done : "async stream should deliver 3 items within 5 seconds";
            assert received.size() >= 3 : "async stream received " + received.size() + " items, expected >= 3";
            assert received.contains(10) : "async stream should contain 10";
            assert received.contains(20) : "async stream should contain 20";
            assert received.contains(30) : "async stream should contain 30";

            subscription.close();
            bus.close();
        } catch (Exception e) {
            throw new RuntimeException("async stream test failed", e);
        }
        System.out.println("  PASS\n");

        System.out.println("Testing streams (batch mode)...");
        try {
            EventBus bus = new EventBus();
            StreamSubscription<Integer> subscription = bus.subscribeValuesBatch();

            bus.emitValue(100);
            bus.emitValue(200);
            bus.emitValue(300);

            Thread.sleep(100);

            java.util.List<Integer> batch = subscription.popBatch(16);
            assert batch.size() >= 3 : "batch stream should have at least 3 items, got " + batch.size();
            assert batch.contains(100) : "batch should contain 100";
            assert batch.contains(200) : "batch should contain 200";
            assert batch.contains(300) : "batch should contain 300";

            subscription.close();
            bus.close();
        } catch (Exception e) {
            throw new RuntimeException("batch stream test failed", e);
        }
        System.out.println("  PASS\n");

        System.out.println("Testing streams (callback mode)...");
        try {
            java.util.concurrent.CountDownLatch latch = new java.util.concurrent.CountDownLatch(3);
            java.util.concurrent.CopyOnWriteArrayList<Integer> received = new java.util.concurrent.CopyOnWriteArrayList<>();

            EventBus bus = new EventBus();
            StreamSubscription<Integer> subscription = bus.subscribeValuesCallback(value -> {
                received.add(value);
                latch.countDown();
            });

            bus.emitValue(1000);
            bus.emitValue(2000);
            bus.emitValue(3000);

            boolean done = latch.await(5, java.util.concurrent.TimeUnit.SECONDS);
            assert done : "callback stream should deliver 3 items within 5 seconds";
            assert received.size() >= 3 : "callback stream received " + received.size() + " items, expected >= 3";
            assert received.contains(1000) : "callback stream should contain 1000";
            assert received.contains(2000) : "callback stream should contain 2000";
            assert received.contains(3000) : "callback stream should contain 3000";

            subscription.close();
            bus.close();
        } catch (Exception e) {
            throw new RuntimeException("callback stream test failed", e);
        }
        System.out.println("  PASS\n");
    }
}
