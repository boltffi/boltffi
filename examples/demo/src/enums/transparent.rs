//! Data enums whose record payloads *are* the variants.
//!
//! `Point` is blittable and `Label` is not, so the two payload lanes are both
//! covered; `Point` is transparent in two enums at once, which is the case
//! that forces the tag onto the base rather than the payload.
//!
//! Behind the `transparent-demo` feature: only the Kotlin and Python backends
//! render transparent variants, and the others reject them at generate time,
//! so an ungated module here would break `pack` for the rest of the demo.

use boltffi::*;

use crate::records::blittable::Point;

#[data]
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub text: String,
}

#[data]
#[derive(Clone, Debug, PartialEq)]
pub enum Waypoint {
    Unset,
    #[boltffi::transparent]
    Point(Point),
    #[boltffi::transparent]
    Label(Label),
    Note(String),
}

/// Carries `Point` under a different tag than [`Waypoint`] does.
#[data]
#[derive(Clone, Debug, PartialEq)]
pub enum Anchor {
    #[boltffi::transparent]
    Point(Point),
    Origin,
}

#[export]
#[demo_bench_macros::demo_case(
    "enums.transparent.waypoint.should_roundtrip_a_blittable_payload",
    justification = "A blittable payload record is the variant itself, so it crosses the wire without a wrapper object.",
    directions = "Call `enums::transparent::echo_waypoint` with the Point payload directly, with no variant wrapper, and assert the returned value is an equal Point.",
    exclude(
        swift,
        reason = ExclusionReason::ImplementationGap,
        details = "The Swift backend renders a data enum as a Swift enum, whose cases a payload struct cannot be, so it rejects transparent variants at generate time."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "The Java backend rejects transparent variants at generate time."
    ),
    exclude(
        csharp,
        reason = ExclusionReason::ImplementationGap,
        details = "A C# payload is a readonly record struct, which cannot inherit the abstract record a data enum renders as, so the backend rejects transparent variants at generate time."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::ImplementationGap,
        details = "The TypeScript backend discriminates its union on a tag field, which a transparent payload does not carry, so it rejects transparent variants at generate time."
    ),
    exclude(
        dart,
        reason = ExclusionReason::ImplementationGap,
        details = "The Dart backend rejects transparent variants at generate time."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.transparent.waypoint.should_roundtrip_an_encoded_payload",
    justification = "A payload holding a string is transparent on the same terms as a blittable one, so both payload lanes read alike.",
    directions = "Call `enums::transparent::echo_waypoint` with the Label payload directly and assert the returned value is an equal Label.",
    exclude(
        swift,
        reason = ExclusionReason::ImplementationGap,
        details = "The Swift backend renders a data enum as a Swift enum, whose cases a payload struct cannot be, so it rejects transparent variants at generate time."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "The Java backend rejects transparent variants at generate time."
    ),
    exclude(
        csharp,
        reason = ExclusionReason::ImplementationGap,
        details = "A C# payload is a readonly record struct, which cannot inherit the abstract record a data enum renders as, so the backend rejects transparent variants at generate time."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::ImplementationGap,
        details = "The TypeScript backend discriminates its union on a tag field, which a transparent payload does not carry, so it rejects transparent variants at generate time."
    ),
    exclude(
        dart,
        reason = ExclusionReason::ImplementationGap,
        details = "The Dart backend rejects transparent variants at generate time."
    )
)]
#[demo_bench_macros::demo_case(
    "enums.transparent.waypoint.should_roundtrip_the_wrapped_and_unit_variants",
    justification = "A scalar payload and the unit variant keep their own classes, so mixing them with transparent variants in one enum has to keep working.",
    directions = "Call `enums::transparent::echo_waypoint` with the Note and Unset variants and assert each returns an equal value.",
    exclude(
        swift,
        reason = ExclusionReason::ImplementationGap,
        details = "The Swift backend renders a data enum as a Swift enum, whose cases a payload struct cannot be, so it rejects transparent variants at generate time."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "The Java backend rejects transparent variants at generate time."
    ),
    exclude(
        csharp,
        reason = ExclusionReason::ImplementationGap,
        details = "A C# payload is a readonly record struct, which cannot inherit the abstract record a data enum renders as, so the backend rejects transparent variants at generate time."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::ImplementationGap,
        details = "The TypeScript backend discriminates its union on a tag field, which a transparent payload does not carry, so it rejects transparent variants at generate time."
    ),
    exclude(
        dart,
        reason = ExclusionReason::ImplementationGap,
        details = "The Dart backend rejects transparent variants at generate time."
    )
)]
pub fn echo_waypoint(waypoint: Waypoint) -> Waypoint {
    waypoint
}

#[export]
#[demo_bench_macros::demo_case(
    "enums.transparent.anchor.should_carry_the_shared_payload_under_its_own_tag",
    justification = "One payload record is transparent in two enums, so the tag has to come from the enum the value crosses under, not from the payload.",
    directions = "Send the same Point payload through `enums::transparent::echo_waypoint` and `enums::transparent::echo_anchor` and assert both return an equal Point, and that the payload satisfies an is/isinstance check against both enum types.",
    exercises = ["enums::transparent::echo_anchor", "enums::transparent::echo_waypoint"],
    exclude(
        swift,
        reason = ExclusionReason::ImplementationGap,
        details = "The Swift backend renders a data enum as a Swift enum, whose cases a payload struct cannot be, so it rejects transparent variants at generate time."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "The Java backend rejects transparent variants at generate time."
    ),
    exclude(
        csharp,
        reason = ExclusionReason::ImplementationGap,
        details = "A C# payload is a readonly record struct, which cannot inherit the abstract record a data enum renders as, so the backend rejects transparent variants at generate time."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::ImplementationGap,
        details = "The TypeScript backend discriminates its union on a tag field, which a transparent payload does not carry, so it rejects transparent variants at generate time."
    ),
    exclude(
        dart,
        reason = ExclusionReason::ImplementationGap,
        details = "The Dart backend rejects transparent variants at generate time."
    )
)]
pub fn echo_anchor(anchor: Anchor) -> Anchor {
    anchor
}
