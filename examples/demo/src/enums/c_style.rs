use boltffi::*;
use demo_bench_macros::benchmark_candidate;

/// Lifecycle status of an entity.
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Active,
    Inactive,
    Pending,
}

#[demo_bench_macros::demo_case(
    "enums.c_style.status.should_roundtrip_values",
    justification = "Ensure Status enum values cross the FFI boundary and return unchanged.",
    directions = "Call `enums::c_style::echo_status` through the generated binding and assert Status enum values cross the FFI boundary and return unchanged."
)]
#[export]
pub fn echo_status(s: Status) -> Status {
    s
}

#[demo_bench_macros::demo_case(
    "enums.c_style.status.should_render_labels",
    justification = "Ensure status_to_string maps Status enum values to their string labels.",
    directions = "Call `enums::c_style::status_to_string` through the generated binding and assert status_to_string maps Status enum values to their string labels."
)]
#[export]
pub fn status_to_string(s: Status) -> String {
    match s {
        Status::Active => "active".to_string(),
        Status::Inactive => "inactive".to_string(),
        Status::Pending => "pending".to_string(),
    }
}

#[demo_bench_macros::demo_case(
    "enums.c_style.status.should_identify_active_values",
    justification = "Ensure is_active returns true only for the active Status variant.",
    directions = "Call `enums::c_style::is_active` through the generated binding and assert is_active returns true only for the active Status variant."
)]
#[export]
pub fn is_active(s: Status) -> bool {
    matches!(s, Status::Active)
}

#[demo_bench_macros::demo_case(
    "enums.c_style.status.should_roundtrip_vectors",
    justification = "Ensure a vector of Status enum values preserves variant order and values.",
    directions = "Call `enums::c_style::echo_vec_status` through the generated binding and assert a vector of Status enum values preserves variant order and values."
)]
#[export]
pub fn echo_vec_status(values: Vec<Status>) -> Vec<Status> {
    values
}

#[benchmark_candidate(enum, uniffi, wasm_bindgen)]
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    North,
    South,
    East,
    West,
}

#[data(impl)]
impl Direction {
    #[demo_bench_macros::demo_case(
        "enums.c_style.direction.should_construct_from_raw_value",
        justification = "Ensure Direction::new maps raw integer values to Direction variants.",
        directions = "Call `enums::c_style::Direction::new` through the generated binding and assert Direction::new maps raw integer values to Direction variants."
    )]
    pub fn new(raw: i32) -> Self {
        match raw {
            0 => Direction::North,
            1 => Direction::South,
            2 => Direction::East,
            3 => Direction::West,
            _ => Direction::North,
        }
    }

    #[demo_bench_macros::demo_case(
        "enums.c_style.direction.should_return_cardinal_value",
        justification = "Ensure Direction::cardinal returns the North direction variant.",
        directions = "Call `enums::c_style::Direction::cardinal` through the generated binding and assert Direction::cardinal returns the North direction variant."
    )]
    pub fn cardinal() -> Self {
        Direction::North
    }

    #[demo_bench_macros::demo_case(
        "enums.c_style.direction.should_construct_from_degrees",
        justification = "Ensure Direction::from_degrees maps compass degrees to Direction variants.",
        directions = "Call `enums::c_style::Direction::from_degrees` through the generated binding and assert Direction::from_degrees maps compass degrees to Direction variants."
    )]
    pub fn from_degrees(degrees: f64) -> Self {
        let normalized = ((degrees % 360.0) + 360.0) % 360.0;
        if normalized < 45.0 || normalized >= 315.0 {
            Direction::North
        } else if normalized < 135.0 {
            Direction::East
        } else if normalized < 225.0 {
            Direction::South
        } else {
            Direction::West
        }
    }

    #[demo_bench_macros::demo_case(
        "enums.c_style.direction.should_return_opposite_from_method",
        justification = "Ensure Direction::opposite returns the opposite compass direction.",
        directions = "Call `enums::c_style::Direction::opposite` through the generated binding and assert Direction::opposite returns the opposite compass direction."
    )]
    pub fn opposite(&self) -> Direction {
        match self {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::East => Direction::West,
            Direction::West => Direction::East,
        }
    }

    #[demo_bench_macros::demo_case(
        "enums.c_style.direction.should_identify_horizontal_values",
        justification = "Ensure Direction::is_horizontal returns true for East and West.",
        directions = "Call `enums::c_style::Direction::is_horizontal` through the generated binding and assert Direction::is_horizontal returns true for East and West."
    )]
    pub fn is_horizontal(&self) -> bool {
        matches!(self, Direction::East | Direction::West)
    }

    #[demo_bench_macros::demo_case(
        "enums.c_style.direction.should_render_compass_label",
        justification = "Ensure Direction::label returns the single-letter compass label.",
        directions = "Call `enums::c_style::Direction::label` through the generated binding and assert Direction::label returns the single-letter compass label."
    )]
    pub fn label(&self) -> String {
        match self {
            Direction::North => "N".to_string(),
            Direction::South => "S".to_string(),
            Direction::East => "E".to_string(),
            Direction::West => "W".to_string(),
        }
    }

    #[demo_bench_macros::demo_case(
        "enums.c_style.direction.should_report_variant_count",
        justification = "Ensure Direction::count returns the number of Direction variants.",
        directions = "Call `enums::c_style::Direction::count` through the generated binding and assert Direction::count returns the number of Direction variants."
    )]
    pub fn count() -> u32 {
        4
    }
}

#[demo_bench_macros::demo_case(
    "enums.c_style.direction.should_roundtrip_value",
    justification = "Ensure a Direction enum value crosses the FFI boundary and returns unchanged.",
    directions = "Call `enums::c_style::echo_direction` through the generated binding and assert a Direction enum value crosses the FFI boundary and returns unchanged."
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn echo_direction(d: Direction) -> Direction {
    d
}

#[demo_bench_macros::demo_case(
    "enums.c_style.direction.should_return_opposite_from_free_function",
    justification = "Ensure opposite_direction returns the opposite compass direction for a Direction argument.",
    directions = "Call `enums::c_style::opposite_direction` through the generated binding and assert opposite_direction returns the opposite compass direction for a Direction argument."
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn opposite_direction(d: Direction) -> Direction {
    match d {
        Direction::North => Direction::South,
        Direction::South => Direction::North,
        Direction::East => Direction::West,
        Direction::West => Direction::East,
    }
}

#[demo_bench_macros::demo_case(
    "enums.c_style.direction.should_return_degrees",
    justification = "Ensure direction_to_degrees maps Direction variants to compass degrees.",
    directions = "Call `enums::c_style::direction_to_degrees` through the generated binding and assert direction_to_degrees maps Direction variants to compass degrees.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for direction_to_degrees in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "Python supports C-style enum parameters and primitive returns, but the demo suite has no assertion for direction_to_degrees yet."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript has no assertion for direction_to_degrees in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn direction_to_degrees(direction: Direction) -> i32 {
    match direction {
        Direction::North => 0,
        Direction::East => 90,
        Direction::South => 180,
        Direction::West => 270,
    }
}

#[demo_bench_macros::demo_case(
    "enums.c_style.direction.should_generate_sequence",
    justification = "Ensure generate_directions returns a cyclic sequence of Direction values.",
    directions = "Call `enums::c_style::generate_directions` through the generated binding and assert generate_directions returns a cyclic sequence of Direction values.",
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "Python supports C-style enum vectors, but the demo suite has no assertion for direction sequence helpers yet."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript has no assertion for direction sequence helpers in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn generate_directions(count: i32) -> Vec<Direction> {
    let directions = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];
    (0..count as usize)
        .map(|index| directions[index % directions.len()])
        .collect()
}

#[demo_bench_macros::demo_case(
    "enums.c_style.direction.should_count_north_values",
    justification = "Ensure count_north returns the number of North variants in a Direction vector.",
    directions = "Call `enums::c_style::count_north` through the generated binding and assert count_north returns the number of North variants in a Direction vector.",
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "Python supports C-style enum vectors, but the demo suite has no assertion for direction sequence helpers yet."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript has no assertion for direction sequence helpers in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn count_north(directions: Vec<Direction>) -> i32 {
    directions
        .iter()
        .filter(|direction| matches!(direction, Direction::North))
        .count() as i32
}

#[demo_bench_macros::demo_case(
    "enums.c_style.direction.find_direction.should_return_some_for_known_id",
    justification = "Ensure find_direction returns Some(Direction) for a known id.",
    directions = "Call `enums::c_style::find_direction` through the generated binding and assert find_direction returns Some(Direction) for a known id.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for find_direction in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; C-style enums are emitted, but Option<Direction> returns are not. Include this case when optional enum returns are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript has no assertion for find_direction in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.c_style.direction.find_direction.should_return_none_for_unknown_id",
    justification = "Ensure find_direction returns None for an unknown id.",
    directions = "Call `enums::c_style::find_direction` through the generated binding and assert find_direction returns None for an unknown id.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for find_direction in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; C-style enums are emitted, but Option<Direction> returns are not. Include this case when optional enum returns are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript has no assertion for find_direction in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn find_direction(id: i32) -> Option<Direction> {
    match id {
        0 => Some(Direction::North),
        1 => Some(Direction::East),
        2 => Some(Direction::South),
        3 => Some(Direction::West),
        _ => None,
    }
}

#[demo_bench_macros::demo_case(
    "enums.c_style.direction.find_directions.should_return_sequence_for_positive_count",
    justification = "Ensure find_directions returns Some generated directions for a positive count.",
    directions = "Call `enums::c_style::find_directions` through the generated binding and assert find_directions returns Some generated directions for a positive count.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for find_directions in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; C-style enum vectors are emitted, but Option<Vec<Direction>> returns are not. Include this case when optional enum-vector returns are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript has no assertion for find_directions in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.c_style.direction.find_directions.should_return_none_for_non_positive_count",
    justification = "Ensure find_directions returns None for a non-positive count.",
    directions = "Call `enums::c_style::find_directions` through the generated binding and assert find_directions returns None for a non-positive count.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# has no assertion for find_directions in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; C-style enum vectors are emitted, but Option<Vec<Direction>> returns are not. Include this case when optional enum-vector returns are implemented for Python."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript has no assertion for find_directions in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi)]
pub fn find_directions(count: i32) -> Option<Vec<Direction>> {
    if count > 0 {
        Some(generate_directions(count))
    } else {
        None
    }
}
