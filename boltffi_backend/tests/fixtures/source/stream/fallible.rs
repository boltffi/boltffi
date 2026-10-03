use boltffi::EventSubscription;
use std::sync::Arc;

#[error]
pub enum JobError {
    Refused { code: u32 },
    Lost,
}

pub struct Jobs;

#[export(single_threaded)]
impl Jobs {
    #[ffi_stream(item = String, error = String)]
    pub fn lines(&self) -> Arc<EventSubscription<String, String>> {
        loop {}
    }

    #[ffi_stream(item = i32, error = JobError, mode = "batch")]
    pub fn progress(&self) -> Arc<EventSubscription<i32, JobError>> {
        loop {}
    }

    #[ffi_stream(item = i32, error = String, mode = "callback")]
    pub fn ticks(&self) -> Arc<EventSubscription<i32, String>> {
        loop {}
    }
}
