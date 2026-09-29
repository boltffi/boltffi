#[repr(u8)]
#[data]
pub enum Mode {
    Fast = 1,
    Slow = 2,
}

#[export]
pub trait Handler: Send + Sync {
    fn process(&self, value: u32) -> u32;
}

#[export]
pub fn greet(
    name: String,
    #[boltffi::default("world")] greeting: String,
    #[boltffi::default(3)] times: u32,
    #[boltffi::default(-1)] offset: i64,
    #[boltffi::default(true)] shout: bool,
    #[boltffi::default(0.5)] ratio: f32,
    #[boltffi::default(Mode::Slow)] mode: Mode,
    #[boltffi::default(None)] suffix: Option<String>,
    #[boltffi::default(7)] limit: Option<u16>,
) -> String {
    let _ = (greeting, times, offset, shout, ratio, mode, suffix, limit);
    name
}

pub struct Server {
    port: u16,
}

#[export]
impl Server {
    pub fn new(#[boltffi::default(8080)] port: u16) -> Self {
        Self { port }
    }

    pub async fn start(
        port: u16,
        #[boltffi::default(None)] first: Option<std::sync::Arc<dyn Handler>>,
        #[boltffi::default(None)] second: Option<std::sync::Arc<dyn Handler>>,
    ) -> Self {
        let _ = (first, second);
        Self { port }
    }

    pub fn port(&self, #[boltffi::default(false)] mapped: bool) -> u16 {
        let _ = mapped;
        self.port
    }
}
