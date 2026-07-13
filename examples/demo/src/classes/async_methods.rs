use boltffi::*;

pub struct AsyncWorker {
    prefix: String,
}

#[export]
impl AsyncWorker {
    pub fn new(prefix: String) -> Self {
        Self { prefix }
    }

    pub fn get_prefix(&self) -> String {
        self.prefix.clone()
    }

    pub async fn process(&self, input: String) -> String {
        format!("{}: {}", self.prefix, input)
    }

    pub async fn try_process(&self, input: String) -> Result<String, String> {
        if input.is_empty() {
            Err("input must not be empty".to_string())
        } else {
            Ok(format!("{}: {}", self.prefix, input))
        }
    }

    pub async fn find_item(&self, id: i32) -> Option<String> {
        if id > 0 {
            Some(format!("{}_{}", self.prefix, id))
        } else {
            None
        }
    }

    pub async fn process_batch(&self, inputs: Vec<String>) -> Vec<String> {
        inputs
            .into_iter()
            .map(|input| format!("{}: {}", self.prefix, input))
            .collect()
    }
}

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
