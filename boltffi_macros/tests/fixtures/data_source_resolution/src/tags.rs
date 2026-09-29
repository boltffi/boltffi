#[boltffi::data]
#[derive(Clone)]
pub struct Tag {
    pub label: String,
}

#[boltffi::export]
pub fn retag(tag: Tag) -> Tag {
    tag
}
