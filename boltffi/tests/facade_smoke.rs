use boltffi::*;

scaffolding!();

#[data]
#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

#[export]
fn add(a: Point, b: Point) -> Point {
    Point {
        x: a.x + b.x,
        y: a.y + b.y,
    }
}

struct ConditionalClass;

#[export]
impl ConditionalClass {
    pub fn new() -> Self {
        Self
    }

    #[cfg(any())]
    pub fn unavailable(&mut self, value: UnavailableType) {
        drop(value);
    }
}

#[test]
fn smoke() {
    let conditional_class = ConditionalClass::new();
    let a = Point { x: 1.0, y: 2.0 };
    let b = Point { x: 3.0, y: 4.0 };
    let c = add(a, b);
    assert_eq!(std::mem::size_of_val(&conditional_class), 0);
    assert_eq!(c.x, 4.0);
    assert_eq!(c.y, 6.0);
}
