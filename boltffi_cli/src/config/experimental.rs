use boltffi_bindgen::target::Target;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Experimental {
    WholeTarget(Target),
    Feature { target: Target, name: &'static str },
}

impl Experimental {
    pub const ALL: &'static [Experimental] = &[
        Experimental::Feature {
            target: Target::TypeScript,
            name: "async_streams",
        },
        Experimental::WholeTarget(Target::Dart),
        Experimental::WholeTarget(Target::DartWeb),
        Experimental::WholeTarget(Target::KotlinMultiplatform),
    ];

    pub fn is_target_experimental(target: Target) -> bool {
        Self::ALL.iter().any(
            |experimental| matches!(experimental, Experimental::WholeTarget(t) if *t == target),
        )
    }
}
