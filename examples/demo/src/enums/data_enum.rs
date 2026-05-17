use boltffi::*;
use demo_bench_macros::benchmark_candidate;

use crate::enums::repr_int::Priority;
use crate::records::blittable::Point;

/// A geometric shape where each variant carries different data.
#[data]
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Circle {
        radius: f64,
    },
    Rectangle {
        width: f64,
        height: f64,
    },
    /// Triangle defined by three vertices.
    Triangle {
        a: Point,
        b: Point,
        c: Point,
    },
    Point,
    /// Optional tip vertex. Exercises `Option<Record>` as a variant
    /// field where the record type is also the name of another
    /// variant on the same enum.
    Apex {
        tip: Option<Point>,
    },
    /// Exercises `Vec<Record>` as a variant field where the record
    /// type is also the name of another variant on the same enum.
    Cluster {
        members: Vec<Point>,
    },
}

#[data(impl)]
impl Shape {
    #[demo_bench_macros::demo_case(
        "enums.data_enum.shape.should_support_primary_constructor",
        justification = "Ensure the generated Shape primary constructor builds a Circle variant with the requested radius.",
        directions = "Call `enums::data_enum::Shape::new` through the generated binding and assert the generated Shape primary constructor builds a Circle variant with the requested radius.",
        exclude(
            java,
            reason = ExclusionReason::ImplementationGap,
            details = "Java cannot expose Shape::new on the generated surface because new is a Java keyword."
        ),
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
        )
    )]
    pub fn new(radius: f64) -> Self {
        Shape::Circle { radius }
    }

    #[demo_bench_macros::demo_case(
        "enums.data_enum.shape.unit_circle.should_construct_circle",
        justification = "Ensure Shape::unit_circle constructs a Circle variant with unit radius.",
        directions = "Call `enums::data_enum::Shape::unit_circle` through the generated binding and assert Shape::unit_circle constructs a Circle variant with unit radius.",
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
        )
    )]
    pub fn unit_circle() -> Self {
        Shape::Circle { radius: 1.0 }
    }

    #[demo_bench_macros::demo_case(
        "enums.data_enum.shape.square.should_construct_rectangle",
        justification = "Ensure Shape::square constructs a Rectangle variant whose width and height match the side length.",
        directions = "Call `enums::data_enum::Shape::square` through the generated binding and assert Shape::square constructs a Rectangle variant whose width and height match the side length.",
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
        )
    )]
    pub fn square(side: f64) -> Self {
        Shape::Rectangle {
            width: side,
            height: side,
        }
    }

    #[demo_bench_macros::demo_case(
        "enums.data_enum.shape.try_circle.should_return_circle_for_positive_radius",
        justification = "Ensure Shape::try_circle returns a Circle variant for a positive radius.",
        directions = "Call `enums::data_enum::Shape::try_circle` through the generated binding and assert Shape::try_circle returns a Circle variant for a positive radius.",
        exclude(
            csharp,
            reason = ExclusionReason::ImplementationGap,
            details = "C# does not currently emit Result-returning data-enum static methods, so Shape::try_circle is absent from the generated surface. Include this case when fallible data-enum factories are implemented for C#."
        ),
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
        )
    )]
    #[demo_bench_macros::demo_case(
        "enums.data_enum.shape.should_reject_non_positive_circle_radius",
        justification = "Ensure Shape::try_circle returns a language-native error when radius is zero or negative.",
        directions = "Call `enums::data_enum::Shape::try_circle` through the generated binding and assert Shape::try_circle returns a language-native error when radius is zero or negative.",
        exclude(
            csharp,
            reason = ExclusionReason::ImplementationGap,
            details = "C# does not currently emit Result-returning data-enum static methods, so Shape::try_circle is absent from the generated surface. Include this case when fallible data-enum factories are implemented for C#."
        ),
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
        )
    )]
    pub fn try_circle(radius: f64) -> Result<Self, String> {
        if radius <= 0.0 {
            Err("radius must be positive".to_string())
        } else {
            Ok(Shape::Circle { radius })
        }
    }

    #[demo_bench_macros::demo_case(
        "enums.data_enum.shape.should_support_numeric_instance_methods",
        justification = "Ensure Shape instance methods can wire-encode the receiver and return numeric results.",
        directions = "Call `enums::data_enum::Shape::area` through the generated binding and assert Shape instance methods can wire-encode the receiver and return numeric results.",
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
        )
    )]
    pub fn area(&self) -> f64 {
        match self {
            Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Triangle { a, b, c } => {
                ((a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y)) / 2.0).abs()
            }
            Shape::Point => 0.0,
            Shape::Apex { .. } => 0.0,
            Shape::Cluster { .. } => 0.0,
        }
    }

    #[demo_bench_macros::demo_case(
        "enums.data_enum.shape.should_support_string_instance_methods",
        justification = "Ensure Shape instance methods can wire-encode the receiver and return string results.",
        directions = "Call `enums::data_enum::Shape::describe` through the generated binding and assert Shape instance methods can wire-encode the receiver and return string results.",
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
        )
    )]
    pub fn describe(&self) -> String {
        match self {
            Shape::Circle { radius } => format!("circle r={}", radius),
            Shape::Rectangle { width, height } => format!("rect {}x{}", width, height),
            Shape::Triangle { .. } => "triangle".to_string(),
            Shape::Point => "point".to_string(),
            Shape::Apex { tip: Some(p) } => format!("apex at ({}, {})", p.x, p.y),
            Shape::Apex { tip: None } => "apex (no tip)".to_string(),
            Shape::Cluster { members } => format!("cluster of {}", members.len()),
        }
    }

    #[demo_bench_macros::demo_case(
        "enums.data_enum.shape.should_report_variant_count",
        justification = "Ensure the generated Shape static method can return primitive metadata about the data enum.",
        directions = "Call `enums::data_enum::Shape::variant_count` through the generated binding and assert the generated Shape static method can return primitive metadata about the data enum.",
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
        )
    )]
    pub fn variant_count() -> u32 {
        6
    }

    /// Returns `Some(Point)` for a positive radius, `None` otherwise.
    /// Exercises a static method on a data enum whose return type is
    /// `Option<Record>` where the record type is shadowed by another
    /// variant on the same enum.
    #[demo_bench_macros::demo_case(
        "enums.data_enum.shape.try_apex_point.should_return_some_for_positive_radius",
        justification = "Ensure Shape::try_apex_point returns Some(Point) for a positive radius while resolving Point as the record type.",
        directions = "Call `enums::data_enum::Shape::try_apex_point` through the generated binding and assert Shape::try_apex_point returns Some(Point) for a positive radius while resolving Point as the record type.",
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
        )
    )]
    #[demo_bench_macros::demo_case(
        "enums.data_enum.shape.try_apex_point.should_return_none_for_non_positive_radius",
        justification = "Ensure Shape::try_apex_point returns None for a non-positive radius while resolving Point as the record type.",
        directions = "Call `enums::data_enum::Shape::try_apex_point` through the generated binding and assert Shape::try_apex_point returns None for a non-positive radius while resolving Point as the record type.",
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
        )
    )]
    pub fn try_apex_point(radius: f64) -> Option<Point> {
        if radius > 0.0 {
            Some(Point { x: 0.0, y: radius })
        } else {
            None
        }
    }
}

#[demo_bench_macros::demo_case(
    "enums.data_enum.shape.should_roundtrip_core_variants",
    justification = "Ensure Circle, Rectangle, Triangle, and Point Shape variants preserve their tags and payload fields.",
    directions = "Call `enums::data_enum::echo_shape` through the generated binding and assert Circle, Rectangle, Triangle, and Point Shape variants preserve their tags and payload fields.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.data_enum.shape.apex.should_roundtrip_some_point_payload",
    justification = "Ensure the Shape::Apex variant preserves a Some(Point) payload when Point is also a sibling variant name.",
    directions = "Call `enums::data_enum::echo_shape` through the generated binding and assert the Shape::Apex variant preserves a Some(Point) payload when Point is also a sibling variant name.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.data_enum.shape.apex.should_roundtrip_none_payload",
    justification = "Ensure the Shape::Apex variant preserves a None payload when Point is also a sibling variant name.",
    directions = "Call `enums::data_enum::echo_shape` through the generated binding and assert the Shape::Apex variant preserves a None payload when Point is also a sibling variant name.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.data_enum.shape.should_roundtrip_vector_record_fields",
    justification = "Ensure a Shape variant can carry a vector of Point records even when Point is also a sibling variant name.",
    directions = "Call `enums::data_enum::echo_shape` through the generated binding and assert a Shape variant can carry a vector of Point records even when Point is also a sibling variant name.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[export]
pub fn echo_shape(s: Shape) -> Shape {
    s
}

#[demo_bench_macros::demo_case(
    "enums.data_enum.shape.should_support_free_function_factories",
    justification = "Ensure free functions construct Circle and Rectangle Shape variants with the requested primitive fields.",
    directions = "Call `enums::data_enum::make_circle` and `enums::data_enum::make_rectangle` through the generated bindings and assert free functions construct Circle and Rectangle Shape variants with the requested primitive fields.",
    exercises = [
        "enums::data_enum::make_circle",
        "enums::data_enum::make_rectangle"
    ],
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[export]
pub fn make_circle(radius: f64) -> Shape {
    Shape::Circle { radius }
}

#[export]
pub fn make_rectangle(width: f64, height: f64) -> Shape {
    Shape::Rectangle { width, height }
}

#[demo_bench_macros::demo_case(
    "enums.data_enum.shape.should_roundtrip_vectors",
    justification = "Ensure a vector of data-enum Shape values preserves variant order and payloads.",
    directions = "Call `enums::data_enum::echo_vec_shape` through the generated binding and assert a vector of data-enum Shape values preserves variant order and payloads.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[export]
pub fn echo_vec_shape(values: Vec<Shape>) -> Vec<Shape> {
    values
}

#[data]
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Text {
        body: String,
    },
    Image {
        url: String,
        width: u32,
        height: u32,
    },
    Ping,
}

#[demo_bench_macros::demo_case(
    "enums.data_enum.message.text.should_roundtrip_string_payload",
    justification = "Ensure the Message::Text variant preserves its string payload when round-tripped.",
    directions = "Call `enums::data_enum::echo_message` through the generated binding and assert the Message::Text variant preserves its string payload when round-tripped.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.data_enum.message.image.should_roundtrip_url_dimensions_payload",
    justification = "Ensure the Message::Image variant preserves its URL and dimension payload fields when round-tripped.",
    directions = "Call `enums::data_enum::echo_message` through the generated binding and assert the Message::Image variant preserves its URL and dimension payload fields when round-tripped.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java reaches the surrounding surface but still needs a round-trip assertion for the Message::Image variant."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.data_enum.message.ping.should_roundtrip_unit_variant",
    justification = "Ensure the Message::Ping unit variant crosses the FFI boundary unchanged.",
    directions = "Call `enums::data_enum::echo_message` through the generated binding and assert the Message::Ping unit variant crosses the FFI boundary unchanged.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java reaches the surrounding surface but still needs a round-trip assertion for the Message::Ping variant."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin reaches the surrounding surface but still needs a round-trip assertion for the Message::Ping variant."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript reaches the surrounding surface but still needs a round-trip assertion for the Message::Ping variant."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[export]
pub fn echo_message(m: Message) -> Message {
    m
}

#[demo_bench_macros::demo_case(
    "enums.data_enum.message.text.should_render_text_summary",
    justification = "Ensure message_summary renders the Text string payload in the summary.",
    directions = "Call `enums::data_enum::message_summary` through the generated binding and assert message_summary renders the Text string payload in the summary.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.data_enum.message.image.should_render_image_summary",
    justification = "Ensure message_summary renders the Image dimensions and URL in the summary.",
    directions = "Call `enums::data_enum::message_summary` through the generated binding and assert message_summary renders the Image dimensions and URL in the summary.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# round-trips the value but still needs an assertion for the the Message::Image variant summary."
    ),
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java round-trips the value but still needs an assertion for the the Message::Image variant summary."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.data_enum.message.ping.should_render_ping_summary",
    justification = "Ensure message_summary renders the Ping unit variant summary.",
    directions = "Call `enums::data_enum::message_summary` through the generated binding and assert message_summary renders the Ping unit variant summary.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[export]
pub fn message_summary(m: Message) -> String {
    match m {
        Message::Text { body } => format!("text: {}", body),
        Message::Image { url, width, height } => format!("image: {}x{} at {}", width, height, url),
        Message::Ping => "ping".to_string(),
    }
}

#[data]
#[derive(Clone, Debug, PartialEq)]
pub enum Animal {
    Dog { name: String, breed: String },
    Cat { name: String, indoor: bool },
    Fish { count: u32 },
}

#[demo_bench_macros::demo_case(
    "enums.data_enum.animal.dog.should_roundtrip_string_payloads",
    justification = "Ensure the Animal::Dog variant preserves its name and breed string payloads when round-tripped.",
    directions = "Call `enums::data_enum::echo_animal` through the generated binding and assert the Animal::Dog variant preserves its name and breed string payloads when round-tripped.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.data_enum.animal.cat.should_roundtrip_name_and_bool_payload",
    justification = "Ensure the Animal::Cat variant preserves its name string and indoor boolean payloads when round-tripped.",
    directions = "Call `enums::data_enum::echo_animal` through the generated binding and assert the Animal::Cat variant preserves its name string and indoor boolean payloads when round-tripped.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java reaches the surrounding surface but still needs a round-trip assertion for the Animal::Cat variant."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.data_enum.animal.fish.should_roundtrip_count_payload",
    justification = "Ensure the Animal::Fish variant preserves its count payload when round-tripped.",
    directions = "Call `enums::data_enum::echo_animal` through the generated binding and assert the Animal::Fish variant preserves its count payload when round-tripped.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java reaches the surrounding surface but still needs a round-trip assertion for the Animal::Fish variant."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin reaches the surrounding surface but still needs a round-trip assertion for the Animal::Fish variant."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript reaches the surrounding surface but still needs a round-trip assertion for the Animal::Fish variant."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[export]
pub fn echo_animal(a: Animal) -> Animal {
    a
}

#[demo_bench_macros::demo_case(
    "enums.data_enum.animal.dog.should_derive_name",
    justification = "Ensure animal_name derives the dog name from an Animal::Dog payload.",
    directions = "Call `enums::data_enum::animal_name` through the generated binding and assert animal_name derives the dog name from an Animal::Dog payload.",
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java round-trips the enum family but still needs an assertion for the derived a name from Animal::Dog behavior."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "Kotlin round-trips the enum family but still needs an assertion for the derived a name from Animal::Dog behavior."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript round-trips the enum family but still needs an assertion for the derived a name from Animal::Dog behavior."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.data_enum.animal.cat.should_derive_name",
    justification = "Ensure animal_name derives the cat name from an Animal::Cat payload.",
    directions = "Call `enums::data_enum::animal_name` through the generated binding and assert animal_name derives the cat name from an Animal::Cat payload.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# round-trips the enum family but still needs an assertion for the derived a name from Animal::Cat behavior."
    ),
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "Java round-trips the enum family but still needs an assertion for the derived a name from Animal::Cat behavior."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.data_enum.animal.fish.should_derive_count_label",
    justification = "Ensure animal_name derives a count label from an Animal::Fish payload.",
    directions = "Call `enums::data_enum::animal_name` through the generated binding and assert animal_name derives a count label from an Animal::Fish payload.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[export]
pub fn animal_name(a: Animal) -> String {
    match a {
        Animal::Dog { name, .. } | Animal::Cat { name, .. } => name,
        Animal::Fish { count } => format!("{} fish", count),
    }
}

#[benchmark_candidate(enum, uniffi)]
#[data]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TaskStatus {
    Pending,
    InProgress { progress: i32 },
    Completed { result: i32 },
    Failed { error_code: i32, retry_count: i32 },
}

/// Returns the given data enum unchanged — measures the full wire
/// round-trip for a value with a variable-width payload. Paired with
/// `echo_direction` so benchmarks can price the wire-encoding overhead
/// against the direct-marshaling baseline.
#[export]
#[benchmark_candidate(function, uniffi)]
pub fn echo_task_status(status: TaskStatus) -> TaskStatus {
    status
}

#[export]
#[benchmark_candidate(function, uniffi)]
pub fn get_status_progress(status: TaskStatus) -> i32 {
    match status {
        TaskStatus::Pending => 0,
        TaskStatus::InProgress { progress } => progress,
        TaskStatus::Completed { result } => result,
        TaskStatus::Failed { error_code, .. } => error_code,
    }
}

#[export]
#[benchmark_candidate(function, uniffi)]
pub fn is_status_complete(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Completed { .. })
}

/// A lifecycle event with a C-style enum nested in a variant's payload.
///
/// Exercises the code path where a C-style enum surfaces inside a
/// data-enum variant's struct payload — distinct from a bare enum
/// parameter and from an enum field on a non-variant record. The
/// generated codec must encode the variant tag, then encode the
/// nested enum's backing integer, then decode them symmetrically on
/// the other side.
#[data]
#[derive(Clone, Debug, PartialEq)]
pub enum LifecycleEvent {
    TaskStarted { priority: Priority, id: i64 },
    Tick,
}

#[demo_bench_macros::demo_case(
    "enums.data_enum.lifecycle_event.should_roundtrip_priority_payload",
    justification = "Ensure a LifecycleEvent variant embeds a Priority enum payload and round-trips both the outer tag and inner enum value.",
    directions = "Call `enums::data_enum::echo_lifecycle_event` through the generated binding and assert a LifecycleEvent variant embeds a Priority enum payload and round-trips both the outer tag and inner enum value.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.data_enum.lifecycle_event.should_roundtrip_tick_variant",
    justification = "Ensure the LifecycleEvent::Tick unit variant crosses the FFI boundary unchanged.",
    directions = "Call `enums::data_enum::echo_lifecycle_event` through the generated binding and assert the LifecycleEvent::Tick unit variant crosses the FFI boundary unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[export]
pub fn echo_lifecycle_event(ev: LifecycleEvent) -> LifecycleEvent {
    ev
}

#[demo_bench_macros::demo_case(
    "enums.data_enum.lifecycle_event.should_make_critical_event",
    justification = "Ensure make_critical_lifecycle_event constructs a TaskStarted LifecycleEvent with Critical priority.",
    directions = "Call `enums::data_enum::make_critical_lifecycle_event` through the generated binding and assert make_critical_lifecycle_event constructs a TaskStarted LifecycleEvent with Critical priority.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only C-style enums, not data-enum payloads. Include this case when Python data-enum bindings are implemented."
    )
)]
#[export]
pub fn make_critical_lifecycle_event(id: i64) -> LifecycleEvent {
    LifecycleEvent::TaskStarted {
        priority: Priority::Critical,
        id,
    }
}
