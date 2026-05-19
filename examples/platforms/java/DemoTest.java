package com.boltffi.demo;

import java.net.URI;
import java.time.Duration;
import java.time.Instant;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Optional;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;

public final class DemoTest {
    private static String currentDemoCase = null;

    public static void main(String[] args) {
        try {
            System.out.println("Testing Java bindings...\n");
            testBool();
            testI8();
            testU8();
            testI16();
            testU16();
            testI32();
            testU32();
            testI64();
            testU64();
            testIsize();
            testUsize();
            testF32();
            testF64();
            testNoop();
            testStrings();
            testCustomTypes();
            testBuiltins();
            testPointRecords();
            testLineRecords();
            testAddressRecords();
            testPersonRecords();
            testUserProfileVecs();
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
            testBorrowedClassRef();
            testResultClassMethods();
            testResultEnumErrors();
            testStreams();
            System.out.println("All tests passed!");
        } catch (Throwable error) {
            throw withDemoCase(error);
        }
    }

    private static void demoCase(String caseId) {
        currentDemoCase = caseId;
    }

    private static AssertionError withDemoCase(Throwable error) {
        String message = String.valueOf(error.getMessage());
        if (currentDemoCase == null || message.contains("case:")) {
            return error instanceof AssertionError
                ? (AssertionError) error
                : new AssertionError(error);
        }
        return new AssertionError(currentDemoCase + ": " + message, error);
    }

    private static void testBool() {
        System.out.println("Testing bool...");
        assert Demo.echoBool(true) : "case:primitives.scalars.bool.should_roundtrip_true";
        assert !Demo.echoBool(false);
        assert !Demo.negateBool(true);
        assert Demo.negateBool(false) : "case:primitives.scalars.bool.should_negate_false_to_true";
        System.out.println("  PASS\n");
    }

    private static void testI32() {
        System.out.println("Testing i32...");
        assert Demo.echoI32(42) == 42 : "echoI32(42)";
        assert Demo.echoI32(-100) == -100 : "case:primitives.scalars.i32.should_roundtrip_negative_value echoI32(-100)";
        assert Demo.addI32(10, 20) == 30 : "case:primitives.scalars.i32.should_add_two_values addI32(10, 20)";
        assert Demo.add(7, 9) == 16 : "case:primitives.scalars.i32.should_add_with_benchmark_alias add(7, 9)";
        System.out.println("  PASS\n");
    }

    private static void testI8() {
        System.out.println("Testing i8...");
        assert Demo.echoI8((byte) 0) == (byte) 0 : "echoI8(0)";
        assert Demo.echoI8((byte) -7) == (byte) -7 : "case:primitives.scalars.i8.should_roundtrip_negative_value echoI8(-7)";
        System.out.println("  PASS\n");
    }

    private static void testU8() {
        System.out.println("Testing u8...");
        assert Demo.echoU8((byte) 0) == (byte) 0 : "echoU8(0)";
        assert Demo.echoU8((byte) 0xFF) == (byte) 0xFF : "case:primitives.scalars.u8.should_roundtrip_max_value echoU8(255)";
        System.out.println("  PASS\n");
    }

    private static void testI16() {
        System.out.println("Testing i16...");
        assert Demo.echoI16((short) 0) == (short) 0 : "echoI16(0)";
        assert Demo.echoI16((short) -1234) == (short) -1234 : "case:primitives.scalars.i16.should_roundtrip_negative_value echoI16(-1234)";
        System.out.println("  PASS\n");
    }

    private static void testU16() {
        System.out.println("Testing u16...");
        assert Demo.echoU16((short) 0) == (short) 0 : "echoU16(0)";
        // Round-trip a u16 value outside the signed short range (55000 -> (short) -10536 in two's complement).
        assert Demo.echoU16((short) 55000) == (short) 55000 : "case:primitives.scalars.u16.should_roundtrip_large_value echoU16(55000)";
        System.out.println("  PASS\n");
    }

    private static void testU32() {
        System.out.println("Testing u32...");
        assert Demo.echoU32(0) == 0 : "echoU32(0)";
        // 4_000_000_000 is outside the signed int range; round-trip its two's-complement bit pattern.
        assert Demo.echoU32((int) 4_000_000_000L) == (int) 4_000_000_000L : "case:primitives.scalars.u32.should_roundtrip_large_value echoU32(4e9)";
        System.out.println("  PASS\n");
    }

    private static void testU64() {
        System.out.println("Testing u64...");
        assert Demo.echoU64(0L) == 0L : "echoU64(0)";
        assert Demo.echoU64(9_999_999_999L) == 9_999_999_999L : "case:primitives.scalars.u64.should_roundtrip_large_value echoU64(1e10)";
        System.out.println("  PASS\n");
    }

    private static void testIsize() {
        System.out.println("Testing isize...");
        assert Demo.echoIsize(0L) == 0L : "echoIsize(0)";
        assert Demo.echoIsize(-123L) == -123L : "case:primitives.scalars.isize.should_roundtrip_negative_value echoIsize(-123)";
        System.out.println("  PASS\n");
    }

    private static void testUsize() {
        System.out.println("Testing usize...");
        assert Demo.echoUsize(0L) == 0L : "echoUsize(0)";
        assert Demo.echoUsize(123L) == 123L : "case:primitives.scalars.usize.should_roundtrip_value echoUsize(123)";
        System.out.println("  PASS\n");
    }

    private static void testNoop() {
        System.out.println("Testing noop...");
        demoCase("case:primitives.scalars.noop.should_cross_without_values");
        Demo.noop();
        System.out.println("  PASS\n");
    }

    private static void testI64() {
        System.out.println("Testing i64...");
        assert Demo.echoI64(9999999999L) == 9999999999L : "echoI64(large)";
        assert Demo.echoI64(-9999999999L) == -9999999999L : "case:primitives.scalars.i64.should_roundtrip_large_negative_value echoI64(negative large)";
        System.out.println("  PASS\n");
    }

    private static void testF32() {
        System.out.println("Testing f32...");
        assert Math.abs(Demo.echoF32(3.14f) - 3.14f) < 0.001f : "case:primitives.scalars.f32.should_roundtrip_value_with_tolerance echoF32(3.14)";
        assert Math.abs(Demo.addF32(1.5f, 2.5f) - 4.0f) < 0.001f : "case:primitives.scalars.f32.should_add_two_values_with_tolerance addF32(1.5, 2.5)";
        System.out.println("  PASS\n");
    }

    private static void testF64() {
        System.out.println("Testing f64...");
        assert Math.abs(Demo.echoF64(3.14159265359) - 3.14159265359) < 0.0000001 : "case:primitives.scalars.f64.should_roundtrip_pi_with_tolerance echoF64(pi)";
        assert Math.abs(Demo.addF64(1.5, 2.5) - 4.0) < 0.0000001 : "case:primitives.scalars.f64.should_add_two_values_with_tolerance addF64(1.5, 2.5)";
        assert Math.abs(Demo.multiply(1.5, 4.0) - 6.0) < 0.0000001 : "case:primitives.scalars.f64.should_multiply_two_values multiply(1.5, 4.0)";
        System.out.println("  PASS\n");
    }

    private static void testStrings() {
        System.out.println("Testing strings...");
        assert Demo.echoString("hello").equals("hello") : "echoString(hello)";
        assert Demo.echoString("").equals("") : "case:primitives.strings.string.should_roundtrip_empty echoString(empty)";
        assert Demo.echoString("café").equals("café") : "echoString(unicode)";
        assert Demo.echoString("日本語").equals("日本語") : "echoString(cjk)";
        assert Demo.echoString("hello 🌍 world").equals("hello 🌍 world") : "case:primitives.strings.string.should_roundtrip_emoji echoString(emoji)";
        assert Demo.concatStrings("foo", "bar").equals("foobar") : "case:primitives.strings.string.should_concatenate_values concatStrings(foo, bar)";
        assert Demo.concatStrings("", "bar").equals("bar") : "concatStrings(empty, bar)";
        assert Demo.concatStrings("foo", "").equals("foo") : "concatStrings(foo, empty)";
        assert Demo.concatStrings("🎉", "🎊").equals("🎉🎊") : "concatStrings(emoji)";
        assert Demo.stringLength("hello") == 5 : "stringLength(hello)";
        assert Demo.stringLength("") == 0 : "stringLength(empty)";
        assert Demo.stringLength("café") == 5 : "case:primitives.strings.string.should_report_utf8_byte_length stringLength(utf8 bytes)";
        assert Demo.stringLength("🌍") == 4 : "stringLength(emoji 4 bytes)";
        assert Demo.stringIsEmpty("") : "case:primitives.strings.string.should_detect_empty stringIsEmpty(empty)";
        assert Demo.repeatString("ab", 3).equals("ababab") : "case:primitives.strings.string.should_repeat_value repeatString(ab, 3)";
        System.out.println("  PASS\n");
    }

    private static void testCustomTypes() {
        System.out.println("Testing custom types...");
        long timestamp = 1_710_000_000_000L;
        demoCase("case:custom_types.datetime.should_roundtrip_millis");
        assert Demo.echoDatetime(timestamp) == timestamp : "echoDatetime";
        demoCase("case:custom_types.datetime.should_convert_to_millis");
        assert Demo.datetimeToMillis(timestamp) == timestamp : "datetimeToMillis";

        demoCase("case:custom_types.datetime.should_format_rfc3339_timestamp");
        assert Demo.formatTimestamp(timestamp).startsWith("2024-03-") : "formatTimestamp";

        Event event = new Event("launch", timestamp);
        demoCase("case:custom_types.event.should_expose_datetime_field");
        assert event.name().equals("launch") : "Event.name";
        assert event.timestamp() == timestamp : "Event.timestamp";

        demoCase("case:custom_types.event.should_roundtrip_datetime_field");
        Event echoed = Demo.echoEvent(event);
        assert echoed.name().equals("launch") : "echoEvent.name";
        assert echoed.timestamp() == timestamp : "echoEvent.timestamp";
        demoCase("case:custom_types.event.should_extract_timestamp_millis");
        assert Demo.eventTimestamp(event) == timestamp : "eventTimestamp";

        String email = "café@example.com";
        demoCase("case:custom_types.email.should_roundtrip_value");
        assert Demo.echoEmail(email).equals(email) : "echoEmail roundtrip";
        demoCase("case:custom_types.email.should_extract_domain");
        assert Demo.emailDomain(email).equals("example.com") : "emailDomain";

        List<String> emails = Arrays.asList("café@example.com", "user@example.org");
        demoCase("case:custom_types.vectors.emails.should_roundtrip_values");
        List<String> echoedEmails = Demo.echoEmails(emails);
        assert echoedEmails.size() == 2 : "echoEmails length";
        assert echoedEmails.get(0).equals("café@example.com") : "echoEmails[0] (utf-8)";
        assert echoedEmails.get(1).equals("user@example.org") : "echoEmails[1]";

        long[] dts = { 1_710_000_000_000L, 1_710_000_001_000L, 1_710_000_002_000L };
        demoCase("case:custom_types.vectors.datetimes.should_roundtrip_millis_values");
        long[] echoedDts = Demo.echoDatetimes(dts);
        assert echoedDts.length == 3 : "echoDatetimes length";
        assert echoedDts[0] == dts[0] && echoedDts[1] == dts[1] && echoedDts[2] == dts[2]
            : "echoDatetimes roundtrip";

        System.out.println("  PASS\n");
    }

    private static void testBuiltins() {
        System.out.println("Testing builtins...");

        Duration duration = Duration.ofSeconds(12L, 345_000_000L);
        demoCase("case:builtins.duration.should_roundtrip_value");
        assert Demo.echoDuration(duration).equals(duration) : "echoDuration";
        demoCase("case:builtins.duration.should_construct_from_parts");
        assert Demo.makeDuration(7L, 89).equals(Duration.ofSeconds(7L, 89L)) : "makeDuration";
        demoCase("case:builtins.duration.should_report_milliseconds");
        assert Demo.durationAsMillis(Duration.ofMillis(1234L)) == 1234L : "durationAsMillis";

        Instant instant = Instant.ofEpochMilli(1_710_000_000_123L);
        demoCase("case:builtins.system_time.should_roundtrip_value");
        assert Demo.echoSystemTime(instant).equals(instant) : "echoSystemTime";
        demoCase("case:builtins.system_time.should_roundtrip_pre_epoch_value");
        Instant preEpochInstant = Instant.ofEpochSecond(-1L, 500_000_000L);
        assert Demo.echoSystemTime(preEpochInstant).equals(preEpochInstant) : "echoSystemTime pre epoch";
        demoCase("case:builtins.system_time.should_convert_to_epoch_milliseconds");
        assert Demo.systemTimeToMillis(instant) == 1_710_000_000_123L : "systemTimeToMillis";
        demoCase("case:builtins.system_time.should_construct_from_epoch_milliseconds");
        assert Demo.millisToSystemTime(1_710_000_000_123L).equals(instant) : "millisToSystemTime";

        UUID uuid = UUID.fromString("550e8400-e29b-41d4-a716-446655440000");
        demoCase("case:builtins.uuid.should_roundtrip_value");
        assert Demo.echoUuid(uuid).equals(uuid) : "echoUuid";
        demoCase("case:builtins.uuid.should_format_canonical_string");
        assert Demo.uuidToString(uuid).equals("550e8400-e29b-41d4-a716-446655440000") : "uuidToString";

        URI uri = URI.create("https://example.com/path?q=boltffi");
        demoCase("case:builtins.url.should_roundtrip_value");
        assert Demo.echoUrl(uri).equals(uri) : "echoUrl";
        demoCase("case:builtins.url.should_format_string");
        assert Demo.urlToString(uri).equals("https://example.com/path?q=boltffi") : "urlToString";

        System.out.println("  PASS\n");
    }

    private static void testPointRecords() {
        System.out.println("Testing records (Point)...");
        demoCase("case:records.blittable.point.should_make_from_coordinates");
        Point point = Demo.makePoint(1.0, 2.0);
        assert point.x() == 1.0 : "makePoint.x";
        assert point.y() == 2.0 : "makePoint.y";
        demoCase("case:records.blittable.point.should_return_origin");
        Point origin = Point.origin();
        assert origin.x() == 0.0 : "Point.origin.x";
        assert origin.y() == 0.0 : "Point.origin.y";
        demoCase("case:records.blittable.point.should_construct_from_polar_coordinates");
        Point fromPolar = Point.fromPolar(2.0, Math.PI / 2.0);
        assert Math.abs(fromPolar.x()) < 0.0001 : "Point.fromPolar.x";
        assert Math.abs(fromPolar.y() - 2.0) < 0.0001 : "Point.fromPolar.y";
        demoCase("case:records.blittable.point.should_normalize_unit_vector");
        Point unit = Point.tryUnit(3.0, 4.0);
        assert Math.abs(unit.x() - 0.6) < 0.0001 : "Point.tryUnit.x";
        assert Math.abs(unit.y() - 0.8) < 0.0001 : "Point.tryUnit.y";
        demoCase("case:records.blittable.point.should_reject_zero_unit_vector");
        try {
            Point.tryUnit(0.0, 0.0);
            assert false : "Point.tryUnit should throw for zero vector";
        } catch (RuntimeException expected) {
            assert expected.getMessage().contains("cannot normalize zero vector") : "Point.tryUnit error";
        }
        demoCase("case:records.blittable.point.should_return_some_for_checked_unit");
        assert Point.checkedUnit(3.0, 4.0).isPresent() : "Point.checkedUnit some";
        demoCase("case:records.blittable.point.should_return_none_for_zero_checked_unit");
        assert !Point.checkedUnit(0.0, 0.0).isPresent() : "Point.checkedUnit none";
        demoCase("case:records.blittable.point.should_compute_distance");
        assert Math.abs(point.distance() - Math.sqrt(5.0)) < 0.0001 : "Point.distance";
        demoCase("case:records.blittable.point.should_scale_coordinates");
        Point scaledPoint = point.scale(2.5);
        assert scaledPoint.x() == 2.5 : "Point.scale.x";
        assert scaledPoint.y() == 5.0 : "Point.scale.y";
        demoCase("case:records.blittable.point.should_add_coordinates");
        Point addedPoint = point.add(new Point(10.0, 20.0));
        assert addedPoint.x() == 11.0 : "Point.add.x";
        assert addedPoint.y() == 22.0 : "Point.add.y";
        demoCase("case:records.blittable.point.should_compute_path_length");
        assert Math.abs(Point.pathLength(Arrays.asList(
            new Point(0.0, 0.0),
            new Point(3.0, 4.0),
            new Point(6.0, 8.0)
        )) - 10.0) < 0.0001 : "Point.pathLength";
        demoCase("case:records.blittable.point.should_report_dimension_count");
        assert Point.dimensions() == 2 : "Point.dimensions";
        demoCase("case:records.blittable.point.should_roundtrip_value");
        Point echoedPoint = Demo.echoPoint(point);
        assert echoedPoint.x() == 1.0 : "echoPoint.x";
        assert echoedPoint.y() == 2.0 : "echoPoint.y";
        demoCase("case:records.blittable.point.should_add_values");
        Point sumPoint = Demo.addPoints(new Point(3.0, 4.0), new Point(5.0, 6.0));
        assert sumPoint.x() == 8.0 : "addPoints.x";
        assert sumPoint.y() == 10.0 : "addPoints.y";

        demoCase("case:records.blittable.point.should_return_some_for_nonzero_coordinates");
        Optional<Point> tryNonzero = Demo.tryMakePoint(1.0, 2.0);
        assert tryNonzero.isPresent() : "tryMakePoint non-zero present";
        assert tryNonzero.get().x() == 1.0 && tryNonzero.get().y() == 2.0 : "tryMakePoint value";
        demoCase("case:records.blittable.point.should_return_none_for_origin_coordinates");
        assert !Demo.tryMakePoint(0.0, 0.0).isPresent() : "tryMakePoint origin none";
        assert Math.abs(MathUtils.distanceBetween(new Point(0.0, 0.0), new Point(3.0, 4.0)) - 5.0) < 0.0001 : "MathUtils.distanceBetween";
        Point midpoint = MathUtils.midpoint(new Point(0.0, 0.0), new Point(2.0, 4.0));
        assert midpoint.x() == 1.0 : "MathUtils.midpoint.x";
        assert midpoint.y() == 2.0 : "MathUtils.midpoint.y";
        System.out.println("  PASS\n");
    }

    private static void testLineRecords() {
        System.out.println("Testing records (Line)...");
        demoCase("case:records.nested.line.should_make_from_coordinates");
        Line line = Demo.makeLine(0.0, 0.0, 3.0, 4.0);
        assert line.start().x() == 0.0 : "makeLine.start.x";
        assert line.end().y() == 4.0 : "makeLine.end.y";
        demoCase("case:records.nested.line.should_roundtrip_nested_points");
        Line echoedLine = Demo.echoLine(line);
        assert echoedLine.start().x() == 0.0 : "echoLine.start.x";
        assert echoedLine.end().x() == 3.0 : "echoLine.end.x";
        demoCase("case:records.nested.line.should_compute_length");
        assert Math.abs(Demo.lineLength(line) - 5.0) < 0.0001 : "lineLength";

        Rect rect = new Rect(new Point(1.0, 2.0), new Dimensions(3.0, 4.0));
        demoCase("case:records.nested.rect.should_compute_area");
        assert Math.abs(Demo.rectArea(rect) - 12.0) < 0.0001 : "rectArea";
        demoCase("case:records.nested.rect.should_roundtrip_nested_records");
        Rect echoedRect = Demo.echoRect(rect);
        assert echoedRect.origin().x() == 1.0 && echoedRect.origin().y() == 2.0 : "echoRect.origin";
        assert echoedRect.dimensions().width() == 3.0 && echoedRect.dimensions().height() == 4.0 : "echoRect.dimensions";

        System.out.println("  PASS\n");
    }

    private static void testUserProfileVecs() {
        System.out.println("Testing records (user profiles)...");
        demoCase("case:records.with_collections.user_profiles.should_generate_profiles");
        List<BenchmarkUserProfile> profiles = Demo.generateUserProfiles(4);
        assert profiles.size() == 4 : "generateUserProfiles size";
        assert profiles.get(0).id == 0 && profiles.get(3).id == 3 : "generateUserProfiles ids";

        demoCase("case:records.with_collections.user_profiles.should_sum_scores");
        double scoreSum = Demo.sumUserScores(profiles);
        double expectedScoreSum = 0.0;
        for (BenchmarkUserProfile p : profiles) {
            expectedScoreSum += p.score;
        }
        assert Math.abs(scoreSum - expectedScoreSum) < 0.0001 : "sumUserScores";

        demoCase("case:records.with_collections.user_profiles.should_count_active_users");
        int active = Demo.countActiveUsers(profiles);
        int expectedActive = 0;
        for (BenchmarkUserProfile p : profiles) {
            if (p.isActive) {
                expectedActive++;
            }
        }
        assert active == expectedActive : "countActiveUsers";

        System.out.println("  PASS\n");
    }

    private static void testAddressRecords() {
        System.out.println("Testing records (Address)...");
        Address address = new Address("123 Main St", "Springfield", "12345");
        demoCase("case:records.with_strings.address.should_roundtrip_value");
        Address echoedAddress = Demo.echoAddress(address);
        assert echoedAddress.equals(address) : "echoAddress";
        demoCase("case:records.with_strings.address.should_format_value");
        assert Demo.formatAddress(address).equals("123 Main St, Springfield, 12345") : "formatAddress";
        System.out.println("  PASS\n");
    }

    private static void testPersonRecords() {
        System.out.println("Testing records (Person)...");
        demoCase("case:records.with_strings.person.should_make_from_fields");
        Person person = Demo.makePerson("Alice", 30);
        assert person.name().equals("Alice") : "makePerson.name";
        assert person.age() == 30 : "makePerson.age";
        demoCase("case:records.with_strings.person.should_roundtrip_value");
        Person echoedPerson = Demo.echoPerson(person);
        assert echoedPerson.name().equals("Alice") : "echoPerson.name";
        assert echoedPerson.age() == 30 : "echoPerson.age";
        demoCase("case:records.with_strings.person.should_format_greeting");
        assert Demo.greetPerson(person).equals("Hello, Alice! You are 30 years old.") : "greetPerson";
        demoCase("case:records.with_strings.person.should_make_from_fields");
        Person emojiPerson = Demo.makePerson("🎉 Party", 25);
        assert emojiPerson.name().equals("🎉 Party") : "makePerson(emoji)";
        demoCase("case:records.with_strings.person.should_roundtrip_value");
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
        demoCase("case:records.default_values.service_config.should_roundtrip_value");
        assert Demo.echoServiceConfig(explicitBackupEndpoint).equals(explicitBackupEndpoint) : "echoServiceConfig";
        demoCase("case:records.default_values.service_config.should_describe_values");
        assert implicitDefaults.describe().equals("worker:3:standard:none:https://default") : "ServiceConfig.describe(defaults)";
        assert customRetries.describe().equals("worker:7:standard:none:https://default") : "ServiceConfig.describe(customRetries)";
        assert explicitRegion.describe().equals("worker:9:eu-west:none:https://default") : "ServiceConfig.describe(explicitRegion)";
        assert explicitEndpoint.describe().equals("worker:9:eu-west:https://edge:https://default") : "ServiceConfig.describe(explicitEndpoint)";
        assert explicitBackupEndpoint.describe().equals("worker:9:eu-west:https://edge:https://backup") : "ServiceConfig.describe(explicitBackupEndpoint)";
        demoCase("case:records.default_values.service_config.should_describe_with_prefix");
        assert explicitBackupEndpoint.describeWithPrefix("cfg").equals("cfg:worker:9:eu-west:https://edge:https://backup") : "ServiceConfig.describeWithPrefix";
        System.out.println("  PASS\n");
    }

    private static void testCStyleEnums() {
        System.out.println("Testing C-style enums...");

        demoCase("case:enums.c_style.status.should_roundtrip_values");
        assert Demo.echoStatus(Status.ACTIVE) == Status.ACTIVE : "echoStatus(Active)";
        assert Demo.echoStatus(Status.INACTIVE) == Status.INACTIVE : "echoStatus(Inactive)";
        assert Demo.echoStatus(Status.PENDING) == Status.PENDING : "echoStatus(Pending)";
        demoCase("case:enums.c_style.status.should_render_labels");
        assert Demo.statusToString(Status.ACTIVE).equals("active") : "statusToString(Active)";
        assert Demo.statusToString(Status.INACTIVE).equals("inactive") : "statusToString(Inactive)";
        demoCase("case:enums.c_style.status.should_identify_active_values");
        assert Demo.isActive(Status.ACTIVE) : "isActive(Active)";
        assert !Demo.isActive(Status.PENDING) : "isActive(Pending)";

        demoCase("case:enums.c_style.direction.should_roundtrip_value");
        assert Demo.echoDirection(Direction.NORTH) == Direction.NORTH : "echoDirection(North)";
        demoCase("case:enums.c_style.direction.should_return_opposite_from_free_function");
        assert Demo.oppositeDirection(Direction.NORTH) == Direction.SOUTH : "oppositeDirection(North)";
        assert Demo.oppositeDirection(Direction.EAST) == Direction.WEST : "oppositeDirection(East)";
        demoCase("case:enums.c_style.direction.should_return_cardinal_value");
        assert Direction.cardinal() == Direction.NORTH : "Direction.cardinal";
        demoCase("case:enums.c_style.direction.should_construct_from_degrees");
        assert Direction.fromDegrees(90.0) == Direction.EAST : "Direction.fromDegrees(90)";
        assert Direction.fromDegrees(225.0) == Direction.WEST : "Direction.fromDegrees(225)";
        demoCase("case:enums.c_style.direction.should_return_opposite_from_method");
        assert Direction.NORTH.opposite() == Direction.SOUTH : "Direction.opposite";
        demoCase("case:enums.c_style.direction.should_identify_horizontal_values");
        assert Direction.WEST.isHorizontal() : "Direction.isHorizontal(West)";
        assert !Direction.NORTH.isHorizontal() : "Direction.isHorizontal(North)";
        demoCase("case:enums.c_style.direction.should_render_compass_label");
        assert Direction.SOUTH.label().equals("S") : "Direction.label";
        demoCase("case:enums.c_style.direction.should_report_variant_count");
        assert Direction.count() == 4 : "Direction.count";

        demoCase("case:enums.c_style.direction.should_construct_from_raw_value");
        assert Direction.fromValue(0) == Direction.NORTH : "Direction.fromValue(0)";
        assert Direction.fromValue(1) == Direction.SOUTH : "Direction.fromValue(1)";
        assert Direction.fromValue(2) == Direction.EAST : "Direction.fromValue(2)";
        assert Direction.fromValue(3) == Direction.WEST : "Direction.fromValue(3)";

        demoCase("case:enums.c_style.direction.should_return_degrees");
        assert Demo.directionToDegrees(Direction.NORTH) == 0 : "directionToDegrees(North)";
        assert Demo.directionToDegrees(Direction.EAST) == 90 : "directionToDegrees(East)";
        assert Demo.directionToDegrees(Direction.SOUTH) == 180 : "directionToDegrees(South)";
        assert Demo.directionToDegrees(Direction.WEST) == 270 : "directionToDegrees(West)";

        demoCase("case:enums.c_style.direction.should_generate_sequence");
        List<Direction> generated = Demo.generateDirections(6);
        assert generated.size() == 6 : "generateDirections size";
        assert generated.get(0) == Direction.NORTH && generated.get(4) == Direction.NORTH : "generateDirections cycle";

        demoCase("case:enums.c_style.direction.should_count_north_values");
        assert Demo.countNorth(generated) == 2 : "countNorth";
        assert Demo.countNorth(Arrays.asList(Direction.EAST, Direction.SOUTH)) == 0 : "countNorth zero";

        demoCase("case:enums.c_style.direction.find_direction.should_return_some_for_known_id");
        Optional<Direction> foundDir = Demo.findDirection(2);
        assert foundDir.isPresent() && foundDir.get() == Direction.SOUTH : "findDirection known";
        demoCase("case:enums.c_style.direction.find_direction.should_return_none_for_unknown_id");
        assert !Demo.findDirection(99).isPresent() : "findDirection unknown";

        demoCase("case:enums.c_style.direction.find_directions.should_return_sequence_for_positive_count");
        Optional<List<Direction>> foundSeq = Demo.findDirections(3);
        assert foundSeq.isPresent() : "findDirections positive present";
        assert foundSeq.get().size() == 3 : "findDirections size";
        assert foundSeq.get().get(0) == Direction.NORTH : "findDirections first";
        demoCase("case:enums.c_style.direction.find_directions.should_return_none_for_non_positive_count");
        assert !Demo.findDirections(0).isPresent() : "findDirections non-positive";

        demoCase("case:enums.repr_int.priority.should_roundtrip_value");
        assert Demo.echoPriority(Priority.HIGH) == Priority.HIGH : "echoPriority(High)";
        demoCase("case:enums.repr_int.priority.should_render_label");
        assert Demo.priorityLabel(Priority.LOW).equals("low") : "priorityLabel(Low)";
        demoCase("case:enums.repr_int.priority.should_identify_high_priority");
        assert Demo.isHighPriority(Priority.CRITICAL) : "isHighPriority(Critical)";
        assert !Demo.isHighPriority(Priority.LOW) : "isHighPriority(Low)";

        demoCase("case:enums.repr_int.log_level.should_roundtrip_value");
        assert Demo.echoLogLevel(LogLevel.INFO) == LogLevel.INFO : "echoLogLevel(Info)";
        demoCase("case:enums.repr_int.log_level.should_compare_against_minimum");
        assert Demo.shouldLog(LogLevel.ERROR, LogLevel.WARN) : "shouldLog(Error >= Warn)";
        assert !Demo.shouldLog(LogLevel.DEBUG, LogLevel.INFO) : "shouldLog(Debug < Info)";

        demoCase("case:enums.repr_int.http_code.should_expose_discriminant_values");
        assert HttpCode.OK.value == (short) 200 : "HttpCode.OK.value == 200";
        assert HttpCode.NOT_FOUND.value == (short) 404 : "HttpCode.NOT_FOUND.value == 404";
        assert HttpCode.SERVER_ERROR.value == (short) 500 : "HttpCode.SERVER_ERROR.value == 500";
        demoCase("case:enums.repr_int.http_code.should_return_not_found");
        assert Demo.httpCodeNotFound() == HttpCode.NOT_FOUND : "httpCodeNotFound() == NOT_FOUND";
        demoCase("case:enums.repr_int.http_code.should_roundtrip_values");
        assert Demo.echoHttpCode(HttpCode.OK) == HttpCode.OK : "echoHttpCode(OK)";
        assert Demo.echoHttpCode(HttpCode.SERVER_ERROR) == HttpCode.SERVER_ERROR : "echoHttpCode(SERVER_ERROR)";

        demoCase("case:enums.repr_int.sign.should_expose_signed_discriminant_values");
        assert Sign.NEGATIVE.value == (byte) -1 : "Sign.NEGATIVE.value == -1";
        assert Sign.ZERO.value == (byte) 0 : "Sign.ZERO.value == 0";
        assert Sign.POSITIVE.value == (byte) 1 : "Sign.POSITIVE.value == 1";
        demoCase("case:enums.repr_int.sign.should_return_negative");
        assert Demo.signNegative() == Sign.NEGATIVE : "signNegative() == NEGATIVE";
        demoCase("case:enums.repr_int.sign.should_roundtrip_signed_values");
        assert Demo.echoSign(Sign.NEGATIVE) == Sign.NEGATIVE : "echoSign(NEGATIVE)";
        assert Demo.echoSign(Sign.POSITIVE) == Sign.POSITIVE : "echoSign(POSITIVE)";

        System.out.println("  PASS\n");
    }

    private static void testDataEnums() {
        System.out.println("Testing data enums...");

        demoCase("case:records.with_enums.holder.should_make_triangle_variant");
        Holder triangleHolder = Demo.makeTriangleHolder();
        assert triangleHolder.shape() instanceof Shape.Triangle : "Holder.shape is Triangle";
        demoCase("case:records.with_enums.holder.should_roundtrip_data_enum_field");
        Holder echoedHolder = Demo.echoHolder(triangleHolder);
        assert echoedHolder.equals(triangleHolder) : "echoHolder round-trip";

        demoCase("case:records.with_enums.task.should_make_incomplete_task");
        Task task = Demo.makeTask("Ship Java coverage", Priority.HIGH);
        assert task.title().equals("Ship Java coverage") : "makeTask.title";
        assert task.priority() == Priority.HIGH : "makeTask.priority";
        assert !task.completed() : "makeTask.completed";
        demoCase("case:records.with_enums.task.should_roundtrip_priority_field");
        Task echoedTask = Demo.echoTask(task);
        assert echoedTask.equals(task) : "echoTask roundtrip";
        demoCase("case:records.with_enums.task.should_detect_urgent_priority");
        assert Demo.isUrgent(new Task("urgent", Priority.CRITICAL, false)) : "isUrgent(Critical)";
        assert Demo.isUrgent(new Task("urgent", Priority.HIGH, false)) : "isUrgent(High)";
        assert !Demo.isUrgent(new Task("calm", Priority.LOW, false)) : "isUrgent(Low)";

        demoCase("case:records.with_enums.notification.should_roundtrip_priority_field");
        Notification notification = new Notification("hello", Priority.MEDIUM, false);
        Notification echoedNotification = Demo.echoNotification(notification);
        assert echoedNotification.equals(notification) : "echoNotification roundtrip";

        demoCase("case:records.with_enums.task_header.should_make_critical_header");
        TaskHeader header = Demo.makeCriticalTaskHeader(42L);
        assert header.id() == 42L : "TaskHeader.id";
        assert header.priority() == Priority.CRITICAL : "TaskHeader.priority";
        assert !header.completed() : "TaskHeader.completed";
        demoCase("case:records.with_enums.task_header.should_roundtrip_repr_enum_field");
        TaskHeader echoedHeader = Demo.echoTaskHeader(header);
        assert echoedHeader.equals(header) : "echoTaskHeader round-trip";

        demoCase("case:enums.data_enum.lifecycle_event.should_make_critical_event");
        LifecycleEvent started = Demo.makeCriticalLifecycleEvent(7L);
        assert started instanceof LifecycleEvent.TaskStarted : "LifecycleEvent.TaskStarted variant";
        LifecycleEvent.TaskStarted startedTs = (LifecycleEvent.TaskStarted) started;
        assert startedTs.priority == Priority.CRITICAL : "LifecycleEvent.TaskStarted.priority";
        assert startedTs.id == 7L : "LifecycleEvent.TaskStarted.id";
        demoCase("case:enums.data_enum.lifecycle_event.should_roundtrip_priority_payload");
        assert Demo.echoLifecycleEvent(started).equals(started) : "echoLifecycleEvent(TaskStarted)";
        demoCase("case:enums.data_enum.lifecycle_event.should_roundtrip_tick_variant");
        assert Demo.echoLifecycleEvent(LifecycleEvent.Tick.INSTANCE) instanceof LifecycleEvent.Tick : "echoLifecycleEvent(Tick)";

        demoCase("case:records.with_enums.log_entry.should_make_error_entry");
        LogEntry logEntry = Demo.makeErrorLogEntry(1234567890L, (short) 42);
        assert logEntry.timestamp() == 1234567890L : "LogEntry.timestamp";
        assert logEntry.level() == LogLevel.ERROR : "LogEntry.level";
        assert logEntry.code() == (short) 42 : "LogEntry.code";
        demoCase("case:records.with_enums.log_entry.should_roundtrip_u8_enum_field");
        assert Demo.echoLogEntry(logEntry).equals(logEntry) : "echoLogEntry round-trip";

        Filter groupFilter = new Filter.ByGroups(
            Arrays.asList(
                Arrays.asList("café", "🌍"),
                Collections.emptyList(),
                Arrays.asList("common")
            )
        );
        demoCase("case:enums.complex_variants.filter.by_groups.should_roundtrip_nested_string_vectors");
        assert Demo.echoFilter(groupFilter).equals(groupFilter) : "echoFilter(ByGroups)";
        demoCase("case:enums.complex_variants.filter.by_groups.should_describe_nested_string_vectors");
        assert Demo.describeFilter(groupFilter).equals("filter by 3 groups") : "describeFilter(ByGroups)";

        demoCase("case:enums.data_enum.shape.should_support_free_function_factories");
        Shape circle = Demo.makeCircle(5.0);
        assert circle instanceof Shape.Circle : "makeCircle returns Circle";
        assert ((Shape.Circle) circle).radius == 5.0 : "makeCircle.radius";
        Shape rect = Demo.makeRectangle(3.0, 4.0);
        assert rect instanceof Shape.Rectangle : "makeRectangle returns Rectangle";

        demoCase("case:enums.data_enum.shape.unit_circle.should_construct_circle");
        Shape unitCircle = Shape.unitCircle();
        assert unitCircle instanceof Shape.Circle : "Shape.unitCircle type";
        assert Math.abs(((Shape.Circle) unitCircle).radius - 1.0) < 0.0001 : "Shape.unitCircle.radius";
        demoCase("case:enums.data_enum.shape.square.should_construct_rectangle");
        Shape square = Shape.square(3.0);
        assert square instanceof Shape.Rectangle : "Shape.square type";
        assert Math.abs(((Shape.Rectangle) square).width - 3.0) < 0.0001 : "Shape.square.width";
        assert Math.abs(((Shape.Rectangle) square).height - 3.0) < 0.0001 : "Shape.square.height";
        demoCase("case:enums.data_enum.shape.try_circle.should_return_circle_for_positive_radius");
        Shape checkedCircle = Shape.tryCircle(2.0);
        assert checkedCircle instanceof Shape.Circle : "Shape.tryCircle type";

        demoCase("case:enums.data_enum.shape.should_reject_non_positive_circle_radius");
        try {
            Shape.tryCircle(0.0);
            assert false : "Shape.tryCircle should throw on non-positive radius";
        } catch (RuntimeException expected) {
            assert expected.getMessage().contains("radius must be positive") : "Shape.tryCircle error";
        }

        demoCase("case:enums.data_enum.shape.should_support_numeric_instance_methods");
        assert Math.abs(circle.area() - (Math.PI * 25.0)) < 0.0001 : "Shape.area circle";
        assert Math.abs(rect.area() - 12.0) < 0.0001 : "Shape.area rectangle";

        demoCase("case:enums.data_enum.shape.should_support_string_instance_methods");
        assert circle.describe().equals("circle r=5") : "Shape.describe circle";
        assert rect.describe().equals("rect 3x4") : "Shape.describe rectangle";

        demoCase("case:enums.data_enum.shape.should_report_variant_count");
        assert Shape.variantCount() == 6 : "Shape.variantCount";

        demoCase("case:enums.data_enum.shape.should_roundtrip_core_variants");
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

        demoCase("case:enums.data_enum.shape.apex.should_roundtrip_some_point_payload");
        Shape apexSome = Demo.echoShape(new Shape.Apex(Optional.of(new Point(3.0, 4.0))));
        assert apexSome instanceof Shape.Apex : "echoShape(apex Some) type";
        assert ((Shape.Apex) apexSome).tip.isPresent() : "echoShape(apex Some).tip";
        assert ((Shape.Apex) apexSome).tip.get().equals(new Point(3.0, 4.0)) : "echoShape(apex Some).point";
        demoCase("case:enums.data_enum.shape.apex.should_roundtrip_none_payload");
        Shape apexNone = Demo.echoShape(new Shape.Apex(Optional.empty()));
        assert apexNone instanceof Shape.Apex : "echoShape(apex None) type";
        assert !((Shape.Apex) apexNone).tip.isPresent() : "echoShape(apex None).tip";

        demoCase("case:enums.data_enum.shape.should_roundtrip_vector_record_fields");
        Shape cluster = Demo.echoShape(new Shape.Cluster(Arrays.asList(new Point(1.0, 2.0))));
        assert cluster instanceof Shape.Cluster : "echoShape(cluster) type";
        assert ((Shape.Cluster) cluster).members.size() == 1 : "echoShape(cluster).members size";
        assert ((Shape.Cluster) cluster).members.get(0).equals(new Point(1.0, 2.0)) : "echoShape(cluster).member";

        demoCase("case:enums.data_enum.shape.try_apex_point.should_return_some_for_positive_radius");
        Optional<Point> apexPoint = Shape.tryApexPoint(2.5);
        assert apexPoint.isPresent() : "Shape.tryApexPoint positive";
        assert apexPoint.get().equals(new Point(0.0, 2.5)) : "Shape.tryApexPoint point";
        demoCase("case:enums.data_enum.shape.try_apex_point.should_return_none_for_non_positive_radius");
        assert !Shape.tryApexPoint(-1.0).isPresent() : "Shape.tryApexPoint negative";

        demoCase("case:enums.data_enum.message.text.should_roundtrip_string_payload");
        Message text = Demo.echoMessage(new Message.Text("hello"));
        assert text instanceof Message.Text : "echoMessage(Text) type";
        assert ((Message.Text) text).body.equals("hello") : "echoMessage(Text).body";
        demoCase("case:enums.data_enum.message.text.should_render_text_summary");
        assert Demo.messageSummary(new Message.Text("hi")).equals("text: hi") : "messageSummary(Text)";
        demoCase("case:enums.data_enum.message.ping.should_render_ping_summary");
        assert Demo.messageSummary(Message.Ping.INSTANCE).equals("ping") : "messageSummary(Ping)";

        demoCase("case:enums.data_enum.animal.dog.should_roundtrip_string_payloads");
        Animal dog = Demo.echoAnimal(new Animal.Dog("Rex", "Labrador"));
        assert dog instanceof Animal.Dog : "echoAnimal(Dog) type";
        assert ((Animal.Dog) dog).name.equals("Rex") : "echoAnimal(Dog).name";
        demoCase("case:enums.data_enum.animal.dog.should_derive_name");
        assert Demo.animalName(new Animal.Dog("Rex", "Labrador")).equals("Rex") : "animalName(Dog)";

        demoCase("case:enums.data_enum.animal.cat.should_roundtrip_name_and_bool_payload");
        Animal cat = Demo.echoAnimal(new Animal.Cat("Whiskers", true));
        assert cat instanceof Animal.Cat : "echoAnimal(Cat) type";
        assert ((Animal.Cat) cat).name.equals("Whiskers") : "echoAnimal(Cat).name";
        assert ((Animal.Cat) cat).indoor : "echoAnimal(Cat).indoor";
        demoCase("case:enums.data_enum.animal.cat.should_derive_name");
        assert Demo.animalName(new Animal.Cat("Whiskers", true)).equals("Whiskers") : "animalName(Cat)";

        demoCase("case:enums.data_enum.animal.fish.should_derive_count_label");
        assert Demo.animalName(new Animal.Fish(5)).equals("5 fish") : "animalName(Fish)";
        demoCase("case:enums.data_enum.animal.fish.should_roundtrip_count_payload");
        Animal fish = Demo.echoAnimal(new Animal.Fish(7));
        assert fish instanceof Animal.Fish : "echoAnimal(Fish) type";
        assert ((Animal.Fish) fish).count == 7 : "echoAnimal(Fish).count";

        demoCase("case:enums.data_enum.message.image.should_roundtrip_url_dimensions_payload");
        Message image = Demo.echoMessage(new Message.Image("https://example.com/cat.png", 640, 480));
        assert image instanceof Message.Image : "echoMessage(Image) type";
        Message.Image img = (Message.Image) image;
        assert img.url.equals("https://example.com/cat.png") && img.width == 640 && img.height == 480 : "echoMessage(Image) payload";
        demoCase("case:enums.data_enum.message.image.should_render_image_summary");
        assert Demo.messageSummary(new Message.Image("https://example.com/cat.png", 640, 480))
            .equals("image: 640x480 at https://example.com/cat.png") : "messageSummary(Image)";

        demoCase("case:enums.data_enum.message.ping.should_roundtrip_unit_variant");
        Message ping = Demo.echoMessage(Message.Ping.INSTANCE);
        assert ping instanceof Message.Ping : "echoMessage(Ping) type";

        demoCase("case:enums.complex_variants.filter.none.should_roundtrip_unit_variant");
        Filter noneFilter = Demo.echoFilter(Filter.None.INSTANCE);
        assert noneFilter instanceof Filter.None : "echoFilter(None) type";

        demoCase("case:enums.complex_variants.filter.by_name.should_roundtrip_string_payload");
        Filter nameFilter = Demo.echoFilter(new Filter.ByName("alpha"));
        assert nameFilter instanceof Filter.ByName : "echoFilter(ByName) type";
        assert ((Filter.ByName) nameFilter).name.equals("alpha") : "echoFilter(ByName).name";
        demoCase("case:enums.complex_variants.filter.by_name.should_describe_string_payload");
        assert Demo.describeFilter(new Filter.ByName("alpha")).equals("filter by name: alpha") : "describeFilter(ByName)";

        demoCase("case:enums.complex_variants.filter.by_range.should_describe_numeric_bounds");
        assert Demo.describeFilter(new Filter.ByRange(1.0, 10.0)).equals("filter by range: 1..10") : "describeFilter(ByRange)";

        demoCase("case:enums.complex_variants.filter.by_tags.should_roundtrip_string_vector_payload");
        Filter tagsFilter = Demo.echoFilter(new Filter.ByTags(Arrays.asList("a", "b", "c")));
        assert tagsFilter instanceof Filter.ByTags : "echoFilter(ByTags) type";
        assert ((Filter.ByTags) tagsFilter).tags.equals(Arrays.asList("a", "b", "c")) : "echoFilter(ByTags) payload";
        demoCase("case:enums.complex_variants.filter.by_tags.should_describe_string_vector_payload");
        assert Demo.describeFilter(new Filter.ByTags(Arrays.asList("a", "b", "c"))).equals("filter by 3 tags") : "describeFilter(ByTags)";

        demoCase("case:enums.complex_variants.filter.by_points.should_roundtrip_record_vector_payload");
        List<Point> anchors = Arrays.asList(new Point(0.0, 0.0), new Point(1.0, 2.0));
        Filter pointsFilter = Demo.echoFilter(new Filter.ByPoints(anchors));
        assert pointsFilter instanceof Filter.ByPoints : "echoFilter(ByPoints) type";
        assert ((Filter.ByPoints) pointsFilter).anchors.equals(anchors) : "echoFilter(ByPoints) payload";
        demoCase("case:enums.complex_variants.filter.by_points.should_describe_record_vector_payload");
        assert Demo.describeFilter(new Filter.ByPoints(anchors)).equals("filter by 2 anchor points") : "describeFilter(ByPoints)";

        demoCase("case:enums.complex_variants.api_response.redirect.should_roundtrip_url_payload");
        ApiResponse redirect = Demo.echoApiResponse(new ApiResponse.Redirect("https://example.com/new"));
        assert redirect instanceof ApiResponse.Redirect : "echoApiResponse(Redirect) type";
        assert ((ApiResponse.Redirect) redirect).url.equals("https://example.com/new") : "echoApiResponse(Redirect).url";

        demoCase("case:enums.complex_variants.api_response.success.should_roundtrip_string_payload");
        ApiResponse success = Demo.echoApiResponse(new ApiResponse.Success("ok"));
        assert success instanceof ApiResponse.Success : "echoApiResponse(Success) type";
        demoCase("case:enums.complex_variants.api_response.success.should_identify_success");
        assert Demo.isSuccess(new ApiResponse.Success("data")) : "isSuccess(Success)";
        demoCase("case:enums.complex_variants.api_response.empty.should_not_identify_as_success");
        assert !Demo.isSuccess(ApiResponse.Empty.INSTANCE) : "isSuccess(Empty)";

        System.out.println("  PASS\n");
    }

    private static void testCStyleEnumVecs() {
        System.out.println("Testing vec C-style enums...");

        demoCase("case:enums.c_style.status.should_roundtrip_vectors");
        List<Status> statuses = Demo.echoVecStatus(
            Arrays.asList(Status.ACTIVE, Status.PENDING, Status.INACTIVE)
        );
        assert statuses.size() == 3 : "echoVecStatus size";
        assert statuses.get(0) == Status.ACTIVE : "echoVecStatus[0]";
        assert statuses.get(1) == Status.PENDING : "echoVecStatus[1]";
        assert statuses.get(2) == Status.INACTIVE : "echoVecStatus[2]";

        demoCase("case:enums.repr_int.log_level.should_roundtrip_vectors");
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

        demoCase("case:enums.data_enum.shape.should_roundtrip_vectors");
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

        demoCase("case:bytes.bytes.should_roundtrip_values");
        byte[] echoed = Demo.echoBytes(new byte[]{1, 2, 3, 4});
        assert echoed.length == 4 : "echoBytes length";
        assert echoed[0] == 1 && echoed[3] == 4 : "echoBytes values";

        demoCase("case:bytes.bytes.should_report_length");
        assert Demo.bytesLength(new byte[]{10, 20, 30}) == 3 : "bytesLength";
        demoCase("case:bytes.bytes.should_sum_values");
        assert Demo.bytesSum(new byte[]{1, 2, 3, 4}) == 10 : "bytesSum";

        demoCase("case:bytes.bytes.should_make_sequential_values");
        byte[] made = Demo.makeBytes(5);
        assert made.length == 5 : "makeBytes length";
        assert made[0] == 0 && made[4] == 4 : "makeBytes values";

        demoCase("case:bytes.bytes.should_reverse_values");
        byte[] reversed = Demo.reverseBytes(new byte[]{5, 6, 7});
        assert reversed.length == 3 : "reverseBytes length";
        assert reversed[0] == 7 && reversed[2] == 5 : "reverseBytes values";

        System.out.println("  PASS\n");
    }

    private static void testPrimitiveVecs() {
        System.out.println("Testing primitive vecs...");

        demoCase("case:primitives.vecs.i32.should_roundtrip_non_empty");
        int[] ints = Demo.echoVecI32(new int[]{1, 2, 3});
        assert ints.length == 3 : "echoVecI32 length";
        assert ints[0] == 1 && ints[1] == 2 && ints[2] == 3 : "echoVecI32 values";

        demoCase("case:primitives.vecs.i32.should_roundtrip_empty");
        int[] empty = Demo.echoVecI32(new int[0]);
        assert empty.length == 0 : "echoVecI32 empty";

        demoCase("case:primitives.vecs.i32.should_sum_values");
        assert Demo.sumVecI32(new int[]{10, 20, 30}) == 60L : "sumVecI32";
        assert Demo.sumVecI32(new int[0]) == 0L : "sumVecI32 empty";

        demoCase("case:primitives.vecs.f64.should_roundtrip_values");
        double[] doubles = Demo.echoVecF64(new double[]{1.5, 2.5});
        assert doubles.length == 2 : "echoVecF64 length";
        assert Math.abs(doubles[0] - 1.5) < 0.0001 : "echoVecF64[0]";
        assert Math.abs(doubles[1] - 2.5) < 0.0001 : "echoVecF64[1]";

        demoCase("case:primitives.vecs.bool.should_roundtrip_values");
        boolean[] bools = Demo.echoVecBool(new boolean[]{true, false, true});
        assert bools.length == 3 : "echoVecBool length";
        assert bools[0] && !bools[1] && bools[2] : "echoVecBool values";

        demoCase("case:primitives.vecs.i8.should_roundtrip_values");
        byte[] i8s = Demo.echoVecI8(new byte[]{-1, 0, 7});
        assert i8s.length == 3 : "echoVecI8 length";
        assert i8s[0] == -1 && i8s[2] == 7 : "echoVecI8 values";

        demoCase("case:primitives.vecs.u8.should_roundtrip_values");
        byte[] u8s = Demo.echoVecU8(new byte[]{0, 1, 2, 3});
        assert u8s.length == 4 : "echoVecU8 length";
        assert u8s[0] == 0 && u8s[3] == 3 : "echoVecU8 values";

        demoCase("case:primitives.vecs.i16.should_roundtrip_values");
        short[] i16s = Demo.echoVecI16(new short[]{-3, 0, 9});
        assert i16s.length == 3 : "echoVecI16 length";
        assert i16s[0] == -3 && i16s[2] == 9 : "echoVecI16 values";

        demoCase("case:primitives.vecs.u16.should_roundtrip_values");
        short[] u16s = Demo.echoVecU16(new short[]{0, 10, 20});
        assert u16s.length == 3 : "echoVecU16 length";
        assert u16s[0] == 0 && u16s[2] == 20 : "echoVecU16 values";

        demoCase("case:primitives.vecs.u32.should_roundtrip_values");
        int[] u32s = Demo.echoVecU32(new int[]{0, 10, 20});
        assert u32s.length == 3 : "echoVecU32 length";
        assert u32s[0] == 0 && u32s[2] == 20 : "echoVecU32 values";

        demoCase("case:primitives.vecs.i64.should_roundtrip_values");
        long[] i64s = Demo.echoVecI64(new long[]{-5L, 0L, 8L});
        assert i64s.length == 3 : "echoVecI64 length";
        assert i64s[0] == -5L && i64s[2] == 8L : "echoVecI64 values";

        demoCase("case:primitives.vecs.u64.should_roundtrip_values");
        long[] u64s = Demo.echoVecU64(new long[]{0L, 1L, 2L});
        assert u64s.length == 3 : "echoVecU64 length";
        assert u64s[0] == 0L && u64s[2] == 2L : "echoVecU64 values";

        demoCase("case:primitives.vecs.isize.should_roundtrip_values");
        long[] isizes = Demo.echoVecIsize(new long[]{-2L, 0L, 5L});
        assert isizes.length == 3 : "echoVecIsize length";
        assert isizes[0] == -2L && isizes[2] == 5L : "echoVecIsize values";

        demoCase("case:primitives.vecs.usize.should_roundtrip_values");
        long[] usizes = Demo.echoVecUsize(new long[]{0L, 2L, 4L});
        assert usizes.length == 3 : "echoVecUsize length";
        assert usizes[0] == 0L && usizes[2] == 4L : "echoVecUsize values";

        demoCase("case:primitives.vecs.f32.should_roundtrip_values_with_tolerance");
        float[] f32s = Demo.echoVecF32(new float[]{1.25f, -2.5f});
        assert f32s.length == 2 : "echoVecF32 length";
        assert Math.abs(f32s[0] - 1.25f) < 0.0001f : "echoVecF32[0]";
        assert Math.abs(f32s[1] + 2.5f) < 0.0001f : "echoVecF32[1]";

        demoCase("case:primitives.vecs.i32.should_make_range");
        int[] range = Demo.makeRange(0, 5);
        assert range.length == 5 : "makeRange length";
        assert range[0] == 0 && range[4] == 4 : "makeRange values";

        demoCase("case:primitives.vecs.i32.should_reverse_values");
        int[] reversed = Demo.reverseVecI32(new int[]{1, 2, 3});
        assert reversed[0] == 3 && reversed[1] == 2 && reversed[2] == 1 : "reverseVecI32";

        demoCase("case:primitives.vecs.i32.should_generate_sequence");
        int[] genI32 = Demo.generateI32Vec(4);
        assert genI32.length == 4 : "generateI32Vec length";
        assert genI32[0] == 0 && genI32[1] == 1 && genI32[2] == 2 && genI32[3] == 3 : "generateI32Vec values";

        demoCase("case:primitives.vecs.i32.should_sum_benchmark_values");
        assert Demo.sumI32Vec(new int[]{4, 5, 6}) == 15L : "sumI32Vec";

        demoCase("case:primitives.vecs.f64.should_generate_sequence");
        double[] genF64 = Demo.generateF64Vec(3);
        assert genF64.length == 3 : "generateF64Vec length";
        assert Math.abs(genF64[0] - 0.0) < 0.0001
            && Math.abs(genF64[1] - 0.1) < 0.0001
            && Math.abs(genF64[2] - 0.2) < 0.0001 : "generateF64Vec values";

        demoCase("case:primitives.vecs.f64.should_sum_values");
        assert Math.abs(Demo.sumF64Vec(new double[]{1.5, 2.5, 4.0}) - 8.0) < 0.0001 : "sumF64Vec";

        demoCase("case:primitives.vecs.u64.should_increment_first_value_in_place");
        long[] u64Buf = new long[]{10L, 20L, 30L};
        Demo.incU64(u64Buf);
        assert u64Buf[0] == 11L && u64Buf[1] == 20L && u64Buf[2] == 30L : "incU64 first element";

        demoCase("case:primitives.vecs.u64.should_increment_value");
        assert Demo.incU64Value(41L) == 42L : "incU64Value";

        System.out.println("  PASS\n");
    }

    private static void testVecStrings() {
        System.out.println("Testing vec strings...");

        demoCase("case:primitives.vecs.string.should_roundtrip_values");
        List<String> strings = Demo.echoVecString(Arrays.asList("hello", "world"));
        assert strings.size() == 2 : "echoVecString size";
        assert strings.get(0).equals("hello") : "echoVecString[0]";
        assert strings.get(1).equals("world") : "echoVecString[1]";

        List<String> emptyStrings = Demo.echoVecString(Collections.emptyList());
        assert emptyStrings.isEmpty() : "echoVecString empty";

        demoCase("case:primitives.vecs.string.should_report_utf8_byte_lengths");
        int[] lengths = Demo.vecStringLengths(Arrays.asList("hi", "café"));
        assert lengths.length == 2 : "vecStringLengths size";
        assert lengths[0] == 2 : "vecStringLengths[0]";
        assert lengths[1] == 5 : "vecStringLengths[1] (utf8)";

        System.out.println("  PASS\n");
    }

    private static void testNestedVecs() {
        System.out.println("Testing nested vecs...");

        demoCase("case:primitives.vecs.nested_i32.should_roundtrip_values");
        List<int[]> vvi = Demo.echoVecVecI32(Arrays.asList(new int[]{1, 2, 3}, new int[]{}, new int[]{4, 5}));
        assert vvi.size() == 3 : "echoVecVecI32 outer size";
        assert vvi.get(0).length == 3 && vvi.get(0)[0] == 1 && vvi.get(0)[2] == 3 : "echoVecVecI32[0]";
        assert vvi.get(1).length == 0 : "echoVecVecI32[1] empty";
        assert vvi.get(2).length == 2 && vvi.get(2)[0] == 4 && vvi.get(2)[1] == 5 : "echoVecVecI32[2]";

        demoCase("case:primitives.vecs.nested_i32.should_roundtrip_empty_outer");
        List<int[]> vviEmpty = Demo.echoVecVecI32(Collections.emptyList());
        assert vviEmpty.isEmpty() : "echoVecVecI32 empty outer";

        demoCase("case:primitives.vecs.nested_bool.should_roundtrip_values");
        List<boolean[]> vvb = Demo.echoVecVecBool(Arrays.asList(
                new boolean[]{true, false, true},
                new boolean[]{},
                new boolean[]{false}));
        assert vvb.size() == 3 : "echoVecVecBool outer size";
        assert vvb.get(0).length == 3 && vvb.get(0)[0] && !vvb.get(0)[1] && vvb.get(0)[2] : "echoVecVecBool[0]";
        assert vvb.get(1).length == 0 : "echoVecVecBool[1] empty";
        assert vvb.get(2).length == 1 && !vvb.get(2)[0] : "echoVecVecBool[2]";

        demoCase("case:primitives.vecs.nested_isize.should_roundtrip_values");
        List<long[]> vvisize = Demo.echoVecVecIsize(Arrays.asList(
                new long[]{-2L, 0L, 5L},
                new long[]{},
                new long[]{9L}));
        assert vvisize.size() == 3 : "echoVecVecIsize outer size";
        assert vvisize.get(0).length == 3 && vvisize.get(0)[0] == -2L && vvisize.get(0)[2] == 5L : "echoVecVecIsize[0]";
        assert vvisize.get(1).length == 0 : "echoVecVecIsize[1] empty";
        assert vvisize.get(2).length == 1 && vvisize.get(2)[0] == 9L : "echoVecVecIsize[2]";

        demoCase("case:primitives.vecs.nested_usize.should_roundtrip_values");
        List<long[]> vvusize = Demo.echoVecVecUsize(Arrays.asList(
                new long[]{0L, 2L, 4L},
                new long[]{},
                new long[]{8L}));
        assert vvusize.size() == 3 : "echoVecVecUsize outer size";
        assert vvusize.get(0).length == 3 && vvusize.get(0)[0] == 0L && vvusize.get(0)[2] == 4L : "echoVecVecUsize[0]";
        assert vvusize.get(1).length == 0 : "echoVecVecUsize[1] empty";
        assert vvusize.get(2).length == 1 && vvusize.get(2)[0] == 8L : "echoVecVecUsize[2]";

        demoCase("case:primitives.vecs.nested_string.should_roundtrip_utf8_values");
        List<List<String>> vvs = Demo.echoVecVecString(Arrays.asList(
                Arrays.asList("hello", "world"),
                Collections.emptyList(),
                Arrays.asList("café", "🌍")));
        assert vvs.size() == 3 : "echoVecVecString outer size";
        assert vvs.get(0).equals(Arrays.asList("hello", "world")) : "echoVecVecString[0]";
        assert vvs.get(1).isEmpty() : "echoVecVecString[1] empty";
        assert vvs.get(2).equals(Arrays.asList("café", "🌍")) : "echoVecVecString[2]";

        demoCase("case:primitives.vecs.nested_i32.should_flatten_values");
        int[] flat = Demo.flattenVecVecI32(Arrays.asList(new int[]{1, 2}, new int[]{3}, new int[]{}, new int[]{4, 5}));
        assert flat.length == 5 : "flattenVecVecI32 length";
        assert flat[0] == 1 && flat[1] == 2 && flat[2] == 3 && flat[3] == 4 && flat[4] == 5 : "flattenVecVecI32 values";

        demoCase("case:primitives.vecs.nested_i32.should_flatten_empty");
        assert Demo.flattenVecVecI32(Collections.emptyList()).length == 0 : "flattenVecVecI32 empty";

        System.out.println("  PASS\n");
    }

    private static void testBlittableRecordVecs() {
        System.out.println("Testing blittable record vecs...");

        demoCase("case:records.blittable.locations.should_generate_sample_vector");
        List<Location> locations = Demo.generateLocations(3);
        assert locations.size() == 3 : "generateLocations size";
        demoCase("case:records.blittable.locations.should_count_vector_items");
        assert Demo.processLocations(locations) == 3 : "processLocations";
        demoCase("case:records.blittable.locations.should_sum_generated_ratings");
        assert Math.abs(Demo.sumRatings(locations) - 9.3) < 0.0001 : "sumRatings";

        demoCase("case:records.blittable.trades.should_generate_sample_vector");
        List<Trade> trades = Demo.generateTrades(3);
        assert trades.size() == 3 : "generateTrades size";
        demoCase("case:records.blittable.trades.should_sum_volumes");
        assert Demo.sumTradeVolumes(trades) == 3000L : "sumTradeVolumes";
        demoCase("case:records.blittable.trades.should_aggregate_with_locations");
        assert Demo.aggregateLocationTradeStats(locations, trades) == 3002L : "aggregateLocationTradeStats";

        demoCase("case:records.blittable.particles.should_generate_sample_vector");
        List<Particle> particles = Demo.generateParticles(3);
        assert particles.size() == 3 : "generateParticles size";
        demoCase("case:records.blittable.particles.should_sum_masses");
        assert Math.abs(Demo.sumParticleMasses(particles) - 3.003) < 0.0001 : "sumParticleMasses";

        demoCase("case:records.blittable.sensor_readings.should_generate_sample_vector");
        List<SensorReading> readings = Demo.generateSensorReadings(3);
        assert readings.size() == 3 : "generateSensorReadings size";
        demoCase("case:records.blittable.sensor_readings.should_average_generated_temperatures");
        assert Math.abs(Demo.avgSensorTemperature(readings) - 21.0) < 0.0001 : "avgSensorTemperature";
        demoCase("case:records.blittable.sensor_readings.should_average_empty_vector_as_zero");
        assert Demo.avgSensorTemperature(Collections.emptyList()) == 0.0 : "avgSensorTemperature empty";

        demoCase("case:records.blittable.color.should_roundtrip_value");
        Color color = new Color((byte) 0xAA, (byte) 0xBB, (byte) 0xCC, (byte) 0xDD);
        Color echoedColor = Demo.echoColor(color);
        assert echoedColor.r() == color.r() && echoedColor.g() == color.g()
            && echoedColor.b() == color.b() && echoedColor.a() == color.a() : "echoColor";
        demoCase("case:records.blittable.color.should_make_from_channels");
        Color madeColor = Demo.makeColor((byte) 1, (byte) 2, (byte) 3, (byte) 4);
        assert madeColor.r() == 1 && madeColor.g() == 2 && madeColor.b() == 3 && madeColor.a() == 4 : "makeColor channels";

        demoCase("case:records.blittable.locations.should_count_empty_vector");
        assert Demo.processLocations(Collections.emptyList()) == 0 : "processLocations empty";
        List<Location> hostLocations = Arrays.asList(
            new Location(1L, 1.0, 2.0, 3.5, 4, true),
            new Location(2L, 5.0, 6.0, 2.5, 8, false)
        );
        demoCase("case:records.blittable.locations.should_count_host_constructed_vector");
        assert Demo.processLocations(hostLocations) == 2 : "processLocations host-constructed";
        demoCase("case:records.blittable.locations.should_sum_host_constructed_ratings");
        assert Math.abs(Demo.sumRatings(hostLocations) - 6.0) < 0.0001 : "sumRatings host-constructed";

        demoCase("case:records.blittable.locations.find_location.should_return_some_for_positive_id");
        Optional<Location> foundLoc = Demo.findLocation(7);
        assert foundLoc.isPresent() && foundLoc.get().id() == 7L : "findLocation present";
        demoCase("case:records.blittable.locations.find_location.should_return_none_for_non_positive_id");
        assert !Demo.findLocation(0).isPresent() : "findLocation absent";

        demoCase("case:records.blittable.locations.find_locations.should_return_some_vector_for_positive_count");
        Optional<List<Location>> foundLocs = Demo.findLocations(3);
        assert foundLocs.isPresent() : "findLocations present";
        assert foundLocs.get().size() == 3 : "findLocations size";
        demoCase("case:records.blittable.locations.find_locations.should_return_none_for_non_positive_count");
        assert !Demo.findLocations(0).isPresent() : "findLocations non-positive";

        System.out.println("  PASS\n");
    }

    private static void testOptions() {
        System.out.println("Testing options...");

        demoCase("case:options.primitives.i32.should_roundtrip_some");
        Optional<Integer> optI32 = Demo.echoOptionalI32(Optional.of(7));
        assert optI32.isPresent() && optI32.get() == 7 : "echoOptionalI32 some";
        demoCase("case:options.primitives.i32.should_roundtrip_none");
        assert !Demo.echoOptionalI32(Optional.empty()).isPresent() : "echoOptionalI32 none";

        demoCase("case:options.primitives.i32.should_unwrap_some");
        assert Demo.unwrapOrDefaultI32(Optional.of(9), 4) == 9 : "unwrapOrDefaultI32 some";
        demoCase("case:options.primitives.i32.should_use_default_for_none");
        assert Demo.unwrapOrDefaultI32(Optional.empty(), 4) == 4 : "unwrapOrDefaultI32 none";

        demoCase("case:options.primitives.i32.should_make_some");
        assert Demo.makeSomeI32(12).orElse(-1) == 12 : "makeSomeI32";
        demoCase("case:options.primitives.i32.should_make_none");
        assert !Demo.makeNoneI32().isPresent() : "makeNoneI32";

        demoCase("case:options.primitives.i32.should_double_some");
        assert Demo.doubleIfSome(Optional.of(8)).orElse(-1) == 16 : "doubleIfSome some";
        demoCase("case:options.primitives.i32.should_preserve_none_when_doubling");
        assert !Demo.doubleIfSome(Optional.empty()).isPresent() : "doubleIfSome none";

        demoCase("case:options.complex.string.should_roundtrip_some");
        Optional<String> optString = Demo.echoOptionalString(Optional.of("hello"));
        assert optString.isPresent() && optString.get().equals("hello") : "echoOptionalString some";
        demoCase("case:options.complex.string.should_roundtrip_none");
        assert !Demo.echoOptionalString(Optional.empty()).isPresent() : "echoOptionalString none";
        demoCase("case:options.complex.string.should_report_some");
        assert Demo.isSomeString(Optional.of("x")) : "isSomeString some";
        demoCase("case:options.complex.string.should_report_none");
        assert !Demo.isSomeString(Optional.empty()) : "isSomeString none";

        demoCase("case:options.complex.point.should_roundtrip_some");
        Optional<Point> optPoint = Demo.echoOptionalPoint(Optional.of(new Point(1.0, 2.0)));
        assert optPoint.isPresent() : "echoOptionalPoint some";
        assert optPoint.get().x() == 1.0 && optPoint.get().y() == 2.0 : "echoOptionalPoint value";
        demoCase("case:options.complex.point.should_roundtrip_none");
        assert !Demo.echoOptionalPoint(Optional.empty()).isPresent() : "echoOptionalPoint none";
        demoCase("case:options.complex.point.should_make_some");
        assert Demo.makeSomePoint(3.0, 4.0).isPresent() : "makeSomePoint";
        demoCase("case:options.complex.point.should_make_none");
        assert !Demo.makeNonePoint().isPresent() : "makeNonePoint";

        demoCase("case:options.complex.status.should_roundtrip_some");
        Optional<Status> optStatus = Demo.echoOptionalStatus(Optional.of(Status.ACTIVE));
        assert optStatus.isPresent() && optStatus.get() == Status.ACTIVE : "echoOptionalStatus some";
        demoCase("case:options.complex.status.should_roundtrip_none");
        assert !Demo.echoOptionalStatus(Optional.empty()).isPresent() : "echoOptionalStatus none";

        demoCase("case:options.complex.vec.should_roundtrip_some");
        Optional<int[]> optVec = Demo.echoOptionalVec(Optional.of(new int[]{1, 2, 3}));
        assert optVec.isPresent() : "echoOptionalVec some";
        assert optVec.get().length == 3 && optVec.get()[0] == 1 && optVec.get()[2] == 3 : "echoOptionalVec value";
        demoCase("case:options.complex.vec.should_roundtrip_none");
        assert !Demo.echoOptionalVec(Optional.empty()).isPresent() : "echoOptionalVec none";

        demoCase("case:options.complex.vec.should_report_length_for_some");
        Optional<Integer> optVecLen = Demo.optionalVecLength(Optional.of(new int[]{9, 8}));
        assert optVecLen.isPresent() && optVecLen.get() == 2 : "optionalVecLength some";
        demoCase("case:options.complex.vec.should_return_none_for_absent_length");
        assert !Demo.optionalVecLength(Optional.empty()).isPresent() : "optionalVecLength none";

        demoCase("case:records.with_options.user_profile.should_make_with_present_options");
        UserProfile withEmail = Demo.makeUserProfile(
            "Alice",
            30,
            Optional.of("alice@example.com"),
            Optional.of(98.5)
        );
        assert withEmail.email().isPresent() : "makeUserProfile email present";
        assert withEmail.score().isPresent() : "makeUserProfile score present";
        demoCase("case:records.with_options.user_profile.should_display_email_when_present");
        assert Demo.userDisplayName(withEmail).equals("Alice <alice@example.com>") : "userDisplayName with email";

        demoCase("case:records.with_options.user_profile.should_make_with_absent_options");
        UserProfile noEmail = Demo.makeUserProfile(
            "Bob",
            22,
            Optional.empty(),
            Optional.empty()
        );
        assert !noEmail.email().isPresent() : "makeUserProfile email none";
        assert !noEmail.score().isPresent() : "makeUserProfile score none";
        demoCase("case:records.with_options.user_profile.should_display_name_when_email_absent");
        assert Demo.userDisplayName(noEmail).equals("Bob") : "userDisplayName without email";

        demoCase("case:records.with_options.user_profile.should_roundtrip_present_options");
        UserProfile echoedProfile = Demo.echoUserProfile(withEmail);
        assert echoedProfile.email().isPresent() : "echoUserProfile email";
        assert echoedProfile.email().get().equals("alice@example.com") : "echoUserProfile email value";

        demoCase("case:records.with_options.user_profile.should_roundtrip_absent_options");
        UserProfile echoedAbsent = Demo.echoUserProfile(noEmail);
        assert !echoedAbsent.email().isPresent() : "echoUserProfile email absent";
        assert !echoedAbsent.score().isPresent() : "echoUserProfile score absent";
        assert echoedAbsent.name().equals("Bob") && echoedAbsent.age() == 22 : "echoUserProfile absent payload";

        demoCase("case:records.with_options.user_profile.should_roundtrip_mixed_options");
        UserProfile mixedProfile = new UserProfile("Cara", 27, Optional.of("cara@example.com"), Optional.empty());
        UserProfile echoedMixedProfile = Demo.echoUserProfile(mixedProfile);
        assert echoedMixedProfile.email().isPresent() && echoedMixedProfile.email().get().equals("cara@example.com") : "echoUserProfile mixed email";
        assert !echoedMixedProfile.score().isPresent() : "echoUserProfile mixed score";

        demoCase("case:records.with_options.user_profile.should_roundtrip_utf8_optional_string");
        UserProfile utf8 = new UserProfile("Élodie", 34, Optional.of("élodie@café.example"), Optional.of(7.5));
        UserProfile echoedUtf8 = Demo.echoUserProfile(utf8);
        assert echoedUtf8.email().isPresent() && echoedUtf8.email().get().equals("élodie@café.example") : "echoUserProfile utf-8 string";
        assert echoedUtf8.name().equals("Élodie") : "echoUserProfile utf-8 name";

        demoCase("case:records.with_options.search_result.should_roundtrip_present_options");
        SearchResult withCursor = Demo.echoSearchResult(
            new SearchResult("rust ffi", 12, Optional.of("cursor-1"), Optional.of(0.99))
        );
        assert withCursor.nextCursor().isPresent() : "echoSearchResult cursor present";
        assert withCursor.maxScore().isPresent() : "echoSearchResult score present";
        demoCase("case:records.with_options.search_result.should_report_more_results_when_cursor_present");
        assert Demo.hasMoreResults(withCursor) : "hasMoreResults true";

        demoCase("case:records.with_options.search_result.should_roundtrip_absent_options");
        SearchResult withoutCursor = Demo.echoSearchResult(
            new SearchResult("rust ffi", 12, Optional.empty(), Optional.empty())
        );
        assert !withoutCursor.nextCursor().isPresent() : "echoSearchResult cursor none";
        assert !withoutCursor.maxScore().isPresent() : "echoSearchResult score none";
        demoCase("case:records.with_options.search_result.should_report_no_more_results_without_cursor");
        assert !Demo.hasMoreResults(withoutCursor) : "hasMoreResults false";

        demoCase("case:options.complex.vec_optional_i32.should_roundtrip_mixed_presence");
        java.util.List<Optional<Integer>> mixed = java.util.Arrays.asList(
            Optional.of(1), Optional.empty(), Optional.of(3), Optional.empty(), Optional.of(5)
        );
        java.util.List<Optional<Integer>> echoedMixed = Demo.echoVecOptionalI32(mixed);
        assert echoedMixed.size() == mixed.size() : "echoVecOptionalI32 size";
        for (int i = 0; i < mixed.size(); i++) {
            assert echoedMixed.get(i).equals(mixed.get(i))
                : "echoVecOptionalI32[" + i + "] preserves presence and value";
        }
        demoCase("case:options.complex.vec_optional_i32.should_roundtrip_empty");
        assert Demo.echoVecOptionalI32(java.util.Collections.emptyList()).isEmpty()
            : "echoVecOptionalI32 empty";

        demoCase("case:options.complex.vec_optional_i32.should_roundtrip_all_none");
        java.util.List<Optional<Integer>> allNone = Arrays.asList(Optional.empty(), Optional.empty(), Optional.empty());
        java.util.List<Optional<Integer>> echoedAllNone = Demo.echoVecOptionalI32(allNone);
        assert echoedAllNone.size() == 3 : "echoVecOptionalI32 all-none size";
        for (Optional<Integer> v : echoedAllNone) {
            assert !v.isPresent() : "echoVecOptionalI32 all-none entry";
        }

        demoCase("case:options.primitives.bool.should_roundtrip_some");
        Optional<Boolean> optBoolSome = Demo.echoOptionalBool(Optional.of(true));
        assert optBoolSome.isPresent() && optBoolSome.get() : "echoOptionalBool some";
        demoCase("case:options.primitives.bool.should_roundtrip_none");
        assert !Demo.echoOptionalBool(Optional.empty()).isPresent() : "echoOptionalBool none";

        demoCase("case:options.primitives.f64.should_roundtrip_some");
        Optional<Double> optF64Some = Demo.echoOptionalF64(Optional.of(1.5));
        assert optF64Some.isPresent() && Math.abs(optF64Some.get() - 1.5) < 0.0001 : "echoOptionalF64 some";
        demoCase("case:options.primitives.f64.should_roundtrip_none");
        assert !Demo.echoOptionalF64(Optional.empty()).isPresent() : "echoOptionalF64 none";

        demoCase("case:options.primitives.f64.should_find_positive_value");
        Optional<Double> posF64 = Demo.findPositiveF64(3.5);
        assert posF64.isPresent() && Math.abs(posF64.get() - 3.5) < 0.0001 : "findPositiveF64 positive";
        demoCase("case:options.primitives.f64.should_return_none_for_non_positive_value");
        assert !Demo.findPositiveF64(-1.0).isPresent() : "findPositiveF64 non-positive";

        demoCase("case:options.primitives.i32.should_find_even_value");
        Optional<Integer> even = Demo.findEven(8);
        assert even.isPresent() && even.get() == 8 : "findEven even";
        demoCase("case:options.primitives.i32.should_return_none_for_odd_value");
        assert !Demo.findEven(7).isPresent() : "findEven odd";

        demoCase("case:options.primitives.i64.should_find_positive_value");
        Optional<Long> posI64 = Demo.findPositiveI64(123L);
        assert posI64.isPresent() && posI64.get() == 123L : "findPositiveI64 positive";
        demoCase("case:options.primitives.i64.should_return_none_for_non_positive_value");
        assert !Demo.findPositiveI64(-5L).isPresent() : "findPositiveI64 non-positive";

        demoCase("case:options.complex.string.should_find_name_for_positive_id");
        Optional<String> foundName = Demo.findName(3);
        assert foundName.isPresent() && foundName.get().equals("Name_3") : "findName positive";
        demoCase("case:options.complex.string.should_return_none_for_non_positive_id");
        assert !Demo.findName(0).isPresent() : "findName non-positive";

        demoCase("case:options.complex.vec.should_find_numbers_for_positive_count");
        Optional<int[]> foundNumbers = Demo.findNumbers(4);
        assert foundNumbers.isPresent() : "findNumbers positive present";
        int[] foundArr = foundNumbers.get();
        assert foundArr.length == 4 && foundArr[0] == 0 && foundArr[3] == 3 : "findNumbers values";
        demoCase("case:options.complex.vec.should_return_none_for_non_positive_number_count");
        assert !Demo.findNumbers(0).isPresent() : "findNumbers non-positive";

        demoCase("case:options.complex.vec.should_roundtrip_empty_some");
        Optional<int[]> emptySome = Demo.echoOptionalVec(Optional.of(new int[0]));
        assert emptySome.isPresent() && emptySome.get().length == 0 : "echoOptionalVec empty-some";

        demoCase("case:options.complex.vec_string.should_find_names_for_positive_count");
        Optional<java.util.List<String>> foundNames = Demo.findNames(2);
        assert foundNames.isPresent() : "findNames positive present";
        assert foundNames.get().equals(Arrays.asList("Name_0", "Name_1")) : "findNames values";
        demoCase("case:options.complex.vec_string.should_return_none_for_non_positive_name_count");
        assert !Demo.findNames(-1).isPresent() : "findNames non-positive";

        demoCase("case:options.complex.api_result.should_find_success_variant");
        Optional<ApiResult> apiSuccess = Demo.findApiResult(0);
        assert apiSuccess.isPresent() && apiSuccess.get() instanceof ApiResult.Success : "findApiResult success";
        demoCase("case:options.complex.api_result.should_find_error_code_variant");
        Optional<ApiResult> apiErrorCode = Demo.findApiResult(1);
        assert apiErrorCode.isPresent() && apiErrorCode.get() instanceof ApiResult.ErrorCode : "findApiResult error code variant";
        assert ((ApiResult.ErrorCode) apiErrorCode.get()).value0 == -1 : "findApiResult error code value";
        demoCase("case:options.complex.api_result.should_find_error_with_data_variant");
        Optional<ApiResult> apiErrorData = Demo.findApiResult(2);
        assert apiErrorData.isPresent() && apiErrorData.get() instanceof ApiResult.ErrorWithData : "findApiResult error with data variant";
        ApiResult.ErrorWithData err = (ApiResult.ErrorWithData) apiErrorData.get();
        assert err.code == -1 && err.detail == -2 : "findApiResult error-with-data payload";
        demoCase("case:options.complex.api_result.should_return_none_for_unknown_code");
        assert !Demo.findApiResult(99).isPresent() : "findApiResult unknown";

        System.out.println("  PASS\n");
    }

    private static void testRecordsWithVecs() {
        System.out.println("Testing records with vecs...");

        demoCase("case:records.with_collections.polygon.should_make_from_points");
        Polygon polygon = Demo.makePolygon(Arrays.asList(
            new Point(0.0, 0.0), new Point(1.0, 0.0), new Point(0.0, 1.0)
        ));
        demoCase("case:records.with_collections.polygon.should_report_vertex_count");
        assert Demo.polygonVertexCount(polygon) == 3 : "polygonVertexCount";

        demoCase("case:records.with_collections.polygon.should_roundtrip_point_vector");
        Polygon echoed = Demo.echoPolygon(polygon);
        assert echoed.points().size() == 3 : "echoPolygon size";
        assert echoed.points().get(0).x() == 0.0 : "echoPolygon[0].x";

        demoCase("case:records.with_collections.polygon.should_compute_centroid");
        Point centroid = Demo.polygonCentroid(polygon);
        assert Math.abs(centroid.x() - 1.0 / 3.0) < 0.0001 : "polygonCentroid.x";
        assert Math.abs(centroid.y() - 1.0 / 3.0) < 0.0001 : "polygonCentroid.y";

        demoCase("case:records.with_collections.team.should_make_from_members");
        Team team = Demo.makeTeam("devs", Arrays.asList("Alice", "Bob"));
        assert team.name().equals("devs") : "makeTeam.name";
        assert team.members().size() == 2 : "makeTeam.members.size";

        demoCase("case:records.with_collections.team.should_roundtrip_member_vector");
        Team echoedTeam = Demo.echoTeam(team);
        assert echoedTeam.members().get(0).equals("Alice") : "echoTeam.members[0]";
        demoCase("case:records.with_collections.team.should_report_member_count");
        assert Demo.teamSize(team) == 2 : "teamSize";

        demoCase("case:records.with_collections.classroom.should_make_from_students");
        Classroom classroom = Demo.makeClassroom(Arrays.asList(
            new Person("Mia", 10),
            new Person("Leo", 11)
        ));
        assert classroom.students().size() == 2 : "makeClassroom.students.size";
        assert classroom.students().get(0).name().equals("Mia") : "makeClassroom.students[0].name";

        demoCase("case:records.with_collections.classroom.should_roundtrip_student_vector");
        Classroom echoedClassroom = Demo.echoClassroom(classroom);
        assert echoedClassroom.students().size() == 2 : "echoClassroom.students.size";
        assert echoedClassroom.students().get(1).name().equals("Leo") : "echoClassroom.students[1].name";

        demoCase("case:records.with_collections.tagged_scores.should_roundtrip_score_vector");
        TaggedScores ts = Demo.echoTaggedScores(new TaggedScores("math", new double[]{90.0, 85.5}));
        assert ts.label().equals("math") : "echoTaggedScores.label";
        assert ts.scores().length == 2 : "echoTaggedScores.scores.length";
        demoCase("case:records.with_collections.tagged_scores.should_average_scores");
        assert Math.abs(Demo.averageScore(new TaggedScores("x", new double[]{80.0, 100.0})) - 90.0) < 0.0001 : "averageScore";

        MixedRecord record = sampleMixedRecord();
        demoCase("case:records.mixed.should_roundtrip_composed_record");
        assert Demo.echoMixedRecord(record).equals(record) : "echoMixedRecord";
        demoCase("case:records.mixed.should_make_from_composed_parts");
        assert Demo.makeMixedRecord(
            record.name(),
            record.anchor(),
            record.priority(),
            record.shape(),
            record.parameters()
        ).equals(record) : "makeMixedRecord";

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
            demoCase("case:async_fns.basic.add.should_return_sum");
            assert addFuture.get() == 10 : "asyncAdd(3, 7)";

            CompletableFuture<String> echoFuture = Demo.asyncEcho("hello async");
            demoCase("case:async_fns.basic.echo.should_prefix_message");
            assert echoFuture.get().equals("Echo: hello async") : "asyncEcho";

            CompletableFuture<int[]> doubleFuture = Demo.asyncDoubleAll(new int[]{1, 2, 3});
            int[] doubled = doubleFuture.get();
            demoCase("case:async_fns.basic.double_all.should_double_i32_vector");
            assert doubled.length == 3 : "asyncDoubleAll length";
            assert doubled[0] == 2 && doubled[1] == 4 && doubled[2] == 6 : "asyncDoubleAll values";

            CompletableFuture<Optional<Integer>> findSome = Demo.asyncFindPositive(new int[]{-1, 0, 5, 3});
            demoCase("case:async_fns.basic.find_positive.should_return_first_positive");
            assert findSome.get().isPresent() && findSome.get().get() == 5 : "asyncFindPositive some";

            CompletableFuture<Optional<Integer>> findNone = Demo.asyncFindPositive(new int[]{-1, -2, -3});
            demoCase("case:async_fns.basic.find_positive.should_return_none_for_all_negative");
            assert !findNone.get().isPresent() : "asyncFindPositive none";

            CompletableFuture<String> concatFuture = Demo.asyncConcat(Arrays.asList("a", "b", "c"));
            demoCase("case:async_fns.basic.concat.should_join_string_vector");
            assert concatFuture.get().equals("a, b, c") : "asyncConcat";

            MixedRecord record = sampleMixedRecord();
            demoCase("case:async_fns.mixed_record.echo.should_roundtrip_record");
            assert Demo.asyncEchoMixedRecord(record).get().equals(record) : "asyncEchoMixedRecord";
            demoCase("case:async_fns.mixed_record.make.should_construct_record");
            assert Demo.asyncMakeMixedRecord(
                record.name(),
                record.anchor(),
                record.priority(),
                record.shape(),
                record.parameters()
            ).get().equals(record) : "asyncMakeMixedRecord";

            demoCase("case:async_fns.basic.get_numbers.should_return_counting_sequence");
            int[] counting = Demo.asyncGetNumbers(4).get();
            assert counting.length == 4 : "asyncGetNumbers length";
            assert counting[0] == 0 && counting[1] == 1 && counting[2] == 2 && counting[3] == 3 : "asyncGetNumbers values";

            demoCase("case:async_fns.results.fetch_data.should_return_scaled_positive_id");
            assert Demo.fetchData(7).get() == 70 : "fetchData scaled";

            demoCase("case:async_fns.results.fetch_data.should_reject_non_positive_id");
            try {
                Demo.fetchData(0).get();
                assert false : "fetchData should reject non-positive id";
            } catch (java.util.concurrent.ExecutionException ee) {
                assert ee.getCause().getMessage().contains("invalid id") : "fetchData error message";
            }

            demoCase("case:async_fns.results.try_compute.should_return_doubled_value");
            assert Demo.tryComputeAsync(21).get() == 42 : "tryComputeAsync doubled";

            demoCase("case:async_fns.results.try_compute.should_return_invalid_input_for_zero");
            try {
                Demo.tryComputeAsync(0).get();
                assert false : "tryComputeAsync should reject zero";
            } catch (java.util.concurrent.ExecutionException ee) {
                assert ee.getCause() instanceof ComputeError.InvalidInput : "tryComputeAsync zero -> InvalidInput";
                assert ((ComputeError.InvalidInput) ee.getCause()).value0 == -999 : "tryComputeAsync InvalidInput payload";
            }

            demoCase("case:async_fns.results.try_compute.should_return_overflow_for_negative_value");
            try {
                Demo.tryComputeAsync(-3).get();
                assert false : "tryComputeAsync should reject negative";
            } catch (java.util.concurrent.ExecutionException ee) {
                assert ee.getCause() instanceof ComputeError.Overflow : "tryComputeAsync negative -> Overflow";
                ComputeError.Overflow ovf = (ComputeError.Overflow) ee.getCause();
                assert ovf.value == -3 && ovf.limit == 0 : "tryComputeAsync Overflow payload";
            }

            demoCase("case:results.async_results.safe_divide.should_return_quotient");
            assert Demo.asyncSafeDivide(12, 3).get() == 4 : "asyncSafeDivide ok";
            demoCase("case:results.async_results.safe_divide.should_reject_division_by_zero");
            try {
                Demo.asyncSafeDivide(12, 0).get();
                assert false : "asyncSafeDivide should reject zero";
            } catch (java.util.concurrent.ExecutionException ee) {
                assert ee.getCause() instanceof MathError.Exception : "asyncSafeDivide -> MathError.Exception";
                assert ((MathError.Exception) ee.getCause()).getError() == MathError.DIVISION_BY_ZERO : "asyncSafeDivide error variant";
            }

            demoCase("case:results.async_results.fallible_fetch.should_return_value_for_non_negative_key");
            assert Demo.asyncFallibleFetch(7).get().equals("value_7") : "asyncFallibleFetch ok";
            demoCase("case:results.async_results.fallible_fetch.should_reject_negative_key");
            try {
                Demo.asyncFallibleFetch(-1).get();
                assert false : "asyncFallibleFetch should reject negative";
            } catch (java.util.concurrent.ExecutionException ee) {
                assert ee.getCause().getMessage().contains("invalid key") : "asyncFallibleFetch error";
            }

            demoCase("case:results.async_results.find_value.should_return_some_for_positive_key");
            Optional<Integer> findSomeKey = Demo.asyncFindValue(4).get();
            assert findSomeKey.isPresent() && findSomeKey.get() == 40 : "asyncFindValue positive";
            demoCase("case:results.async_results.find_value.should_return_none_for_zero_key");
            assert !Demo.asyncFindValue(0).get().isPresent() : "asyncFindValue zero";
            demoCase("case:results.async_results.find_value.should_reject_negative_key");
            try {
                Demo.asyncFindValue(-1).get();
                assert false : "asyncFindValue should reject negative";
            } catch (java.util.concurrent.ExecutionException ee) {
                assert ee.getCause().getMessage().contains("invalid key") : "asyncFindValue error";
            }
        } catch (Exception e) {
            throw new RuntimeException("async function test failed", e);
        }
        System.out.println("  PASS\n");
    }

    private static MixedRecordParameters sampleMixedRecordParameters() {
        return new MixedRecordParameters(
            Arrays.asList("alpha", "beta"),
            Arrays.asList(new Point(1.0, 2.0), new Point(3.0, 5.0)),
            Optional.of(new Point(-1.0, -2.0)),
            4,
            true
        );
    }

    private static MixedRecord sampleMixedRecord() {
        return new MixedRecord(
            "outline",
            new Point(10.0, 20.0),
            Priority.CRITICAL,
            new Shape.Rectangle(3.0, 4.0),
            sampleMixedRecordParameters()
        );
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

            try (MixedRecordService service = new MixedRecordService("records")) {
                MixedRecord record = sampleMixedRecord();
                assert service.getLabel().equals("records") : "MixedRecordService.getLabel";
                assert service.storedCount() == 0 : "MixedRecordService.storedCount.initial";
                assert service.echoRecord(record).equals(record) : "MixedRecordService.echoRecord";
                assert service.storeRecordParts(
                    record.name(),
                    record.anchor(),
                    record.priority(),
                    record.shape(),
                    record.parameters()
                ).equals(record) : "MixedRecordService.storeRecordParts";
                assert service.storedCount() == 1 : "MixedRecordService.storedCount.sync";
                assert service.asyncEchoRecord(record).get().equals(record) : "MixedRecordService.asyncEchoRecord";
                assert service.asyncStoreRecordParts(
                    record.name(),
                    record.anchor(),
                    record.priority(),
                    record.shape(),
                    record.parameters()
                ).get().equals(record) : "MixedRecordService.asyncStoreRecordParts";
                assert service.storedCount() == 2 : "MixedRecordService.storedCount.async";
            }
        } catch (Exception e) {
            throw new RuntimeException("async class method test failed", e);
        }
        System.out.println("  PASS\n");
    }

    private static void testResultFunctions() {
        System.out.println("Testing result functions...");

        demoCase("case:results.basic.safe_divide.should_return_quotient");
        assert Demo.safeDivide(10, 2) == 5 : "safeDivide ok";
        demoCase("case:results.basic.safe_divide.should_reject_division_by_zero");
        try {
            Demo.safeDivide(10, 0);
            assert false : "safeDivide should throw on zero divisor";
        } catch (RuntimeException e) {
            assert e.getMessage().contains("division by zero") : "safeDivide error message";
        }

        demoCase("case:results.basic.always_ok.should_return_doubled_value");
        assert Demo.alwaysOk(21) == 42 : "alwaysOk";
        demoCase("case:results.basic.always_err.should_return_message_error");
        try {
            Demo.alwaysErr("boom");
            assert false : "alwaysErr should throw";
        } catch (RuntimeException e) {
            assert e.getMessage().contains("boom") : "alwaysErr error message";
        }

        demoCase("case:results.basic.parse_point.should_parse_coordinates");
        Point p = Demo.parsePoint("3.0,4.0");
        assert p.x() == 3.0 : "parsePoint x";
        assert p.y() == 4.0 : "parsePoint y";
        demoCase("case:results.basic.parse_point.should_reject_malformed_input");
        try {
            Demo.parsePoint("bad");
            assert false : "parsePoint should throw on bad input";
        } catch (RuntimeException ignored) {}

        demoCase("case:results.nested_results.string.should_return_value_for_non_negative_key");
        assert Demo.resultOfString(1).equals("item_1") : "resultOfString ok";
        demoCase("case:results.nested_results.string.should_reject_negative_key");
        try {
            Demo.resultOfString(-1);
            assert false : "resultOfString should throw on negative key";
        } catch (RuntimeException ignored) {}

        demoCase("case:results.nested_results.option.should_return_some_for_positive_key");
        Optional<Integer> some = Demo.resultOfOption(5);
        assert some.isPresent() && some.get() == 10 : "resultOfOption present";
        demoCase("case:results.nested_results.option.should_return_none_for_zero_key");
        Optional<Integer> none = Demo.resultOfOption(0);
        assert !none.isPresent() : "resultOfOption empty";
        demoCase("case:results.nested_results.option.should_reject_negative_key");
        try {
            Demo.resultOfOption(-1);
            assert false : "resultOfOption should throw on negative key";
        } catch (RuntimeException ignored) {}

        demoCase("case:results.nested_results.vec.should_return_values_for_non_negative_count");
        int[] vec = Demo.resultOfVec(3);
        assert vec.length == 3 : "resultOfVec length";
        assert vec[0] == 0 && vec[1] == 1 && vec[2] == 2 : "resultOfVec values";
        demoCase("case:results.nested_results.vec.should_reject_negative_count");
        try {
            Demo.resultOfVec(-1);
            assert false : "resultOfVec should throw on negative count";
        } catch (RuntimeException ignored) {}

        demoCase("case:results.basic.divide.should_return_quotient");
        assert Demo.divide(20, 4) == 5 : "divide ok";
        demoCase("case:results.basic.divide.should_reject_division_by_zero");
        try {
            Demo.divide(20, 0);
            assert false : "divide should throw on zero";
        } catch (RuntimeException e) {
            assert e.getMessage().contains("division by zero") : "divide error";
        }

        demoCase("case:results.basic.parse_int.should_parse_integer");
        assert Demo.parseInt("42") == 42 : "parseInt ok";
        demoCase("case:results.basic.parse_int.should_reject_invalid_integer");
        try {
            Demo.parseInt("not-a-number");
            assert false : "parseInt should throw on invalid input";
        } catch (RuntimeException e) {
            assert e.getMessage().contains("invalid integer") : "parseInt error";
        }

        demoCase("case:results.basic.safe_sqrt.should_return_square_root");
        assert Math.abs(Demo.safeSqrt(16.0) - 4.0) < 0.0001 : "safeSqrt ok";
        demoCase("case:results.basic.safe_sqrt.should_reject_negative_input");
        try {
            Demo.safeSqrt(-4.0);
            assert false : "safeSqrt should throw on negative";
        } catch (RuntimeException e) {
            assert e.getMessage().contains("negative input") : "safeSqrt error";
        }

        demoCase("case:results.basic.validate_name.should_greet_valid_name");
        assert Demo.validateName("Java").equals("Hello, Java!") : "validateName ok";
        demoCase("case:results.basic.validate_name.should_reject_empty_name");
        try {
            Demo.validateName("");
            assert false : "validateName should reject empty";
        } catch (RuntimeException e) {
            assert e.getMessage().contains("name cannot be empty") : "validateName error";
        }

        System.out.println("  PASS\n");
    }

    private static void testBorrowedClassRef() {
        System.out.println("Testing borrowed class ref...");

        try (Counter counter = new Counter(42)) {
            assert Demo.describeCounter(counter).equals("Counter(value=42)") : "describeCounter";
        }

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

        demoCase("case:results.error_enums.checked_divide.should_return_quotient");
        assert Demo.checkedDivide(10, 2) == 5 : "checkedDivide ok";
        demoCase("case:results.error_enums.checked_divide.should_reject_division_by_zero");
        try {
            Demo.checkedDivide(10, 0);
            assert false : "checkedDivide should throw on zero divisor";
        } catch (MathError.Exception e) {
            assert e.getError() == MathError.DIVISION_BY_ZERO : "checkedDivide typed error";
        }

        demoCase("case:results.error_enums.checked_sqrt.should_return_square_root");
        assert Demo.checkedSqrt(9.0) == 3.0 : "checkedSqrt ok";
        demoCase("case:results.error_enums.checked_sqrt.should_reject_negative_input");
        try {
            Demo.checkedSqrt(-1.0);
            assert false : "checkedSqrt should throw on negative";
        } catch (MathError.Exception e) {
            assert e.getError() == MathError.NEGATIVE_INPUT : "checkedSqrt typed error";
        }

        demoCase("case:results.error_enums.checked_add.should_return_sum");
        assert Demo.checkedAdd(1, 2) == 3 : "checkedAdd ok";
        demoCase("case:results.error_enums.checked_add.should_reject_overflow");
        try {
            Demo.checkedAdd(Integer.MAX_VALUE, 1);
            assert false : "checkedAdd should throw on overflow";
        } catch (MathError.Exception e) {
            assert e.getError() == MathError.OVERFLOW : "checkedAdd typed error";
        }

        demoCase("case:results.error_enums.validate_username.should_accept_valid_name");
        assert Demo.validateUsername("alice").equals("alice") : "validateUsername ok";
        demoCase("case:results.error_enums.validate_username.should_reject_too_short_name");
        try {
            Demo.validateUsername("ab");
            assert false : "validateUsername should throw on short name";
        } catch (ValidationError.Exception e) {
            assert e.getError() == ValidationError.TOO_SHORT : "validateUsername typed error";
        }
        demoCase("case:results.error_enums.validate_username.should_reject_too_long_name");
        try {
            Demo.validateUsername("a]bcdefghijklmnopqrstu");
            assert false : "validateUsername should throw on long name";
        } catch (ValidationError.Exception e) {
            assert e.getError() == ValidationError.TOO_LONG : "validateUsername typed error";
        }
        demoCase("case:results.error_enums.validate_username.should_reject_invalid_format");
        try {
            Demo.validateUsername("has space");
            assert false : "validateUsername should throw on spaces";
        } catch (ValidationError.Exception e) {
            assert e.getError() == ValidationError.INVALID_FORMAT : "validateUsername typed error";
        }

        demoCase("case:results.error_enums.may_fail.should_return_success_when_valid");
        assert Demo.mayFail(true).equals("Success!") : "mayFail ok";
        demoCase("case:results.error_enums.may_fail.should_return_app_error_when_invalid");
        try {
            Demo.mayFail(false);
            assert false : "mayFail should throw structured AppError";
        } catch (AppError e) {
            assert e.code() == 400 : "mayFail code";
            assert e.message().equals("Invalid input") : "mayFail message field";
            assert e.getMessage().equals("Invalid input") : "mayFail exception message";
        }

        demoCase("case:results.error_enums.api_result_is_success.should_report_success_variant");
        assert Demo.apiResultIsSuccess(ApiResult.Success.INSTANCE) : "apiResultIsSuccess(Success)";
        demoCase("case:results.error_enums.api_result_is_success.should_report_error_variant");
        assert !Demo.apiResultIsSuccess(new ApiResult.ErrorCode(-1)) : "apiResultIsSuccess(ErrorCode)";

        demoCase("case:results.error_enums.process_value.should_return_success_variant");
        assert Demo.processValue(5) instanceof ApiResult.Success : "processValue(positive) -> Success";
        demoCase("case:results.error_enums.process_value.should_return_error_code_variant");
        ApiResult zeroResult = Demo.processValue(0);
        assert zeroResult instanceof ApiResult.ErrorCode : "processValue(0) -> ErrorCode";
        assert ((ApiResult.ErrorCode) zeroResult).value0 == -1 : "processValue(0) error code value";
        demoCase("case:results.error_enums.process_value.should_return_error_with_data_variant");
        ApiResult negResult = Demo.processValue(-3);
        assert negResult instanceof ApiResult.ErrorWithData : "processValue(negative) -> ErrorWithData";

        demoCase("case:results.error_enums.try_compute.should_return_doubled_value");
        assert Demo.tryCompute(21) == 42 : "tryCompute doubled";
        demoCase("case:results.error_enums.try_compute.should_return_overflow_error");
        try {
            Demo.tryCompute(-3);
            assert false : "tryCompute should throw on negative";
        } catch (ComputeError.Overflow ovf) {
            assert ovf.value == -3 && ovf.limit == 0 : "tryCompute Overflow payload";
        }

        demoCase("case:results.error_enums.divide_app.should_return_quotient");
        assert Demo.divideApp(10, 2) == 5 : "divideApp ok";
        demoCase("case:results.error_enums.divide_app.should_return_app_error_for_division_by_zero");
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
