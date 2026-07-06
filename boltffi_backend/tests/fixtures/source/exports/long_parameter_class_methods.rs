pub struct Calculator;

#[export]
impl Calculator {
    pub fn new() -> Self {
        Self
    }

    pub fn weighted_sum(&self, first: i32, second: i32, third: i32, fourth: i32) -> i32 {
        first + second + third + fourth
    }
}
