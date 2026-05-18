use boltffi::*;

use crate::enums::data_enum::Shape;
use crate::enums::repr_int::Priority;
use crate::records::blittable::Point;

#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MixedRecordParameters {
    pub tags: Vec<String>,
    pub checkpoints: Vec<Point>,
    pub fallback_anchor: Option<Point>,
    pub max_retries: u32,
    pub preview_only: bool,
}

#[data]
#[derive(Clone, Debug, PartialEq)]
pub struct MixedRecord {
    pub name: String,
    pub anchor: Point,
    pub priority: Priority,
    pub shape: Shape,
    pub parameters: MixedRecordParameters,
}

impl MixedRecord {
    pub fn from_parts(
        name: String,
        anchor: Point,
        priority: Priority,
        shape: Shape,
        parameters: MixedRecordParameters,
    ) -> Self {
        Self {
            name,
            anchor,
            priority,
            shape,
            parameters,
        }
    }
}

#[export]
#[demo_bench_macros::demo_case(
    "records.mixed.should_roundtrip_composed_record",
    justification = "Ensure a MixedRecord composed from strings, records, enums, options, and vectors crosses the wire and returns unchanged.",
    directions = "Call `records::mixed::echo_mixed_record` through the generated binding and assert a MixedRecord composed from strings, records, enums, options, and vectors crosses the wire and returns unchanged.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when non-blittable records are implemented for Python."
    )
)]
pub fn echo_mixed_record(record: MixedRecord) -> MixedRecord {
    record
}

#[export]
#[demo_bench_macros::demo_case(
    "records.mixed.should_make_from_composed_parts",
    justification = "Ensure make_mixed_record constructs a MixedRecord from nested records, data enums, repr-int enums, options, and vectors.",
    directions = "Call `records::mixed::make_mixed_record` through the generated binding and assert make_mixed_record constructs a MixedRecord from nested records, data enums, repr-int enums, options, and vectors.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently emits only primitive-field blittable records. Include this case when non-blittable records are implemented for Python."
    )
)]
pub fn make_mixed_record(
    name: String,
    anchor: Point,
    priority: Priority,
    shape: Shape,
    parameters: MixedRecordParameters,
) -> MixedRecord {
    MixedRecord::from_parts(name, anchor, priority, shape, parameters)
}
