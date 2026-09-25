#[error]
pub enum ServiceError {
    Failed { message: String },
    Optional { message: Option<String> },
    Detailed { code: u32, message: String },
    Unknown,
    Text(String),
}

#[error]
pub struct RecordError {
    pub message: String,
}

#[error]
pub struct OptionalRecordError {
    pub message: Option<String>,
}

#[error]
pub enum ShadowedError {
    String { message: String },
    Optional { message: Option<String> },
}

#[data]
pub enum Message {
    Plain { message: String, cause: String },
    Numeric { message: u32 },
}

#[data]
pub struct MessageRecord {
    pub message: String,
    pub cause: String,
}

#[export]
pub fn fail() -> Result<(), ServiceError> {
    Err(ServiceError::Failed {
        message: "failed".to_owned(),
    })
}

#[export]
pub fn fail_record() -> Result<(), RecordError> {
    Err(RecordError {
        message: "failed".to_owned(),
    })
}

#[export]
pub fn fail_optional_record() -> Result<(), OptionalRecordError> {
    Err(OptionalRecordError { message: None })
}

#[export]
pub fn fail_shadowed() -> Result<(), ShadowedError> {
    Err(ShadowedError::String {
        message: "failed".to_owned(),
    })
}

#[export]
pub fn echo_message(message: Message) -> Message {
    message
}

#[export]
pub fn echo_message_record(record: MessageRecord) -> MessageRecord {
    record
}
