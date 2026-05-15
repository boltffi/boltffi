import Demo
import XCTest

final class WithCollectionsRecordsTests: DemoTestCase {
    func testPolygonFns() {
        demoCase("case:records.with_collections.polygon.should_roundtrip_point_vector")
        XCTAssertEqual(echoPolygon(p: Polygon(points: [Point(x: 0, y: 0), Point(x: 1, y: 0)])), Polygon(points: [Point(x: 0, y: 0), Point(x: 1, y: 0)]))
        demoCase("case:records.with_collections.polygon.should_make_from_points")
        XCTAssertEqual(makePolygon(points: [Point(x: 0, y: 0), Point(x: 1, y: 0), Point(x: 0.5, y: 1)]).points.count, 3)
        demoCase("case:records.with_collections.polygon.should_report_vertex_count")
        XCTAssertEqual(polygonVertexCount(p: Polygon(points: [Point(x: 0, y: 0), Point(x: 1, y: 1)])), 2)
        demoCase("case:records.with_collections.polygon.should_compute_centroid")
        assertPointEquals(polygonCentroid(p: Polygon(points: [Point(x: 0, y: 0), Point(x: 2, y: 0), Point(x: 1, y: 3)])), 1.0, 1.0, accuracy: 1e-6)
    }

    func testTeamFns() {
        demoCase("case:records.with_collections.team.should_roundtrip_member_vector")
        XCTAssertEqual(echoTeam(t: Team(name: "QA", members: ["Dave", "Eve"])), Team(name: "QA", members: ["Dave", "Eve"]))
        demoCase("case:records.with_collections.team.should_make_from_members")
        XCTAssertEqual(makeTeam(name: "Dev Team", members: ["Alice", "Bob", "Charlie"]).members.count, 3)
        demoCase("case:records.with_collections.team.should_report_member_count")
        XCTAssertEqual(teamSize(t: Team(name: "Ops", members: ["Frank", "Grace", "Heidi", "Ivan"])), 4)
    }

    func testClassroomFns() {
        demoCase("case:records.with_collections.classroom.should_roundtrip_student_vector")
        XCTAssertEqual(echoClassroom(c: Classroom(students: [Person(name: "Charlie", age: 25)])), Classroom(students: [Person(name: "Charlie", age: 25)]))
        demoCase("case:records.with_collections.classroom.should_make_from_students")
        XCTAssertEqual(makeClassroom(students: [Person(name: "Alice", age: 20), Person(name: "Bob", age: 22)]).students.count, 2)
    }

    func testTaggedScoresFns() {
        demoCase("case:records.with_collections.tagged_scores.should_roundtrip_score_vector")
        XCTAssertEqual(echoTaggedScores(ts: TaggedScores(label: "set", scores: [1.0, 2.0, 3.0])), TaggedScores(label: "set", scores: [1.0, 2.0, 3.0]))
        demoCase("case:records.with_collections.tagged_scores.should_average_scores")
        XCTAssertEqual(averageScore(ts: TaggedScores(label: "set", scores: [1.0, 2.0, 3.0])), 2.0, accuracy: 1e-9)
    }

    func testBenchmarkUserProfileFns() {
        demoCase("case:records.with_collections.user_profiles.should_generate_profiles")
        let users = generateUserProfiles(count: 3)
        XCTAssertEqual(users.count, 3)
        XCTAssertEqual(users[0], BenchmarkUserProfile(
            id: 0,
            name: "User 0",
            email: "user0@example.com",
            bio: "This is a bio for user 0. It contains enough text to behave like a real payload.",
            age: 20,
            score: 0,
            tags: ["tag0", "category0", "common"],
            scores: [0, 10, 20],
            isActive: true
        ))
        demoCase("case:records.with_collections.user_profiles.should_sum_scores")
        XCTAssertEqual(sumUserScores(users: users), 4.5, accuracy: 1e-9)
        demoCase("case:records.with_collections.user_profiles.should_count_active_users")
        XCTAssertEqual(countActiveUsers(users: users), 2)
    }
}
