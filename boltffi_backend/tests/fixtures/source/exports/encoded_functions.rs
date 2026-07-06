#[export]
pub fn echo_bytes(bytes: Vec<u8>) -> Vec<u8> {
    bytes
}

#[export]
pub fn keep_person(person: Person) -> Person {
    person
}

#[export]
pub fn keep_shape(shape: Shape) -> Shape {
    shape
}

#[export]
pub fn echo_message(message: Message) -> Message {
    message
}
