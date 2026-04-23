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
    pub fn new(radius: f64) -> Self {
        Shape::Circle { radius }
    }

    pub fn unit_circle() -> Self {
        Shape::Circle { radius: 1.0 }
    }

    pub fn square(side: f64) -> Self {
        Shape::Rectangle {
            width: side,
            height: side,
        }
    }

    pub fn try_circle(radius: f64) -> Result<Self, String> {
        if radius <= 0.0 {
            Err("radius must be positive".to_string())
        } else {
            Ok(Shape::Circle { radius })
        }
    }

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

    pub fn variant_count() -> u32 {
        6
    }

    /// Returns `Some(Point)` for a positive radius, `None` otherwise.
    /// Exercises a static method on a data enum whose return type is
    /// `Option<Record>` where the record type is shadowed by another
    /// variant on the same enum.
    pub fn try_apex_point(radius: f64) -> Option<Point> {
        if radius > 0.0 {
            Some(Point { x: 0.0, y: radius })
        } else {
            None
        }
    }
}

#[export]
pub fn echo_shape(s: Shape) -> Shape {
    s
}

#[export]
pub fn make_circle(radius: f64) -> Shape {
    Shape::Circle { radius }
}

#[export]
pub fn make_rectangle(width: f64, height: f64) -> Shape {
    Shape::Rectangle { width, height }
}

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

#[export]
pub fn echo_message(m: Message) -> Message {
    m
}

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

#[export]
pub fn echo_animal(a: Animal) -> Animal {
    a
}

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

#[export]
pub fn echo_lifecycle_event(ev: LifecycleEvent) -> LifecycleEvent {
    ev
}

#[export]
pub fn make_critical_lifecycle_event(id: i64) -> LifecycleEvent {
    LifecycleEvent::TaskStarted {
        priority: Priority::Critical,
        id,
    }
}
