#[data]
pub enum StorageError {
    InvalidPath,
    Io(String),
}

#[data]
pub enum RecorderError {
    InvalidConfig,
    StorageError(StorageError),
    SessionNotStarted,
}
