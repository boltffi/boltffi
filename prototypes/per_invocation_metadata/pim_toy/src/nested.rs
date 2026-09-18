use std::collections::HashMap;

use crate::geometry;
use pim_macros::data;

#[data]
pub struct Node {
    pub next: Option<Box<Node>>,
}

#[data]
pub struct Index {
    pub table: HashMap<String, Vec<geometry::Point>>,
}
