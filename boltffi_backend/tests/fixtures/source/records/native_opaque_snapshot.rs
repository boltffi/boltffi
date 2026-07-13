#[data(opaque)]
pub struct Snapshot {
    pub count: u32,
    pub title: String,
    pub payload: Vec<u8>,
    pub score: Option<f64>,
    pub label: Option<String>,
}
