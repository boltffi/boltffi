use std::collections::HashMap;

#[data]
pub struct Blob {
    pub id: u32,
    pub payload: Vec<u8>,
    pub samples: Vec<u32>,
    pub checksum: Option<Vec<u8>>,
    pub chunks: Vec<Vec<u8>>,
    pub ratio: f64,
    pub weight: Option<f32>,
}

#[data]
pub struct Nested {
    pub maybe_chunks: Option<Vec<Vec<u8>>>,
    pub pages: Vec<Vec<Vec<u8>>>,
    pub sparse: Vec<Option<Vec<Vec<u8>>>>,
    pub named: HashMap<String, Vec<u8>>,
    pub maybe_named: Option<HashMap<String, Vec<Vec<u8>>>>,
}

#[data]
pub struct Label {
    pub name: String,
}

#[data]
pub enum Frame {
    Empty,
    Data { payload: Vec<u8> },
    Tagged(u32, Vec<i64>),
    Named { name: String },
}

#[error]
pub struct BlobError {
    pub message: String,
    pub payload: Vec<u8>,
}

#[export]
pub fn fail_blob() -> Result<(), BlobError> {
    Err(BlobError {
        message: "failed".to_owned(),
        payload: Vec::new(),
    })
}
