package com.boltffi.demo

import java.net.URI
import java.time.Duration
import java.time.Instant
import java.util.UUID
import kotlin.math.PI
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

class DemoValueTypesTest {
    @Test
    fun builtinsAndCustomTypesRoundTrip() {
        val duration = Duration.ofSeconds(2).plusMillis(500)
        assertEquals(duration, echoDuration(duration))
        assertEquals(Duration.ofSeconds(3).plusNanos(25), makeDuration(3uL, 25u))
        assertEquals(2_500uL, durationAsMillis(duration))

        val instant = Instant.ofEpochMilli(1_701_234_567_890L)
        assertEquals(instant, echoSystemTime(instant))
        assertEquals(1_701_234_567_890uL, systemTimeToMillis(instant))
        assertEquals(instant, millisToSystemTime(1_701_234_567_890uL))

        val uuid = UUID.fromString("123e4567-e89b-12d3-a456-426614174000")
        assertEquals(uuid, echoUuid(uuid))
        assertEquals(uuid.toString(), uuidToString(uuid))

        val url = URI("https://example.com/demo?q=boltffi")
        assertEquals(url, echoUrl(url))
        assertEquals(url.toString(), urlToString(url))

        val email = "ali@example.com"
        assertEquals(email, echoEmail(email))
        assertEquals("example.com", emailDomain(email))

        val datetime: UtcDateTime = 1_701_234_567_890L
        assertEquals(datetime, echoDatetime(datetime))
        assertEquals(1_701_234_567_890L, datetimeToMillis(datetime))

    }

    @Test
    fun primitivesStringsBytesAndVectorsRoundTrip() {
        assertEquals(true, echoBool(true))
        assertEquals(true, negateBool(false))
        assertEquals((-7).toByte(), echoI8((-7).toByte()))
        assertEquals(255u.toUByte(), echoU8(255u.toUByte()))
        assertEquals((-1234).toShort(), echoI16((-1234).toShort()))
        assertEquals(55_000u.toUShort(), echoU16(55_000u.toUShort()))
        assertEquals(-42, echoI32(-42))
        assertEquals(30, addI32(10, 20))
        assertEquals(4_000_000_000u, echoU32(4_000_000_000u))
        assertEquals(-9_999_999_999L, echoI64(-9_999_999_999L))
        assertEquals(9_999_999_999uL, echoU64(9_999_999_999uL))
        assertFloatEquals(3.5f, echoF32(3.5f))
        assertFloatEquals(4.0f, addF32(1.5f, 2.5f))
        assertDoubleEquals(3.14159265359, echoF64(3.14159265359))
        assertDoubleEquals(4.0, addF64(1.5, 2.5))
        assertEquals(123uL, echoUsize(123uL))
        assertEquals(-123L, echoIsize(-123L))

        assertEquals("hello 🌍", echoString("hello 🌍"))
        assertEquals("foobar", concatStrings("foo", "bar"))
        assertEquals(5u, stringLength("café"))
        assertEquals(true, stringIsEmpty(""))
        assertEquals("ababab", repeatString("ab", 3u))

        assertContentEquals(byteArrayOf(1, 2, 3, 4), echoBytes(byteArrayOf(1, 2, 3, 4)))
        assertEquals(3u, bytesLength(byteArrayOf(9, 8, 7)))
        assertEquals(10u, bytesSum(byteArrayOf(1, 2, 3, 4)))
        assertContentEquals(byteArrayOf(0, 1, 2, 3), makeBytes(4u))
        assertContentEquals(byteArrayOf(4, 3, 2, 1), reverseBytes(byteArrayOf(1, 2, 3, 4)))

        assertContentEquals(intArrayOf(1, 2, 3), echoVecI32(intArrayOf(1, 2, 3)))
        assertContentEquals(byteArrayOf(-1, 0, 7), echoVecI8(byteArrayOf(-1, 0, 7)))
        assertContentEquals(byteArrayOf(0, 1, 2, 3), echoVecU8(byteArrayOf(0, 1, 2, 3)))
        assertContentEquals(shortArrayOf(-3, 0, 9), echoVecI16(shortArrayOf(-3, 0, 9)))
        assertContentEquals(shortArrayOf(0, 10, 20), echoVecU16(shortArrayOf(0, 10, 20)))
        assertContentEquals(intArrayOf(0, 10, 20), echoVecU32(intArrayOf(0, 10, 20)))
        assertContentEquals(longArrayOf(-5L, 0L, 8L), echoVecI64(longArrayOf(-5L, 0L, 8L)))
        assertContentEquals(longArrayOf(0L, 1L, 2L), echoVecU64(longArrayOf(0L, 1L, 2L)))
        assertContentEquals(longArrayOf(-2L, 0L, 5L), echoVecIsize(longArrayOf(-2L, 0L, 5L)))
        assertContentEquals(longArrayOf(0L, 2L, 4L), echoVecUsize(longArrayOf(0L, 2L, 4L)))
        assertContentEquals(floatArrayOf(1.25f, -2.5f), echoVecF32(floatArrayOf(1.25f, -2.5f)))
        assertContentEquals(doubleArrayOf(1.5, 2.5), echoVecF64(doubleArrayOf(1.5, 2.5)))
        assertContentEquals(booleanArrayOf(true, false, true), echoVecBool(booleanArrayOf(true, false, true)))
        assertContentEquals(listOf("hello", "world"), echoVecString(listOf("hello", "world")))
        assertContentEquals(intArrayOf(2, 5), vecStringLengths(listOf("hi", "café")))
        assertEquals(60L, sumVecI32(intArrayOf(10, 20, 30)))
        assertContentEquals(intArrayOf(0, 1, 2, 3, 4), makeRange(0, 5))
        assertContentEquals(intArrayOf(3, 2, 1), reverseVecI32(intArrayOf(1, 2, 3)))
    }

    @Test
    fun nestedVecsRoundTrip() {
        val input = listOf(intArrayOf(1, 2, 3), intArrayOf(), intArrayOf(4, 5))
        val roundTripped = echoVecVecI32(input)
        assertEquals(input.size, roundTripped.size)
        for (i in input.indices) {
            assertContentEquals(input[i], roundTripped[i])
        }
        assertEquals(0, echoVecVecI32(emptyList()).size)

        val bools = listOf(booleanArrayOf(true, false, true), booleanArrayOf(), booleanArrayOf(false))
        val roundTrippedBools = echoVecVecBool(bools)
        assertEquals(bools.size, roundTrippedBools.size)
        for (i in bools.indices) {
            assertContentEquals(bools[i], roundTrippedBools[i])
        }

        val isizes = listOf(longArrayOf(-2L, 0L, 5L), longArrayOf(), longArrayOf(9L))
        val roundTrippedIsizes = echoVecVecIsize(isizes)
        assertEquals(isizes.size, roundTrippedIsizes.size)
        for (i in isizes.indices) {
            assertContentEquals(isizes[i], roundTrippedIsizes[i])
        }

        val usizes = listOf(longArrayOf(0L, 2L, 4L), longArrayOf(), longArrayOf(8L))
        val roundTrippedUsizes = echoVecVecUsize(usizes)
        assertEquals(usizes.size, roundTrippedUsizes.size)
        for (i in usizes.indices) {
            assertContentEquals(usizes[i], roundTrippedUsizes[i])
        }

        val strings = listOf(listOf("hello", "world"), emptyList(), listOf("café", "🌍"))
        assertEquals(strings, echoVecVecString(strings))

        assertContentEquals(
            intArrayOf(1, 2, 3, 4, 5),
            flattenVecVecI32(listOf(intArrayOf(1, 2), intArrayOf(3), intArrayOf(), intArrayOf(4, 5))),
        )
        assertContentEquals(intArrayOf(), flattenVecVecI32(emptyList()))
    }

    @Test
    fun blittableRecordVecsRoundTrip() {
        val locations = generateLocations(3)
        assertEquals(3, locations.size)
        assertEquals(3, processLocations(locations))
        assertDoubleEquals(9.3, sumRatings(locations))

        val trades = generateTrades(3)
        assertEquals(3, trades.size)
        assertEquals(3000L, sumTradeVolumes(trades))
        assertEquals(3002L, aggregateLocationTradeStats(locations, trades))

        val particles = generateParticles(3)
        assertEquals(3, particles.size)
        assertDoubleEquals(3.003, sumParticleMasses(particles))

        val readings = generateSensorReadings(3)
        assertEquals(3, readings.size)
        assertDoubleEquals(21.0, avgSensorTemperature(readings))
    }

    @Test
    fun optionFunctionsUseCorrectKotlinSurface() {
        assertEquals(7, echoOptionalI32(7))
        assertNull(echoOptionalI32(null))
        assertDoubleEquals(4.5, echoOptionalF64(4.5)!!)
        assertNull(echoOptionalF64(null))
        assertEquals(true, echoOptionalBool(true))
        assertNull(echoOptionalBool(null))
        assertEquals(9, unwrapOrDefaultI32(9, 4))
        assertEquals(4, unwrapOrDefaultI32(null, 4))
        assertEquals(12, makeSomeI32(12))
        assertNull(makeNoneI32())
        assertEquals(16, doubleIfSome(8))
        assertNull(doubleIfSome(null))

        assertEquals("hello", echoOptionalString("hello"))
        assertNull(echoOptionalString(null))
        assertEquals(true, isSomeString("x"))
        assertEquals(false, isSomeString(null))

        assertPointEquals(1.0, 2.0, echoOptionalPoint(Point(1.0, 2.0))!!)
        assertNull(echoOptionalPoint(null))
        assertPointEquals(3.0, 4.0, makeSomePoint(3.0, 4.0)!!)
        assertNull(makeNonePoint())

        assertEquals(Status.ACTIVE, echoOptionalStatus(Status.ACTIVE))
        assertNull(echoOptionalStatus(null))
        assertContentEquals(intArrayOf(1, 2, 3), echoOptionalVec(intArrayOf(1, 2, 3)))
        assertNull(echoOptionalVec(null))
        assertEquals(2u, optionalVecLength(intArrayOf(9, 8)))
        assertNull(optionalVecLength(null))
    }

    @Test
    fun basicStringResultFunctionsUseCorrectKotlinSurface() {
        assertEquals(5, safeDivide(10, 2))
        assertMessageContains(assertFailsWith<FfiException> { safeDivide(1, 0) }, "division by zero")
        assertDoubleEquals(3.0, safeSqrt(9.0))
        assertMessageContains(assertFailsWith<FfiException> { safeSqrt(-1.0) }, "negative input")
        assertPointEquals(1.5, 2.5, parsePoint("1.5, 2.5"))
        assertMessageContains(assertFailsWith<FfiException> { parsePoint("wat") }, "expected format")
        assertEquals(42, alwaysOk(21))
        assertMessageContains(assertFailsWith<FfiException> { alwaysErr("boom") }, "boom")
        assertEquals("ok: 7", resultToString(BoltFFIResult.Ok(7)))
        assertEquals("err: bad", resultToString(BoltFFIResult.Err("bad")))
    }

    @Test
    fun typedErrorResultFunctionsUseCorrectKotlinSurface() {
        assertEquals(5, checkedDivide(10, 2))
        assertTrue(assertFailsWith<MathError> { checkedDivide(1, 0) } is MathError.DivisionByZero)
        assertDoubleEquals(3.0, checkedSqrt(9.0))
        assertTrue(assertFailsWith<MathError> { checkedSqrt(-1.0) } is MathError.NegativeInput)
        assertTrue(assertFailsWith<MathError> { checkedAdd(Int.MAX_VALUE, 1) } is MathError.Overflow)

        assertEquals("Success!", mayFail(true))
        val invalidInputError = assertFailsWith<AppError> { mayFail(false) }
        assertEquals(400, invalidInputError.code)
        assertEquals("Invalid input", invalidInputError.message)

        assertEquals(5, divideApp(10, 2))
        val divideByZeroError = assertFailsWith<AppError> { divideApp(10, 0) }
        assertEquals(500, divideByZeroError.code)
        assertEquals("Division by zero", divideByZeroError.message)

        assertEquals("valid_name", validateUsername("valid_name"))
        assertTrue(assertFailsWith<ValidationError> { validateUsername("ab") } is ValidationError.TooShort)
        assertTrue(
            assertFailsWith<ValidationError> { validateUsername("a".repeat(21)) } is ValidationError.TooLong
        )
        assertTrue(
            assertFailsWith<ValidationError> { validateUsername("has space") } is ValidationError.InvalidFormat
        )
    }

    @Test
    fun nestedResultFunctionsUseCorrectKotlinSurface() {
        assertEquals(8, resultOfOption(4))
        assertNull(resultOfOption(0))
        assertMessageContains(assertFailsWith<FfiException> { resultOfOption(-1) }, "invalid key")
        assertContentEquals(intArrayOf(0, 1, 2), resultOfVec(3))
        assertMessageContains(assertFailsWith<FfiException> { resultOfVec(-1) }, "negative count")
        assertEquals("item_7", resultOfString(7))
        assertMessageContains(assertFailsWith<FfiException> { resultOfString(-1) }, "invalid key")
    }

    @Test
    fun enumAndDataEnumExportsBehaveCorrectly() {
        assertEquals(Status.ACTIVE, echoStatus(Status.ACTIVE))
        assertEquals("active", statusToString(Status.ACTIVE))
        assertEquals(false, isActive(Status.PENDING))
        assertEquals(listOf(Status.ACTIVE, Status.PENDING), echoVecStatus(listOf(Status.ACTIVE, Status.PENDING)))

        assertEquals(Direction.WEST, Direction.new(3))
        assertEquals(Direction.NORTH, Direction.cardinal())
        assertEquals(Direction.EAST, Direction.fromDegrees(90.0))
        assertEquals(4u, Direction.count())
        assertEquals(Direction.SOUTH, Direction.NORTH.opposite())
        assertEquals(true, Direction.EAST.isHorizontal())
        assertEquals("W", Direction.WEST.label())
        assertEquals(Direction.EAST, echoDirection(Direction.EAST))
        assertEquals(Direction.WEST, oppositeDirection(Direction.EAST))

        val nameFilter = Filter.ByName("ali")
        val pointFilter = Filter.ByPoints(listOf(Point(0.0, 0.0), Point(1.0, 1.0)))
        val groupFilter = Filter.ByGroups(listOf(listOf("café", "🌍"), emptyList(), listOf("common")))
        assertEquals(Filter.None, echoFilter(Filter.None))
        assertEquals(nameFilter, echoFilter(nameFilter))
        assertEquals(groupFilter, echoFilter(groupFilter))
        assertEquals("filter by name: ali", describeFilter(nameFilter))
        assertEquals("filter by 2 anchor points", describeFilter(pointFilter))
        assertEquals("filter by 2 tags", describeFilter(Filter.ByTags(listOf("ffi", "jni"))))
        assertEquals("filter by 3 groups", describeFilter(groupFilter))
        assertEquals("filter by range: 1..5", describeFilter(Filter.ByRange(1.0, 5.0)))

        val success = ApiResponse.Success("ok")
        val redirect = ApiResponse.Redirect("https://example.com")
        assertEquals(success, echoApiResponse(success))
        assertEquals(redirect, echoApiResponse(redirect))
        assertEquals(true, isSuccess(success))
        assertEquals(false, isSuccess(ApiResponse.Empty))

        assertEquals(Priority.HIGH, echoPriority(Priority.HIGH))
        assertEquals("low", priorityLabel(Priority.LOW))
        assertEquals(true, isHighPriority(Priority.CRITICAL))
        assertEquals(false, isHighPriority(Priority.LOW))
        assertEquals(LogLevel.INFO, echoLogLevel(LogLevel.INFO))
        assertEquals(true, shouldLog(LogLevel.ERROR, LogLevel.WARN))
        assertEquals(
            listOf(LogLevel.TRACE, LogLevel.INFO, LogLevel.ERROR),
            echoVecLogLevel(listOf(LogLevel.TRACE, LogLevel.INFO, LogLevel.ERROR))
        )

        assertEquals(200.toShort(), HttpCode.OK.value)
        assertEquals(404.toShort(), HttpCode.NOT_FOUND.value)
        assertEquals(500.toShort(), HttpCode.SERVER_ERROR.value)
        assertEquals(HttpCode.NOT_FOUND, httpCodeNotFound())
        assertEquals(HttpCode.OK, echoHttpCode(HttpCode.OK))
        assertEquals(HttpCode.SERVER_ERROR, echoHttpCode(HttpCode.SERVER_ERROR))

        assertEquals((-1).toByte(), Sign.NEGATIVE.value)
        assertEquals(0.toByte(), Sign.ZERO.value)
        assertEquals(1.toByte(), Sign.POSITIVE.value)
        assertEquals(Sign.NEGATIVE, signNegative())
        assertEquals(Sign.NEGATIVE, echoSign(Sign.NEGATIVE))
        assertEquals(Sign.POSITIVE, echoSign(Sign.POSITIVE))

        val circle = Shape.new(5.0)
        assertIs<Shape.Circle>(circle)
        assertIs<Shape.Circle>(Shape.unitCircle())
        assertIs<Shape.Rectangle>(Shape.square(3.0))
        assertIs<Shape.Circle>(Shape.tryCircle(2.0))
        assertMessageContains(assertFailsWith<FfiException> { Shape.tryCircle(-1.0) }, "radius must be positive")
        assertEquals(4u, Shape.variantCount())
        assertDoubleEquals(PI * 25.0, circle.area(), 1e-6)
        assertEquals("circle r=5", circle.describe())
        assertIs<Shape.Circle>(echoShape(makeCircle(2.0)))
        assertIs<Shape.Rectangle>(echoShape(makeRectangle(3.0, 4.0)))
        assertEquals(3, echoVecShape(listOf(Shape.Circle(2.0), Shape.Rectangle(3.0, 4.0), Shape.Point)).size)

        assertEquals(Message.Text("hello"), echoMessage(Message.Text("hello")))
        assertEquals(
            Message.Image("https://example.com/image.png", 640u, 480u),
            echoMessage(Message.Image("https://example.com/image.png", 640u, 480u))
        )
        assertEquals("text: hi", messageSummary(Message.Text("hi")))
        assertEquals(
            "image: 640x480 at https://example.com/image.png",
            messageSummary(Message.Image("https://example.com/image.png", 640u, 480u))
        )
        assertEquals("ping", messageSummary(Message.Ping))

        assertEquals(Animal.Dog("Rex", "Labrador"), echoAnimal(Animal.Dog("Rex", "Labrador")))
        assertEquals(Animal.Cat("Milo", true), echoAnimal(Animal.Cat("Milo", true)))
        assertEquals("5 fish", animalName(Animal.Fish(5u)))
        assertEquals("Milo", animalName(Animal.Cat("Milo", true)))
    }

    @Test
    fun pointAndRecordMethodExportsBehaveCorrectly() {
        assertEquals(Point(1.0, 2.0), Point.new(1.0, 2.0))
        assertPointEquals(0.0, 0.0, Point.origin())
        assertPointEquals(0.0, 2.0, Point.fromPolar(2.0, PI / 2.0), 1e-6)
        assertPointEquals(0.6, 0.8, Point.tryUnit(3.0, 4.0), 1e-6)
        assertMessageContains(assertFailsWith<FfiException> { Point.tryUnit(0.0, 0.0) }, "cannot normalize zero vector")
        assertPointEquals(1.0, 0.0, Point.checkedUnit(2.0, 0.0)!!, 1e-6)
        assertNull(Point.checkedUnit(0.0, 0.0))
        assertEquals(2u, Point.dimensions())
        assertDoubleEquals(5.0, Point(3.0, 4.0).distance())
        assertPointEquals(6.0, 8.0, Point(3.0, 4.0).scale(2.0))
        assertPointEquals(4.0, 6.0, Point(1.0, 2.0).add(Point(3.0, 4.0)))
        assertEquals(Point(1.0, 2.0), echoPoint(Point(1.0, 2.0)))
        assertEquals(Point(2.0, 3.0), tryMakePoint(2.0, 3.0))
        assertNull(tryMakePoint(0.0, 0.0))
        assertEquals(Point(1.0, 2.0), makePoint(1.0, 2.0))
        assertEquals(Point(8.0, 10.0), addPoints(Point(3.0, 4.0), Point(5.0, 6.0)))

        val color = Color(1u, 2u, 3u, 255u)
        assertEquals(color, echoColor(color))
        assertEquals(Color(9u, 8u, 7u, 6u), makeColor(9u, 8u, 7u, 6u))

        val line = makeLine(0.0, 0.0, 3.0, 4.0)
        assertEquals(line, echoLine(line))
        assertDoubleEquals(5.0, lineLength(line))

        val rect = Rect(Point(1.0, 2.0), Dimensions(3.0, 4.0))
        assertEquals(rect, echoRect(rect))
        assertDoubleEquals(12.0, rectArea(rect))
    }

    @Test
    fun collectionAndNestedRecordExportsBehaveCorrectly() {
        val polygon = makePolygon(listOf(Point(0.0, 0.0), Point(1.0, 0.0), Point(0.0, 1.0)))
        assertEquals(polygon, echoPolygon(polygon))
        assertEquals(3u, polygonVertexCount(polygon))
        assertPointEquals(1.0 / 3.0, 1.0 / 3.0, polygonCentroid(polygon), 1e-6)

        val team = makeTeam("devs", listOf("Ali", "Mia"))
        assertEquals(team, echoTeam(team))
        assertEquals(2u, teamSize(team))

        val classroom = makeClassroom(listOf(Person("Mia", 10u), Person("Leo", 11u)))
        assertEquals(classroom, echoClassroom(classroom))

        val taggedScores = TaggedScores("math", doubleArrayOf(90.0, 85.5))
        val echoedTaggedScores = echoTaggedScores(taggedScores)
        assertEquals("math", echoedTaggedScores.label)
        assertContentEquals(doubleArrayOf(90.0, 85.5), echoedTaggedScores.scores)
        assertDoubleEquals(90.0, averageScore(TaggedScores("x", doubleArrayOf(80.0, 100.0))))

        val task = makeTask("ship bindings", Priority.CRITICAL)
        assertEquals(task, echoTask(task))
        assertEquals(false, task.completed)
        assertEquals(true, isUrgent(task))

        val notification = Notification("heads up", Priority.HIGH, false)
        assertEquals(notification, echoNotification(notification))

        val triangleHolder = makeTriangleHolder()
        assertIs<Shape.Triangle>(triangleHolder.shape)
        assertEquals(triangleHolder, echoHolder(triangleHolder))

        val header = makeCriticalTaskHeader(42L)
        assertEquals(42L, header.id)
        assertEquals(Priority.CRITICAL, header.priority)
        assertEquals(false, header.completed)
        assertEquals(header, echoTaskHeader(header))

        val started = makeCriticalLifecycleEvent(7L)
        assertIs<LifecycleEvent.TaskStarted>(started)
        assertEquals(Priority.CRITICAL, started.priority)
        assertEquals(7L, started.id)
        assertEquals(started, echoLifecycleEvent(started))
        assertEquals(LifecycleEvent.Tick, echoLifecycleEvent(LifecycleEvent.Tick))

        val logEntry = makeErrorLogEntry(1234567890L, 42.toUShort())
        assertEquals(1234567890L, logEntry.timestamp)
        assertEquals(LogLevel.ERROR, logEntry.level)
        assertEquals(42.toUShort(), logEntry.code)
        assertEquals(logEntry, echoLogEntry(logEntry))

        val userProfile = makeUserProfile("Alice", 30u, "alice@example.com", 98.5)
        assertEquals(userProfile, echoUserProfile(userProfile))
        assertEquals("Alice <alice@example.com>", userDisplayName(userProfile))
        assertEquals("Bob", userDisplayName(makeUserProfile("Bob", 22u, null, null)))

        val searchResult = SearchResult("rust ffi", 12u, "cursor-1", 0.99)
        assertEquals(searchResult, echoSearchResult(searchResult))
        assertEquals(true, hasMoreResults(searchResult))
        assertEquals(false, hasMoreResults(SearchResult("rust ffi", 12u, null, null)))

        val person = makePerson("Ali", 30u)
        assertEquals(person, echoPerson(person))
        assertEquals("Hello, Ali! You are 30 years old.", greetPerson(person))

        val address = Address("Main St", "Amsterdam", "1000AA")
        assertEquals(address, echoAddress(address))
        assertEquals("Main St, Amsterdam, 1000AA", formatAddress(address))
    }

    @Test
    fun recordDefaultValuesSurfaceCorrectly() {
        val implicitDefaults = ServiceConfig("worker")
        assertEquals("worker", implicitDefaults.name)
        assertEquals(3, implicitDefaults.retries)
        assertEquals("standard", implicitDefaults.region)
        assertNull(implicitDefaults.endpoint)
        assertEquals("https://default", implicitDefaults.backupEndpoint)

        val customRetries = ServiceConfig("worker", 7)
        assertEquals("worker", customRetries.name)
        assertEquals(7, customRetries.retries)
        assertEquals("standard", customRetries.region)
        assertNull(customRetries.endpoint)
        assertEquals("https://default", customRetries.backupEndpoint)

        val explicitRegion = ServiceConfig("worker", 9, "eu-west")
        assertNull(explicitRegion.endpoint)
        assertEquals("https://default", explicitRegion.backupEndpoint)

        val explicitEndpoint = ServiceConfig("worker", 9, "eu-west", "https://edge")
        assertEquals("https://default", explicitEndpoint.backupEndpoint)

        val explicitBackupEndpoint = ServiceConfig(
            "worker",
            9,
            "eu-west",
            "https://edge",
            "https://backup"
        )
        assertEquals(explicitBackupEndpoint, echoServiceConfig(explicitBackupEndpoint))
        assertEquals("worker:3:standard:none:https://default", implicitDefaults.describe())
        assertEquals("worker:7:standard:none:https://default", customRetries.describe())
        assertEquals("worker:9:eu-west:none:https://default", explicitRegion.describe())
        assertEquals("worker:9:eu-west:https://edge:https://default", explicitEndpoint.describe())
        assertEquals("worker:9:eu-west:https://edge:https://backup", explicitBackupEndpoint.describe())
    }
}
