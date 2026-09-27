#[data]
pub struct Marker {}

#[data(impl)]
impl Marker {
    pub fn describe(&self) -> String {
        String::from("marker")
    }

    pub fn into_code(self) -> u32 {
        7
    }

    pub fn touch(&mut self) {}
}
