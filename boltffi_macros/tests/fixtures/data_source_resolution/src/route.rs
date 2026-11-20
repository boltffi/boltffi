#[boltffi::data]
#[derive(Clone)]
pub struct Route {
    pub name: String,
    pub branches: Vec<self::Route>,
}
