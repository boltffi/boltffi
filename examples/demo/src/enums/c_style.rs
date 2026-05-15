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
    description = "Status enum values cross the FFI boundary and return unchanged."
)]
#[export]
pub fn echo_status(s: Status) -> Status {
    s
}

#[demo_bench_macros::demo_case(
    "enums.c_style.status.should_render_labels",
    description = "status_to_string maps Status enum values to their string labels."
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
    description = "is_active returns true only for the active Status variant."
)]
#[export]
pub fn is_active(s: Status) -> bool {
    matches!(s, Status::Active)
}

#[demo_bench_macros::demo_case(
    "enums.c_style.status.should_roundtrip_vectors",
    description = "A vector of Status enum values preserves variant order and values."
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
        description = "Direction::new maps raw integer values to Direction variants.",
        exclude(
            java,
            reason = "The Java demo tests do not currently cover Direction::new."
        )
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
        description = "Direction::cardinal returns the North direction variant."
    )]
    pub fn cardinal() -> Self {
        Direction::North
    }

    #[demo_bench_macros::demo_case(
        "enums.c_style.direction.should_construct_from_degrees",
        description = "Direction::from_degrees maps compass degrees to Direction variants."
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
        description = "Direction::opposite returns the opposite compass direction."
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
        description = "Direction::is_horizontal returns true for East and West."
    )]
    pub fn is_horizontal(&self) -> bool {
        matches!(self, Direction::East | Direction::West)
    }

    #[demo_bench_macros::demo_case(
        "enums.c_style.direction.should_render_compass_label",
        description = "Direction::label returns the single-letter compass label."
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
        description = "Direction::count returns the number of Direction variants."
    )]
    pub fn count() -> u32 {
        4
    }
}

#[demo_bench_macros::demo_case(
    "enums.c_style.direction.should_roundtrip_value",
    description = "A Direction enum value crosses the FFI boundary and returns unchanged."
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn echo_direction(d: Direction) -> Direction {
    d
}

#[demo_bench_macros::demo_case(
    "enums.c_style.direction.should_return_opposite_from_free_function",
    description = "opposite_direction returns the opposite compass direction for a Direction argument."
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
    description = "direction_to_degrees maps Direction variants to compass degrees.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover direction_to_degrees."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover direction_to_degrees."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover direction_to_degrees."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover direction_to_degrees."
    ),
    exclude(
        wasm,
        reason = "The WASM demo tests do not currently cover direction_to_degrees."
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
    description = "generate_directions returns a cyclic sequence of Direction values.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover direction sequence helpers."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover direction sequence helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover direction sequence helpers."
    ),
    exclude(
        wasm,
        reason = "The WASM demo tests do not currently cover direction sequence helpers."
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
    description = "count_north returns the number of North variants in a Direction vector.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover direction sequence helpers."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover direction sequence helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover direction sequence helpers."
    ),
    exclude(
        wasm,
        reason = "The WASM demo tests do not currently cover direction sequence helpers."
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
    "enums.c_style.direction.should_find_by_id",
    description = "find_direction returns Some(Direction) for known ids and None for unknown ids.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover find_direction."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover find_direction."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover find_direction."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover find_direction."
    ),
    exclude(
        wasm,
        reason = "The WASM demo tests do not currently cover find_direction."
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
    "enums.c_style.direction.should_find_sequence_by_count",
    description = "find_directions returns Some generated directions for positive counts and None otherwise.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover find_directions."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover find_directions."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin demo tests do not currently cover find_directions."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover find_directions."
    ),
    exclude(
        wasm,
        reason = "The WASM demo tests do not currently cover find_directions."
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
