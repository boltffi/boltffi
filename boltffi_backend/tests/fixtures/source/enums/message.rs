#[data]
pub enum Message {
    Empty,
    Text(String),
    Named { name: String },
}
