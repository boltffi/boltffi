use boltffi::*;
use demo_bench_macros::benchmark_candidate;

use crate::records::blittable::Point;
use crate::records::with_strings::Person;

#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Polygon {
    pub points: Vec<Point>,
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_collections.polygon.should_roundtrip_point_vector",
    description = "A Polygon record containing a vector of Point records crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with vector fields."
    )
)]
pub fn echo_polygon(p: Polygon) -> Polygon {
    p
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_collections.polygon.should_make_from_points",
    description = "make_polygon builds a Polygon from a vector of Point records.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with vector fields."
    )
)]
pub fn make_polygon(points: Vec<Point>) -> Polygon {
    Polygon { points }
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_collections.polygon.should_report_vertex_count",
    description = "polygon_vertex_count returns the number of Point records contained in a Polygon.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with vector fields."
    )
)]
pub fn polygon_vertex_count(p: Polygon) -> u32 {
    p.points.len() as u32
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_collections.polygon.should_compute_centroid",
    description = "polygon_centroid receives a Polygon record and returns the average of its Point coordinates.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with vector fields."
    )
)]
pub fn polygon_centroid(p: Polygon) -> Point {
    if p.points.is_empty() {
        return Point { x: 0.0, y: 0.0 };
    }
    let count = p.points.len() as f64;
    let sum_x: f64 = p.points.iter().map(|pt| pt.x).sum();
    let sum_y: f64 = p.points.iter().map(|pt| pt.y).sum();
    Point {
        x: sum_x / count,
        y: sum_y / count,
    }
}

#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Team {
    pub name: String,
    pub members: Vec<String>,
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_collections.team.should_roundtrip_member_vector",
    description = "A Team record containing a vector of member strings crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with vector fields."
    )
)]
pub fn echo_team(t: Team) -> Team {
    t
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_collections.team.should_make_from_members",
    description = "make_team builds a Team record from a name and vector of member strings.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with vector fields."
    )
)]
pub fn make_team(name: String, members: Vec<String>) -> Team {
    Team { name, members }
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_collections.team.should_report_member_count",
    description = "team_size returns the number of member strings contained in a Team record.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with vector fields."
    )
)]
pub fn team_size(t: Team) -> u32 {
    t.members.len() as u32
}

#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Classroom {
    pub students: Vec<Person>,
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_collections.classroom.should_roundtrip_student_vector",
    description = "A Classroom record containing a vector of Person records crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with vector fields."
    )
)]
pub fn echo_classroom(c: Classroom) -> Classroom {
    c
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_collections.classroom.should_make_from_students",
    description = "make_classroom builds a Classroom from a vector of Person records.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with vector fields."
    )
)]
pub fn make_classroom(students: Vec<Person>) -> Classroom {
    Classroom { students }
}

#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TaggedScores {
    pub label: String,
    pub scores: Vec<f64>,
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_collections.tagged_scores.should_roundtrip_score_vector",
    description = "A TaggedScores record containing a label and score vector crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with vector fields."
    )
)]
pub fn echo_tagged_scores(ts: TaggedScores) -> TaggedScores {
    ts
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_collections.tagged_scores.should_average_scores",
    description = "average_score receives a TaggedScores record and returns the average of its score vector.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with vector fields."
    )
)]
pub fn average_score(ts: TaggedScores) -> f64 {
    if ts.scores.is_empty() {
        return 0.0;
    }
    let sum: f64 = ts.scores.iter().sum();
    sum / ts.scores.len() as f64
}

/// A heavier benchmark profile with heap-owned collections.
#[benchmark_candidate(record, uniffi)]
#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BenchmarkUserProfile {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub bio: String,
    pub age: i32,
    pub score: f64,
    pub tags: Vec<String>,
    pub scores: Vec<i32>,
    pub is_active: bool,
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.with_collections.user_profiles.should_generate_profiles",
    description = "generate_user_profiles returns a vector of benchmark profile records with nested vectors.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover benchmark user profile helpers."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover benchmark user profile helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with vector fields."
    ),
    exclude(
        wasm,
        reason = "The wasm demo tests do not currently cover benchmark user profile helpers."
    )
)]
pub fn generate_user_profiles(count: i32) -> Vec<BenchmarkUserProfile> {
    (0..count as i64)
        .map(|index| BenchmarkUserProfile {
            id: index,
            name: format!("User {index}"),
            email: format!("user{index}@example.com"),
            bio: format!(
                "This is a bio for user {index}. It contains enough text to behave like a real payload."
            ),
            age: 20 + (index % 50) as i32,
            score: index as f64 * 1.5,
            tags: vec![
                format!("tag{}", index % 5),
                format!("category{}", index % 3),
                "common".to_string(),
            ],
            scores: vec![
                (index % 100) as i32,
                ((index + 10) % 100) as i32,
                ((index + 20) % 100) as i32,
            ],
            is_active: index % 2 == 0,
        })
        .collect()
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.with_collections.user_profiles.should_sum_scores",
    description = "sum_user_scores receives a vector of benchmark profile records and sums their score fields.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover benchmark user profile helpers."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover benchmark user profile helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with vector fields."
    ),
    exclude(
        wasm,
        reason = "The wasm demo tests do not currently cover benchmark user profile helpers."
    )
)]
pub fn sum_user_scores(users: Vec<BenchmarkUserProfile>) -> f64 {
    users.iter().map(|user| user.score).sum()
}

#[export]
#[benchmark_candidate(function, uniffi)]
#[demo_bench_macros::demo_case(
    "records.with_collections.user_profiles.should_count_active_users",
    description = "count_active_users receives a vector of benchmark profile records and counts active users.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover benchmark user profile helpers."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover benchmark user profile helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover records with vector fields."
    ),
    exclude(
        wasm,
        reason = "The wasm demo tests do not currently cover benchmark user profile helpers."
    )
)]
pub fn count_active_users(users: Vec<BenchmarkUserProfile>) -> i32 {
    users.iter().filter(|user| user.is_active).count() as i32
}
