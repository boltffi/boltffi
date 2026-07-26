#[data]
pub enum PrimitiveValue {
    Null,
    Bool(bool),
    Int(i64),
    Double(f64),
    MaybeDouble(Option<f64>),
    Doubles(Vec<f64>),
}

#[data(impl)]
impl PrimitiveValue {
    pub fn new(value: i32) -> Self {
        PrimitiveValue::Int(i64::from(value))
    }

    pub fn scaled(&self, factor: i32) -> i32 {
        factor
    }
}

#[data]
pub enum Int {
    Value(i64),
}
