use boltffi::*;

use crate::enums::data_enum::Shape;
use crate::enums::repr_int::{LogLevel, Priority};

#[data]
#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub title: String,
    pub priority: Priority,
    pub completed: bool,
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_enums.task.should_roundtrip_priority_field",
    justification = "Ensure a Task record with a Priority enum field crosses the wire and returns unchanged.",
    directions = "Call `records::with_enums::echo_task` through the generated binding and assert a Task record with a Priority enum field crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with enum fields are implemented for Python."
    )
)]
pub fn echo_task(task: Task) -> Task {
    task
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_enums.task.should_make_incomplete_task",
    justification = "Ensure make_task constructs a Task with the requested Priority enum and completed set to false.",
    directions = "Call `records::with_enums::make_task` through the generated binding and assert make_task constructs a Task with the requested Priority enum and completed set to false.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with enum fields are implemented for Python."
    )
)]
pub fn make_task(title: String, priority: Priority) -> Task {
    Task {
        title,
        priority,
        completed: false,
    }
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_enums.task.should_detect_urgent_priority",
    justification = "Ensure is_urgent reads the Priority enum field from a Task record and classifies urgent priorities.",
    directions = "Call `records::with_enums::is_urgent` through the generated binding and assert is_urgent reads the Priority enum field from a Task record and classifies urgent priorities.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with enum fields are implemented for Python."
    )
)]
pub fn is_urgent(task: Task) -> bool {
    matches!(task.priority, Priority::High | Priority::Critical)
}

#[data]
#[derive(Clone, Debug, PartialEq)]
pub struct Notification {
    pub message: String,
    pub priority: Priority,
    pub read: bool,
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_enums.notification.should_roundtrip_priority_field",
    justification = "Ensure a Notification record with a Priority enum field crosses the wire and returns unchanged.",
    directions = "Call `records::with_enums::echo_notification` through the generated binding and assert a Notification record with a Priority enum field crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with enum fields are implemented for Python."
    )
)]
pub fn echo_notification(notification: Notification) -> Notification {
    notification
}

/// A `#[repr(C)]` wrapper around a data enum field.
///
/// Data enums have a variable-width on-the-wire representation — a
/// discriminant tag followed by the active variant's payload. A
/// record embedding one cannot be laid out as a flat C struct and
/// marshalled direct; it must ride the wire codec end to end.
///
/// This ensures that even when the host struct wears `#[repr(C)]`,
/// backends that have a "blittable if all fields are primitive"
/// fast path don't incorrectly admit a data-enum field into that
/// fast path.
#[data]
#[repr(C)]
#[derive(Clone, Debug, PartialEq)]
pub struct Holder {
    pub shape: Shape,
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_enums.holder.should_roundtrip_data_enum_field",
    justification = "Ensure a Holder record containing a Shape data enum crosses the wire and returns unchanged.",
    directions = "Call `records::with_enums::echo_holder` through the generated binding and assert a Holder record containing a Shape data enum crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with enum fields are implemented for Python."
    )
)]
pub fn echo_holder(h: Holder) -> Holder {
    h
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_enums.holder.should_make_triangle_variant",
    justification = "Ensure make_triangle_holder constructs a Holder whose Shape field is the Triangle data enum variant.",
    directions = "Call `records::with_enums::make_triangle_holder` through the generated binding and assert make_triangle_holder constructs a Holder whose Shape field is the Triangle data enum variant.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with enum fields are implemented for Python."
    )
)]
pub fn make_triangle_holder() -> Holder {
    Holder {
        shape: Shape::Triangle {
            a: crate::records::blittable::Point { x: 0.0, y: 0.0 },
            b: crate::records::blittable::Point { x: 4.0, y: 0.0 },
            c: crate::records::blittable::Point { x: 0.0, y: 3.0 },
        },
    }
}

/// A compact header whose every field is a primitive or a C-style enum.
///
/// Rides the wire codec today because the `#[export]` macro's
/// blittability check admits only literal primitive fields, so a
/// `Priority` field (a C-style enum, same bit layout as its backing
/// `i32` but not a literal primitive from the macro's point of view)
/// bumps the struct onto the encoded path. A future change coordinating
/// the macro and each backend's blittable classifier can promote this
/// shape to direct P/Invoke with zero encode/decode cost.
#[data]
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TaskHeader {
    pub id: i64,
    pub priority: Priority,
    pub completed: bool,
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_enums.task_header.should_roundtrip_repr_enum_field",
    justification = "Ensure a TaskHeader record with a repr-int Priority enum field crosses the wire and returns unchanged.",
    directions = "Call `records::with_enums::echo_task_header` through the generated binding and assert a TaskHeader record with a repr-int Priority enum field crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with enum fields are implemented for Python."
    )
)]
pub fn echo_task_header(header: TaskHeader) -> TaskHeader {
    header
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_enums.task_header.should_make_critical_header",
    justification = "Ensure make_critical_task_header constructs a TaskHeader with Critical priority and completed set to false.",
    directions = "Call `records::with_enums::make_critical_task_header` through the generated binding and assert make_critical_task_header constructs a TaskHeader with Critical priority and completed set to false.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with enum fields are implemented for Python."
    )
)]
pub fn make_critical_task_header(id: i64) -> TaskHeader {
    TaskHeader {
        id,
        priority: Priority::Critical,
        completed: false,
    }
}

/// A `#[repr(C)]` struct mixing a `u8`-backed C-style enum with wider
/// primitives — same family as `TaskHeader`, but the enum's backing
/// type forces non-trivial alignment padding between fields.
///
/// Rides the wire codec today for the same reason `TaskHeader` does
/// (see its doc): the `#[export]` macro won't admit a C-style enum
/// field as a layout-compatible primitive. When that changes, this
/// struct is a useful shape to verify: padding between the `u8` enum
/// and the `u16` / `i64` fields has to line up on both sides of the
/// boundary, and non-`i32` enum backing types have historically been
/// the first place a new blittable path breaks.
#[data]
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogEntry {
    pub timestamp: i64,
    pub level: LogLevel,
    pub code: u16,
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_enums.log_entry.should_roundtrip_u8_enum_field",
    justification = "Ensure a LogEntry record with a u8-backed LogLevel enum field crosses the wire and returns unchanged.",
    directions = "Call `records::with_enums::echo_log_entry` through the generated binding and assert a LogEntry record with a u8-backed LogLevel enum field crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with enum fields are implemented for Python."
    )
)]
pub fn echo_log_entry(entry: LogEntry) -> LogEntry {
    entry
}

#[export]
#[demo_bench_macros::demo_case(
    "records.with_enums.log_entry.should_make_error_entry",
    justification = "Ensure make_error_log_entry constructs a LogEntry with an Error log level and caller-provided fields.",
    directions = "Call `records::with_enums::make_error_log_entry` through the generated binding and assert make_error_log_entry constructs a LogEntry with an Error log level and caller-provided fields.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when records with enum fields are implemented for Python."
    )
)]
pub fn make_error_log_entry(timestamp: i64, code: u16) -> LogEntry {
    LogEntry {
        timestamp,
        level: LogLevel::Error,
        code,
    }
}
