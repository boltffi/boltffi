#[data]
pub struct User {
    pub name: String,
    pub age: u32,
    pub role: Role,
    pub nickname: Option<String>,
    pub tags: Vec<String>,
}
