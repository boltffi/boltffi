use boltffi::*;

/// A traffic light whose state is a plain Rust enum behind the class handle.
#[export]
pub enum TrafficLight {
    Red,
    Green { seconds: u32 },
}

#[export]
impl TrafficLight {
    pub fn new() -> Self {
        Self::Red
    }

    #[demo_bench_macros::demo_case(
        "classes.enum_backed.traffic_light.seconds_left.should_match_on_the_variant_behind_the_handle",
        justification = "Ensure an `#[export]` enum declares a class whose methods see the variant the handle holds.",
        directions = "Construct `classes::enum_backed::TrafficLight` through the generated binding, call `seconds_left` on it and on the light `turn_green` returns, and assert 0 and the given seconds.",
        exclude(
            ruby,
            reason = ExclusionReason::ImplementationGap,
            details = "the Ruby target does not support classes yet"
        )
    )]
    pub fn seconds_left(&self) -> u32 {
        match self {
            Self::Red => 0,
            Self::Green { seconds } => *seconds,
        }
    }

    pub fn turn_green(&self, seconds: u32) -> TrafficLight {
        Self::Green { seconds }
    }
}
