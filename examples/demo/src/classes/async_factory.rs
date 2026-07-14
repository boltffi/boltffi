use boltffi::export;

pub struct AsyncFactory {
    value: i32,
}

#[export]
impl AsyncFactory {
    #[demo_bench_macros::demo_case(
        "classes.async_methods.async_factory.new.should_construct_from_async_initializer",
        justification = "Ensure an async primary class initializer is exposed as an awaitable static factory instead of an invalid target-language constructor.",
        directions = "Call `classes::async_methods::AsyncFactory::new` through the generated binding, await the returned class handle, and assert its value method returns the initializer argument.",
        exclude(
            swift,
            reason = ExclusionReason::CoverageGap,
            details = "The Swift demo does not yet exercise async class initializers."
        ),
        exclude(
            kotlin,
            reason = ExclusionReason::CoverageGap,
            details = "The Kotlin demo does not yet exercise async class initializers."
        ),
        exclude(
            java,
            reason = ExclusionReason::CoverageGap,
            details = "The Java demo does not yet exercise async class initializers."
        ),
        exclude(
            typescript,
            reason = ExclusionReason::CoverageGap,
            details = "The TypeScript demo does not yet exercise async class initializers."
        ),
        exclude(
            python,
            reason = ExclusionReason::CoverageGap,
            details = "The Python demo does not yet exercise async class initializers."
        )
    )]
    pub async fn new(value: i32) -> Self {
        Self { value }
    }

    pub fn value(&self) -> i32 {
        self.value
    }
}
