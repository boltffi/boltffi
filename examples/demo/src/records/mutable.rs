use boltffi::{data, export};

#[data]
#[derive(Clone)]
pub struct MutableRecord {
    pub title: String,
    pub tags: Vec<String>,
}

#[data(impl)]
impl MutableRecord {
    pub fn new(title: String) -> Self {
        Self {
            title,
            tags: vec!["original".to_owned()],
        }
    }

    pub fn rename(&mut self, title: &str) -> usize {
        self.title = title.to_owned();
        self.tags.push(title.to_owned());
        self.tags.len()
    }
}

#[data]
#[derive(Clone)]
pub enum MutableMessage {
    Text(String),
    Record(MutableRecord),
}

#[data(impl)]
impl MutableMessage {
    pub fn text(value: String) -> Self {
        Self::Text(value)
    }

    pub fn replace(&mut self) {
        *self = Self::Record(MutableRecord::new("updated".to_owned()));
    }
}

#[export]
pub fn replace_mutable_text(value: &mut String) {
    *value = "updated".to_owned();
}

#[export]
pub fn replace_mutable_records(values: &mut Vec<MutableRecord>) {
    values.clear();
    values.push(MutableRecord::new("updated".to_owned()));
}

#[repr(u8)]
#[data]
#[derive(Clone, Copy)]
pub enum MutableMode {
    Idle = 1,
    Running = 2,
}

#[data(impl)]
impl MutableMode {
    pub fn start(&mut self) {
        *self = Self::Running;
    }
}
