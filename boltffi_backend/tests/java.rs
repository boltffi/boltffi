use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use boltffi_ast::PackageInfo;
use boltffi_backend::{
    CoverageMode, DeclarationLabel, Error,
    target::java::{JavaDesktopLoader, JavaHost, JavaVersion},
};
use boltffi_binding::{DeclarationRef, Native, lower};

mod java_toolchain;

use java_toolchain::{JavaCompiler, JavaEightCompilation};

const PRIMITIVE_FUNCTIONS: &str = r#"
    #[export]
    pub fn carriers(
        flag: bool,
        signed_byte: i8,
        unsigned_byte: u8,
        signed_short: i16,
        unsigned_short: u16,
        signed_word: i32,
        unsigned_word: u32,
        signed_wide: i64,
        unsigned_wide: u64,
        signed_size: isize,
        unsigned_size: usize,
        single: f32,
        double: f64,
    ) -> u64 {
        unsigned_wide
    }

    #[export]
    pub fn enabled(flag: bool) -> bool {
        flag
    }

    #[export]
    pub fn refresh() {}
"#;

const MUTABLE_PARAMETERS: &str = r#"
    #[export]
    pub fn increment(values: &mut [u64]) {
        values.iter_mut().for_each(|value| *value += 1);
    }
"#;

const MAP_FUNCTIONS: &str = r#"
    use std::collections::HashMap;

    #[export]
    pub fn echo_scores(scores: HashMap<String, u32>) -> HashMap<String, u32> { scores }
"#;

const DIRECT_RECORD_FUNCTIONS: &str = r#"
    #[repr(C)]
    #[data]
    pub struct Point {
        pub x: f64,
        pub y: f64,
        pub visible: bool,
        pub color: u32,
    }

    #[export]
    pub fn echo_point(point: Point) -> Point { point }
"#;

const DIRECT_RECORD_CALLS: &str = r#"
    #[repr(C)]
    #[data]
    pub struct Counter {
        pub value: i32,
    }

    #[data(impl)]
    impl Counter {
        pub fn new(value: i32) -> Self { Self { value } }
        pub fn zero() -> Self { Self { value: 0 } }
        pub fn current(&self) -> i32 { self.value }
        pub fn increment(&mut self, amount: i32) { self.value += amount; }
        pub fn added(&self, other: Counter) -> Counter {
            Counter { value: self.value + other.value }
        }
    }
"#;

const ENCODED_RECORD: &str = r#"
    #[data]
    pub struct Profile {
        pub name: String,
        pub samples: Vec<i32>,
        pub marker: Option<i32>,
    }
"#;

const RESULT_RECORD: &str = r#"
    #[data]
    pub struct ResultHolder {
        pub outcome: Result<Vec<i32>, Option<String>>,
    }
"#;

const CUSTOM_TYPES: &str = r#"
    pub struct Email(String);

    #[custom_ffi]
    impl CustomFfiConvertible for Email {
        type FfiRepr = String;
        type Error = String;

        fn into_ffi(&self) -> String { self.0.clone() }
        fn try_from_ffi(value: String) -> Result<Self, String> { Ok(Self(value)) }
    }

    custom_type!(
        pub TimestampValue,
        remote = TimestampRust,
        repr = i64,
        into_ffi = timestamp_into_ffi,
        try_from_ffi = timestamp_from_ffi,
    );

    #[data]
    pub struct Event {
        pub email: Email,
        pub timestamp: TimestampRust,
        pub attendees: Vec<Email>,
    }

    #[export]
    pub fn echo_email(email: Email) -> Email { email }

    #[export]
    pub fn echo_timestamp(timestamp: TimestampRust) -> TimestampRust { timestamp }

    #[export]
    pub fn echo_event(event: Event) -> Event { event }

    #[export]
    pub fn echo_emails(emails: Vec<Email>) -> Vec<Email> { emails }

    #[export]
    pub fn echo_timestamps(timestamps: Vec<TimestampRust>) -> Vec<TimestampRust> { timestamps }

    #[export]
    pub fn maybe_timestamp(timestamp: Option<TimestampRust>) -> Option<TimestampRust> { timestamp }
"#;

const ENCODED_RECORD_CALLS: &str = r#"
    #[data]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }

    #[data(impl)]
    impl Point {
        pub fn new(x: f64, y: f64) -> Self { Self { x, y } }
        pub fn origin() -> Self { Self { x: 0.0, y: 0.0 } }
        pub fn try_unit(x: f64, y: f64) -> Result<Self, String> {
            let length = (x * x + y * y).sqrt();
            if length == 0.0 {
                Err("cannot normalize zero vector".to_owned())
            } else {
                Ok(Self { x: x / length, y: y / length })
            }
        }
        pub fn checked_unit(x: f64, y: f64) -> Option<Self> {
            let length = (x * x + y * y).sqrt();
            (length != 0.0).then(|| Self { x: x / length, y: y / length })
        }
        pub fn distance(&self) -> f64 { (self.x * self.x + self.y * self.y).sqrt() }
        pub fn scale(&mut self, factor: f64) {
            self.x *= factor;
            self.y *= factor;
        }
        pub fn add(&self, other: Point) -> Point {
            Point { x: self.x + other.x, y: self.y + other.y }
        }
        pub fn path_length(points: Vec<Point>) -> f64 {
            points.windows(2).map(|pair| {
                let dx = pair[1].x - pair[0].x;
                let dy = pair[1].y - pair[0].y;
                (dx * dx + dy * dy).sqrt()
            }).sum()
        }
    }

    #[export]
    pub fn echo_point(point: Point) -> Point { point }
"#;

const RECORD_DEFAULTS: &str = r#"
    #[data]
    pub struct ServiceConfig {
        pub name: String,
        #[boltffi::default(3)]
        pub retries: i32,
        #[boltffi::default("standard")]
        pub region: String,
        /// Optional endpoint.
        #[boltffi::default(None)]
        pub endpoint: Option<String>,
        #[boltffi::default("https://default")]
        pub backup_endpoint: Option<String>,
    }
"#;

const ERROR_RECORD: &str = r#"
    #[error]
    pub struct AppError {
        pub code: i32,
        pub message: String,
    }

    #[export]
    pub fn may_fail(valid: bool) -> Result<String, AppError> {
        match valid {
            true => Ok("ok".to_owned()),
            false => Err(AppError { code: 400, message: "invalid".to_owned() }),
        }
    }
"#;

const ENUMS: &str = r#"
    #[repr(u8)]
    #[data]
    pub enum Mode {
        Fast = 1,
        Slow = 7,
    }

    #[repr(u64)]
    #[data]
    pub enum WideMode {
        Zero = 0,
        Maximum = 18446744073709551615,
    }

    #[data(impl)]
    impl Mode {
        pub fn fast() -> Self { Self::Fast }
        pub fn is_fast(&self) -> bool { matches!(self, Self::Fast) }
    }

    #[data]
    pub enum Shape {
        Empty,
        Circle { radius: f64 },
        Label(String),
    }

    #[data(impl)]
    impl Shape {
        pub fn empty() -> Self { Self::Empty }
        pub fn is_empty(&self) -> bool { matches!(self, Self::Empty) }
    }

    #[export]
    pub fn echo_mode(value: Mode) -> Mode { value }

    #[export]
    pub fn echo_modes(values: Vec<Mode>) -> Vec<Mode> { values }

    #[export]
    pub fn echo_wide_mode(value: WideMode) -> WideMode { value }

    #[export]
    pub fn echo_shape(value: Shape) -> Shape { value }
"#;

const ERROR_ENUMS: &str = r#"
    #[error]
    pub enum ParseError {
        Missing,
        Invalid,
    }

    #[error]
    pub enum ApiError {
        Message { message: String },
        Code(i32),
    }

    #[export]
    pub fn parse(valid: bool) -> Result<i32, ParseError> {
        if valid { Ok(1) } else { Err(ParseError::Invalid) }
    }

    #[export]
    pub fn request(valid: bool) -> Result<String, ApiError> {
        if valid {
            Ok("ok".to_owned())
        } else {
            Err(ApiError::Message { message: "failed".to_owned() })
        }
    }
"#;

const CLASSES: &str = r#"
    pub struct Counter {
        value: i32,
    }

    #[export(single_threaded)]
    impl Counter {
        pub fn new(value: i32) -> Self { Self { value } }

        pub fn try_new(value: i32) -> Result<Self, String> {
            if value < 0 {
                Err("negative counter".to_owned())
            } else {
                Ok(Self { value })
            }
        }

        pub fn get(&self) -> i32 { self.value }

        pub fn set(&mut self, value: i32) { self.value = value; }
    }

    pub struct FallibleOnly {
        name: String,
    }

    #[export]
    impl FallibleOnly {
        pub fn open(name: String) -> Result<Self, String> {
            if name.is_empty() {
                Err("empty name".to_owned())
            } else {
                Ok(Self { name })
            }
        }

        pub fn name(&self) -> String { self.name.clone() }
    }

    pub struct Factory;

    #[export]
    impl Factory {
        pub fn new() -> Self { Self }

        pub fn make(value: i32) -> Counter { Counter { value } }

        pub fn maybe(&self, value: i32) -> Option<Counter> {
            (value >= 0).then_some(Counter { value })
        }

        pub fn read(&self, counter: &Counter) -> i32 { counter.value }
    }

    #[export]
    pub fn describe(counter: &Counter) -> String { counter.value.to_string() }

    #[export]
    pub fn make_counter(value: i32) -> Counter { Counter { value } }

    #[export]
    pub fn maybe_counter(value: i32) -> Option<Counter> {
        (value >= 0).then_some(Counter { value })
    }
"#;

const CALLBACKS: &str = r#"
    #[repr(C)]
    #[data]
    pub struct DataPoint {
        pub x: f64,
        pub y: f64,
    }

    #[export]
    pub trait DataProvider: Send + Sync {
        fn get_count(&self) -> u32;
        fn get_item(&self, index: u32) -> DataPoint;
    }

    #[export]
    pub fn consume(provider: Box<dyn DataProvider>) -> u32 {
        provider.get_count()
    }

    #[export]
    pub trait ValueCallback {
        fn apply(&self, value: i32) -> i32;
    }

    struct Increment {
        delta: i32,
    }

    impl ValueCallback for Increment {
        fn apply(&self, value: i32) -> i32 { value + self.delta }
    }

    #[export]
    pub fn make_callback(delta: i32) -> Box<dyn ValueCallback> {
        Box::new(Increment { delta })
    }

    #[export]
    pub fn invoke_callback(callback: Box<dyn ValueCallback>, value: i32) -> i32 {
        callback.apply(value)
    }

    #[export]
    pub fn invoke_optional_callback(
        callback: Option<Box<dyn ValueCallback>>,
        value: i32,
    ) -> i32 {
        callback.map(|callback| callback.apply(value)).unwrap_or(value)
    }
"#;

const ENCODED_CALLBACKS: &str = r#"
    #[error]
    pub enum MathError {
        Invalid,
    }

    #[export]
    pub trait MessageFormatter {
        fn format(&self, value: String) -> String;
    }

    #[export]
    pub trait OptionCallback {
        fn find(&self, key: i32) -> Option<i32>;
    }

    #[export]
    pub trait ResultCallback {
        fn compute(&self, value: i32) -> Result<i32, MathError>;
    }

    #[export]
    pub fn format_value(callback: impl MessageFormatter, value: String) -> String {
        callback.format(value)
    }

    #[export]
    pub fn find_value(callback: impl OptionCallback, key: i32) -> Option<i32> {
        callback.find(key)
    }

    #[export]
    pub fn compute_value(callback: impl ResultCallback, value: i32) -> Result<i32, MathError> {
        callback.compute(value)
    }
"#;

const CLOSURES: &str = r#"
    #[repr(C)]
    #[data]
    pub struct Point {
        pub x: i32,
        pub y: i32,
    }

    #[error]
    pub enum MathError {
        Invalid,
    }

    #[export]
    pub fn apply(callback: impl Fn(i32) -> i32, value: i32) -> i32 {
        callback(value)
    }

    #[export]
    pub fn apply_point(callback: impl Fn(Point) -> Point, value: Point) -> Point {
        callback(value)
    }

    #[export]
    pub fn apply_text(callback: impl Fn(String) -> String, value: String) -> String {
        callback(value)
    }

    #[export]
    pub fn apply_fallible(
        callback: impl Fn(i32) -> Result<i32, MathError>,
        value: i32,
    ) -> Result<i32, MathError> {
        callback(value)
    }
"#;

const ASYNC_FUNCTIONS: &str = r#"
    #[data]
    pub struct AsyncPoint {
        pub x: f64,
        pub y: f64,
    }

    #[error]
    pub enum AsyncError {
        Unavailable,
    }

    #[export]
    pub async fn refresh() {}

    #[export]
    pub async fn load_count(value: u32) -> u32 { value }

    #[export]
    pub async fn load_name() -> String { "ready".to_owned() }

    #[export]
    pub async fn load_point() -> AsyncPoint { AsyncPoint { x: 1.0, y: 2.0 } }

    #[export]
    pub async fn check(available: bool) -> Result<bool, AsyncError> {
        if available { Ok(true) } else { Err(AsyncError::Unavailable) }
    }

    pub struct Worker;

    #[export]
    impl Worker {
        pub fn new() -> Self { Self }
        pub async fn run(&self, value: i32) -> i32 { value }
    }
"#;

const STREAMS: &str = r#"
    use std::sync::Arc;
    use boltffi::EventSubscription;

    #[repr(C)]
    #[data]
    pub struct StreamPoint {
        pub x: f64,
        pub y: f64,
    }

    #[data]
    pub struct StreamMessage {
        pub text: String,
        pub values: Vec<i32>,
    }

    #[repr(u32)]
    #[data]
    pub enum StreamMode {
        Fast = 1,
        Slow = 2,
    }

    pub struct EventBus;

    #[export(single_threaded)]
    impl EventBus {
        pub fn new() -> Self { Self }

        #[ffi_stream(item = i32)]
        pub fn values(&self) -> Arc<EventSubscription<i32>> { todo!() }

        #[ffi_stream(item = StreamPoint)]
        pub fn points(&self) -> Arc<EventSubscription<StreamPoint>> { todo!() }

        #[ffi_stream(item = StreamMode)]
        pub fn modes(&self) -> Arc<EventSubscription<StreamMode>> { todo!() }

        #[ffi_stream(item = StreamMessage)]
        pub fn messages(&self) -> Arc<EventSubscription<StreamMessage>> { todo!() }

        #[ffi_stream(item = Result<i32, String>)]
        pub fn results(&self) -> Arc<EventSubscription<Result<i32, String>>> { todo!() }

        #[ffi_stream(item = i32, mode = "batch")]
        pub fn value_batches(&self) -> Arc<EventSubscription<i32>> { todo!() }

        #[ffi_stream(item = i32, mode = "callback")]
        pub fn value_callbacks(&self) -> Arc<EventSubscription<i32>> { todo!() }
    }
"#;

const STREAM_RUNTIME_PROBE: &str = r#"
    package com.boltffi.demo;

    import java.util.Arrays;
    import java.util.Collections;
    import java.util.List;
    import java.util.concurrent.atomic.AtomicInteger;
    import java.util.concurrent.atomic.AtomicLong;

    public final class StreamRuntimeProbe {
        private StreamRuntimeProbe() {}

        public static void main(String[] arguments) {
            callbackDelivery();
            callbackFailure();
            batchLifecycle();
            directBatches();
        }

        private static void callbackDelivery() {
            AtomicInteger polls = new AtomicInteger();
            AtomicInteger reads = new AtomicInteger();
            AtomicInteger unsubscribed = new AtomicInteger();
            AtomicInteger freed = new AtomicInteger();
            AtomicLong pending = new AtomicLong();
            java.util.concurrent.CopyOnWriteArrayList<Integer> received =
                new java.util.concurrent.CopyOnWriteArrayList<>();
            StreamSubscription<Integer> subscription = BoltFfiStream.callback(
                7L,
                16L,
                (stream, maxCount) -> reads.getAndIncrement() == 0
                    ? Arrays.asList(11, 13)
                    : Collections.emptyList(),
                (stream, continuation) -> {
                    if (polls.getAndIncrement() == 0) {
                        BoltFfiAsync.resume(continuation, (byte) 0);
                    } else {
                        pending.set(continuation);
                    }
                },
                stream -> {
                    unsubscribed.incrementAndGet();
                    BoltFfiAsync.resume(pending.get(), (byte) 1);
                },
                stream -> freed.incrementAndGet(),
                received::add
            );
            require(received.equals(Arrays.asList(11, 13)), "callback values");
            require(polls.get() == 2, "callback repoll");
            subscription.close();
            subscription.close();
            require(unsubscribed.get() == 1, "callback unsubscribe");
            require(freed.get() == 1, "callback free");
        }

        private static void callbackFailure() {
            AtomicInteger unsubscribed = new AtomicInteger();
            AtomicInteger freed = new AtomicInteger();
            try {
                BoltFfiStream.callback(
                    8L,
                    16L,
                    (stream, maxCount) -> { throw new IllegalStateException("decode"); },
                    (stream, continuation) -> BoltFfiAsync.resume(continuation, (byte) 0),
                    stream -> unsubscribed.incrementAndGet(),
                    stream -> freed.incrementAndGet(),
                    item -> {}
                );
                fail("callback decode failure");
            } catch (IllegalStateException failure) {
                require(failure.getMessage().equals("decode"), "callback failure value");
            }
            require(unsubscribed.get() == 1, "failure unsubscribe");
            require(freed.get() == 1, "failure free");
        }

        private static void batchLifecycle() {
            AtomicInteger unsubscribed = new AtomicInteger();
            AtomicInteger freed = new AtomicInteger();
            StreamSubscription<Integer> subscription = StreamSubscription.batch(
                9L,
                (stream, maxCount) -> Arrays.asList(3, 5),
                (stream, timeout) -> 2,
                stream -> unsubscribed.incrementAndGet(),
                stream -> freed.incrementAndGet()
            );
            require(subscription.popBatch(2).equals(Arrays.asList(3, 5)), "batch values");
            require(subscription.waitForItems(10) == 2, "batch wait");
            subscription.cancel();
            subscription.unsubscribe();
            require(unsubscribed.get() == 1, "batch unsubscribe");
            require(freed.get() == 1, "batch free");
            require(subscription.popBatch(2).isEmpty(), "closed batch values");
            require(subscription.waitForItems(10) == -1, "closed batch wait");

            AtomicInteger failureFree = new AtomicInteger();
            StreamSubscription<Integer> failing = StreamSubscription.batch(
                10L,
                (stream, maxCount) -> Collections.emptyList(),
                (stream, timeout) -> 0,
                stream -> { throw new IllegalStateException("unsubscribe"); },
                stream -> {
                    failureFree.incrementAndGet();
                    throw new IllegalStateException("free");
                }
            );
            try {
                failing.close();
                fail("batch lifecycle failure");
            } catch (IllegalStateException failure) {
                require(failure.getMessage().equals("unsubscribe"), "batch failure value");
                require(failure.getSuppressed().length == 1, "batch suppressed free");
            }
            require(failureFree.get() == 1, "batch failure free");
        }

        private static void directBatches() {
            require(BoltFfiStreamBatches.booleans(new byte[] { 0, 1 }).equals(
                Arrays.asList(false, true)
            ), "boolean batch");
            require(BoltFfiStreamBatches.bytes(new byte[] { 2, 4 }).equals(
                Arrays.asList((byte) 2, (byte) 4)
            ), "byte batch");
            require(BoltFfiStreamBatches.ints(
                DirectVectorCodec.writeIntArray(new int[] { 1, 2 })
            ).equals(Arrays.asList(1, 2)), "integer batch");
            List<String> mapped = BoltFfiStreamBatches.map(
                Arrays.asList(1, 2),
                value -> "value-" + value
            );
            require(mapped.equals(Arrays.asList("value-1", "value-2")), "mapped batch");
        }

        private static void require(boolean condition, String message) {
            if (!condition) fail(message);
        }

        private static void fail(String message) {
            throw new AssertionError(message);
        }
    }
"#;

const STREAM_FLOW_RUNTIME_PROBE: &str = r#"
    package com.boltffi.demo;

    import java.util.Arrays;
    import java.util.Collections;
    import java.util.concurrent.CountDownLatch;
    import java.util.concurrent.Flow;
    import java.util.concurrent.TimeUnit;
    import java.util.concurrent.atomic.AtomicInteger;

    public final class StreamFlowRuntimeProbe {
        private StreamFlowRuntimeProbe() {}

        public static void main(String[] arguments) throws Exception {
            deliversDemandAndCompletes();
            externalCloseWakesPublisher();
        }

        private static void deliversDemandAndCompletes() throws Exception {
            AtomicInteger reads = new AtomicInteger();
            AtomicInteger unsubscribed = new AtomicInteger();
            AtomicInteger freed = new AtomicInteger();
            CountDownLatch completed = new CountDownLatch(1);
            java.util.concurrent.CopyOnWriteArrayList<Integer> received =
                new java.util.concurrent.CopyOnWriteArrayList<>();
            StreamSubscription<Integer> stream = StreamSubscription.batch(
                21L,
                (handle, maxCount) -> reads.getAndIncrement() == 0
                    ? Arrays.asList(17, 19)
                    : Collections.emptyList(),
                (handle, timeout) -> -1,
                handle -> unsubscribed.incrementAndGet(),
                handle -> freed.incrementAndGet()
            );
            stream.toPublisher().subscribe(new Flow.Subscriber<Integer>() {
                public void onSubscribe(Flow.Subscription subscription) {
                    subscription.request(3);
                }

                public void onNext(Integer item) {
                    received.add(item);
                }

                public void onError(Throwable failure) {
                    throw new AssertionError(failure);
                }

                public void onComplete() {
                    completed.countDown();
                }
            });
            require(completed.await(5, TimeUnit.SECONDS), "publisher completion");
            require(received.equals(Arrays.asList(17, 19)), "publisher values");
            require(unsubscribed.get() == 1, "publisher unsubscribe");
            require(freed.get() == 1, "publisher free");
        }

        private static void externalCloseWakesPublisher() throws Exception {
            AtomicInteger unsubscribed = new AtomicInteger();
            AtomicInteger freed = new AtomicInteger();
            CountDownLatch subscribed = new CountDownLatch(1);
            CountDownLatch completed = new CountDownLatch(1);
            StreamSubscription<Integer> stream = StreamSubscription.batch(
                22L,
                (handle, maxCount) -> Collections.emptyList(),
                (handle, timeout) -> 0,
                handle -> unsubscribed.incrementAndGet(),
                handle -> freed.incrementAndGet()
            );
            stream.toPublisher().subscribe(new Flow.Subscriber<Integer>() {
                public void onSubscribe(Flow.Subscription subscription) {
                    subscribed.countDown();
                }

                public void onNext(Integer item) {
                    throw new AssertionError("unexpected item");
                }

                public void onError(Throwable failure) {
                    throw new AssertionError(failure);
                }

                public void onComplete() {
                    completed.countDown();
                }
            });
            require(subscribed.await(5, TimeUnit.SECONDS), "publisher subscription");
            stream.close();
            require(completed.await(5, TimeUnit.SECONDS), "external close completion");
            require(unsubscribed.get() == 1, "external close unsubscribe");
            require(freed.get() == 1, "external close free");
        }

        private static void require(boolean condition, String message) {
            if (!condition) throw new AssertionError(message);
        }
    }
"#;

const ASYNC_RUNTIME_PROBE: &str = r#"
    package com.boltffi.demo;

    import java.util.concurrent.CompletableFuture;
    import java.util.concurrent.CompletionException;
    import java.util.concurrent.CountDownLatch;
    import java.util.concurrent.atomic.AtomicInteger;
    import java.util.concurrent.atomic.AtomicLong;

    public final class AsyncRuntimeProbe {
        private AsyncRuntimeProbe() {}

        public static void main(String[] arguments) {
            immediateReady();
            asynchronousPending();
            startFailure();
            pollFailure();
            completionFailure();
            panicTranslation();
            cancellation();
            lifecycleFailure();
            readyCancellationRace();
        }

        private static void immediateReady() {
            AtomicInteger completed = new AtomicInteger();
            AtomicInteger cancelled = new AtomicInteger();
            AtomicInteger freed = new AtomicInteger();
            CompletableFuture<Integer> result = BoltFfiAsync.call(
                () -> 11L,
                (future, continuation) -> BoltFfiAsync.resume(continuation, (byte) 0),
                future -> {
                    completed.incrementAndGet();
                    return 42;
                },
                future -> cancelled.incrementAndGet(),
                future -> freed.incrementAndGet()
            );
            require(result.join() == 42, "immediate value");
            require(completed.get() == 1, "immediate completion");
            require(cancelled.get() == 0, "immediate cancellation");
            require(freed.get() == 1, "immediate free");
        }

        private static void asynchronousPending() {
            AtomicInteger polls = new AtomicInteger();
            AtomicInteger freed = new AtomicInteger();
            AtomicLong continuation = new AtomicLong();
            CompletableFuture<Integer> result = BoltFfiAsync.call(
                () -> 12L,
                (future, handle) -> {
                    polls.incrementAndGet();
                    continuation.set(handle);
                },
                future -> 73,
                future -> fail("pending cancellation"),
                future -> freed.incrementAndGet()
            );
            require(!result.isDone(), "pending result");
            BoltFfiAsync.resume(continuation.get(), (byte) 1);
            require(polls.get() == 2, "pending repoll");
            require(!result.isDone(), "repoll result");
            BoltFfiAsync.resume(continuation.get(), (byte) 0);
            require(result.join() == 73, "pending value");
            require(freed.get() == 1, "pending free");
        }

        private static void startFailure() {
            AtomicInteger cancelled = new AtomicInteger();
            AtomicInteger freed = new AtomicInteger();
            CompletableFuture<Integer> result = BoltFfiAsync.call(
                () -> { throw new IllegalStateException("start"); },
                (future, continuation) -> fail("start poll"),
                future -> failValue("start complete"),
                future -> cancelled.incrementAndGet(),
                future -> freed.incrementAndGet()
            );
            requireFailure(result, "start");
            require(cancelled.get() == 0, "start cancellation");
            require(freed.get() == 0, "start free");
        }

        private static void pollFailure() {
            AtomicInteger cancelled = new AtomicInteger();
            AtomicInteger freed = new AtomicInteger();
            CompletableFuture<Integer> result = BoltFfiAsync.call(
                () -> 13L,
                (future, continuation) -> { throw new IllegalStateException("poll"); },
                future -> failValue("poll complete"),
                future -> cancelled.incrementAndGet(),
                future -> freed.incrementAndGet()
            );
            requireFailure(result, "poll");
            require(cancelled.get() == 1, "poll cancellation");
            require(freed.get() == 1, "poll free");
        }

        private static void completionFailure() {
            AtomicInteger cancelled = new AtomicInteger();
            AtomicInteger freed = new AtomicInteger();
            CompletableFuture<Integer> result = BoltFfiAsync.call(
                () -> 14L,
                (future, continuation) -> BoltFfiAsync.resume(continuation, (byte) 0),
                future -> { throw new IllegalStateException("complete"); },
                future -> cancelled.incrementAndGet(),
                future -> freed.incrementAndGet()
            );
            requireFailure(result, "complete");
            require(cancelled.get() == 0, "completion cancellation");
            require(freed.get() == 1, "completion free");
        }

        private static void panicTranslation() {
            IllegalStateException original = new IllegalStateException("complete");
            RuntimeException translated = BoltFfiAsync.failure(
                original,
                () -> "native panic".getBytes(java.nio.charset.StandardCharsets.UTF_8)
            );
            require("native panic".equals(translated.getMessage()), "panic message");
            require(translated.getCause() == original, "panic cause");
            require(
                BoltFfiAsync.failure(original, () -> new byte[0]) == original,
                "empty panic message"
            );
            require(
                BoltFfiAsync.failure(
                    original,
                    () -> { throw new IllegalStateException("panic lookup"); }
                ) == original,
                "panic lookup failure"
            );
        }

        private static void cancellation() {
            AtomicInteger cancelled = new AtomicInteger();
            AtomicInteger freed = new AtomicInteger();
            AtomicLong continuation = new AtomicLong();
            CompletableFuture<Integer> result = BoltFfiAsync.call(
                () -> 15L,
                (future, handle) -> continuation.set(handle),
                future -> failValue("cancel complete"),
                future -> cancelled.incrementAndGet(),
                future -> freed.incrementAndGet()
            );
            require(result.cancel(false), "cancel accepted");
            require(result.isCancelled(), "cancel state");
            require(cancelled.get() == 1, "cancel callback");
            require(freed.get() == 1, "cancel free");
            BoltFfiAsync.resume(continuation.get(), (byte) 0);
            require(cancelled.get() == 1, "late cancel callback");
            require(freed.get() == 1, "late cancel free");
        }

        private static void lifecycleFailure() {
            CompletableFuture<Integer> result = BoltFfiAsync.call(
                () -> 16L,
                (future, continuation) -> {},
                future -> failValue("lifecycle complete"),
                future -> { throw new IllegalStateException("cancel lifecycle"); },
                future -> { throw new IllegalStateException("free lifecycle"); }
            );
            require(result.cancel(false), "lifecycle cancel accepted");
            require(result.isCancelled(), "lifecycle cancel state");
        }

        private static void readyCancellationRace() {
            java.util.stream.IntStream.range(0, 500).forEach(iteration -> {
                AtomicInteger completed = new AtomicInteger();
                AtomicInteger cancelled = new AtomicInteger();
                AtomicInteger freed = new AtomicInteger();
                AtomicLong continuation = new AtomicLong();
                CompletableFuture<Integer> result = BoltFfiAsync.call(
                    () -> 17L,
                    (future, handle) -> continuation.set(handle),
                    future -> {
                        completed.incrementAndGet();
                        return 99;
                    },
                    future -> cancelled.incrementAndGet(),
                    future -> freed.incrementAndGet()
                );
                CountDownLatch start = new CountDownLatch(1);
                Thread cancellation = new Thread(() -> {
                    await(start);
                    result.cancel(false);
                });
                Thread readiness = new Thread(() -> {
                    await(start);
                    BoltFfiAsync.resume(continuation.get(), (byte) 0);
                });
                cancellation.start();
                readiness.start();
                start.countDown();
                join(cancellation);
                join(readiness);
                require(freed.get() == 1, "race free " + iteration);
                require(completed.get() + cancelled.get() == 1, "race owner " + iteration);
                if (result.isCancelled()) {
                    require(cancelled.get() == 1, "race cancellation " + iteration);
                } else {
                    require(result.join() == 99, "race value " + iteration);
                    require(completed.get() == 1, "race completion " + iteration);
                }
            });
        }

        private static void await(CountDownLatch latch) {
            try {
                latch.await();
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
                throw new RuntimeException(error);
            }
        }

        private static void join(Thread thread) {
            try {
                thread.join();
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
                throw new RuntimeException(error);
            }
        }

        private static void requireFailure(CompletableFuture<?> result, String message) {
            try {
                result.join();
                fail("missing " + message + " failure");
            } catch (CompletionException error) {
                require(error.getCause().getMessage().equals(message), message + " failure");
            }
        }

        private static Integer failValue(String message) {
            fail(message);
            return 0;
        }

        private static void require(boolean condition, String message) {
            if (!condition) fail(message);
        }

        private static void fail(String message) {
            throw new AssertionError(message);
        }
    }
"#;

const ASYNC_CALLBACKS: &str = r#"
    #[repr(C)]
    #[data]
    pub struct AsyncCallbackPoint {
        pub x: i32,
        pub y: i32,
    }

    #[repr(i32)]
    #[data]
    pub enum AsyncCallbackError {
        Unavailable = 1,
    }

    #[export]
    pub trait AsyncListener {
        async fn refresh(&self);
        async fn value(&self, key: u32) -> u32;
        async fn point(&self, point: AsyncCallbackPoint) -> AsyncCallbackPoint;
        async fn values(&self, count: u32) -> Vec<u32>;
        async fn try_load(&self, key: u32) -> Result<String, AsyncCallbackError>;
        async fn check_enabled(&self, key: u32) -> Result<bool, AsyncCallbackError>;
    }
"#;

const ASYNC_RETURNED_CALLBACKS: &str = r#"
    #[repr(C)]
    #[data]
    pub struct ReturnedPoint {
        pub x: i32,
        pub y: i32,
    }

    #[export]
    pub trait ReturnedChild {
        fn on_value(&self, value: u32) -> u32;
    }

    #[export]
    pub trait ReturnedListener {
        async fn name(&self) -> String;
        async fn point(&self) -> ReturnedPoint;
        async fn child(&self) -> Box<dyn ReturnedChild>;
    }

    #[export]
    pub fn make_returned_listener() -> Box<dyn ReturnedListener> { loop {} }
"#;

fn bindings(source: &str) -> boltffi_binding::Bindings<Native> {
    let file = syn::parse_str(source).expect("valid Java source fixture");
    let source = boltffi_scan::scan_file(file, PackageInfo::new("demo", None))
        .expect("Java fixture should scan");
    lower::<Native>(&source).expect("Java fixture should lower")
}

fn host() -> JavaHost {
    JavaHost::new("com.boltffi.demo", "Demo")
        .expect("Java host")
        .desktop_loader(JavaDesktopLoader::None)
}

fn render(source: &str, coverage: CoverageMode) -> boltffi_backend::GeneratedOutput {
    render_with_host(source, coverage, host())
}

fn render_with_host(
    source: &str,
    coverage: CoverageMode,
    host: JavaHost,
) -> boltffi_backend::GeneratedOutput {
    let bindings = bindings(source);
    host.render_with_coverage(&bindings, coverage)
        .expect("Java target should render")
}

fn validate_host(host: JavaHost) -> Result<boltffi_backend::GeneratedOutput, Error> {
    host.render_with_coverage(&bindings(""), CoverageMode::Complete)
}

fn java_source<'output>(
    output: &'output boltffi_backend::GeneratedOutput,
    package: &str,
    file: &str,
) -> &'output str {
    let path = PathBuf::from(package.replace('.', "/")).join(format!("{file}.java"));
    output
        .files()
        .iter()
        .find(|generated| generated.path().as_path() == path)
        .unwrap_or_else(|| {
            panic!(
                "Java target should emit {path:?}; emitted {:?}",
                output
                    .files()
                    .iter()
                    .map(|file| file.path().as_path())
                    .collect::<Vec<_>>()
            )
        })
        .contents()
}

#[test]
fn java_target_renders_primitive_function_stack() {
    insta::assert_snapshot!(java_source(
        &render(PRIMITIVE_FUNCTIONS, CoverageMode::Complete),
        "com.boltffi.demo",
        "Demo"
    ));
}

#[test]
fn java_target_carries_mutable_primitive_slices_as_direct_vectors() {
    let output = render(MUTABLE_PARAMETERS, CoverageMode::Complete);
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(module.contains("public static void increment(long[] values)"));
    assert!(module.contains("static native void boltffi_function_demo_increment(long[] values)"));
    assert!(!module.contains("java.nio.ByteBuffer values"));
    assert!(!module.contains("System.arraycopy"));
    assert!(output.coverage().unsupported().is_empty());
}

#[test]
fn java_target_renders_direct_record_stack() {
    let output = render(DIRECT_RECORD_FUNCTIONS, CoverageMode::Complete);
    insta::assert_snapshot!(
        "java_direct_record_stack",
        format!(
            "===== Demo.java =====\n{}\n===== Point.java =====\n{}",
            java_source(&output, "com.boltffi.demo", "Demo"),
            java_source(&output, "com.boltffi.demo", "Point"),
        )
    );
}

#[test]
fn java_target_renders_direct_record_associated_calls() {
    let output = render(DIRECT_RECORD_CALLS, CoverageMode::Complete);
    let counter = java_source(&output, "com.boltffi.demo", "Counter");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(counter.contains("public static Counter _new(int value)"));
    assert!(counter.contains("public static Counter zero()"));
    assert!(counter.contains("public int current()"));
    assert!(counter.contains("public Counter increment(int amount)"));
    assert!(counter.contains("return Counter.fromDirectBuffer(__boltffi_receiver);"));
    assert!(counter.contains("public Counter added(Counter other)"));
    assert!(counter.contains("other.toDirectBuffer()"));
    assert!(module.contains("static native byte[]"));
    assert!(module.contains("static native int"));
    assert!(module.contains("static native void"));
}

#[test]
fn java_target_renders_encoded_record_fields_through_codec_plans() {
    let output = render(ENCODED_RECORD, CoverageMode::Complete);
    let profile = java_source(&output, "com.boltffi.demo", "Profile");

    assert!(profile.contains("public final class Profile"));
    assert!(profile.contains("String name"));
    assert!(profile.contains("int[] samples"));
    assert!(profile.contains("java.util.Optional<Integer> marker"));
    assert!(profile.contains("reader.readString()"));
    assert!(profile.contains("reader.readIntArray()"));
    assert!(profile.contains("reader.readOptional"));
    assert!(profile.contains("writer.writeString"));
    assert!(profile.contains("writer.writeIntArray"));
    assert!(profile.contains("writer.writeOptional"));
}

#[test]
fn java_target_emits_one_public_result_type_for_result_records() {
    let output = render(RESULT_RECORD, CoverageMode::Complete);
    let holder = java_source(&output, "com.boltffi.demo", "ResultHolder");
    let result = java_source(&output, "com.boltffi.demo", "BoltFFIResult");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(holder.contains("BoltFFIResult<int[], java.util.Optional<String>> outcome"));
    assert!(result.contains("public final class BoltFFIResult<Ok, Err>"));
    assert!(result.contains("public static <Ok, Err> BoltFFIResult<Ok, Err> ok"));
    assert!(result.contains("public boolean isOk()"));
    assert!(result.contains("public String toString()"));
    assert!(!module.contains("class BoltFfiResult"));
    assert_eq!(
        output
            .files()
            .iter()
            .filter(|file| {
                file.path().as_path() == Path::new("com/boltffi/demo/BoltFFIResult.java")
            })
            .count(),
        1
    );
}

#[test]
fn java_target_renders_custom_types_through_their_shared_representations() {
    let output = render(CUSTOM_TYPES, CoverageMode::Complete);
    let event = java_source(&output, "com.boltffi.demo", "Event");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(event.contains("String email"));
    assert!(event.contains("long timestamp"));
    assert!(event.contains("java.util.List<String> attendees"));
    assert!(event.contains("reader.readString()"));
    assert!(event.contains("reader.readLong()"));
    assert!(event.contains("reader.readStringSequence()"));
    assert!(event.contains("writer.writeString(this.email)"));
    assert!(event.contains("writer.writeLong(this.timestamp)"));
    assert!(module.contains("public static String echoEmail(String email)"));
    assert!(module.contains("public static long echoTimestamp(long timestamp)"));
    assert!(module.contains("public static Event echoEvent(Event event)"));
    assert!(module.contains(
        "public static java.util.List<String> echoEmails(java.util.List<String> emails)"
    ));
    assert!(module.contains("public static long[] echoTimestamps(long[] timestamps)"));
    assert!(module.contains(
        "public static java.util.Optional<Long> maybeTimestamp(java.util.Optional<Long> timestamp)"
    ));
    assert!(module.contains("reader.readLongArray()"));
    assert!(module.contains("writer.writeLongArray"));
    assert!(output.coverage().unsupported().is_empty());
}

#[test]
fn java_target_rejects_public_result_file_collisions() {
    let bindings = bindings(RESULT_RECORD);
    let error = JavaHost::new("com.boltffi.demo", "BoltFFIResult")
        .expect("Java result collision host")
        .desktop_loader(JavaDesktopLoader::None)
        .render_with_coverage(&bindings, CoverageMode::Complete)
        .expect_err("public result file should not replace the module file");

    assert_eq!(
        error,
        Error::JavaNameCollision {
            scope: "com.boltffi.demo".to_owned(),
            name: "BoltFFIResult".to_owned(),
        }
    );
}

#[test]
fn java_target_renders_trailing_record_default_constructors() {
    let output = render(RECORD_DEFAULTS, CoverageMode::Complete);
    let config = java_source(&output, "com.boltffi.demo", "ServiceConfig");

    assert!(config.contains("public ServiceConfig(String name)"));
    assert!(config.contains(
        "this(name, 3, \"standard\", java.util.Optional.empty(), java.util.Optional.of(\"https://default\"));"
    ));
    assert!(config.contains("public ServiceConfig(String name, int retries)"));
    assert!(config.contains("* Optional endpoint."));
}

#[test]
fn java_target_uses_class_semantics_for_primitive_array_fields() {
    let output = render_with_host(
        ENCODED_RECORD,
        CoverageMode::Complete,
        JavaHost::for_version("com.boltffi.demo", "Demo", JavaVersion::JAVA_17)
            .expect("Java 17 host")
            .desktop_loader(JavaDesktopLoader::None),
    );
    let profile = java_source(&output, "com.boltffi.demo", "Profile");

    assert!(profile.contains("public final class Profile"));
    assert!(!profile.contains("public record Profile"));
    assert!(profile.contains("java.util.Arrays.equals(this.samples, other.samples)"));
    assert!(profile.contains("java.util.Arrays.hashCode(this.samples)"));
}

#[test]
fn java_target_uses_error_record_messages_for_exceptions() {
    let output = render(ERROR_RECORD, CoverageMode::Complete);
    let error = java_source(&output, "com.boltffi.demo", "AppError");

    assert!(error.contains("public final class AppError extends RuntimeException"));
    assert!(error.contains("super(message);"));
    assert!(error.contains("public AppError getError()"));
}

#[test]
fn java_target_renders_c_style_and_data_enums_from_binding_ir() {
    let output = render(ENUMS, CoverageMode::Complete);
    let mode = java_source(&output, "com.boltffi.demo", "Mode");
    let shape = java_source(&output, "com.boltffi.demo", "Shape");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(mode.contains("public enum Mode"));
    assert!(mode.contains("FAST((byte) (1))"));
    assert!(mode.contains("case (byte) (7): return SLOW;"));
    assert!(module.contains("writeByte"));
    assert!(module.contains("readByte()"));
    assert!(mode.contains("public boolean isFast()"));
    let wide = java_source(&output, "com.boltffi.demo", "WideMode");
    assert!(wide.contains("MAXIMUM(0xFFFFFFFFFFFFFFFFL)"));
    assert!(wide.contains("if (value == 0xFFFFFFFFFFFFFFFFL) return MAXIMUM;"));
    assert!(shape.contains("public abstract class Shape"));
    assert!(shape.contains("public static final class Empty extends Shape"));
    assert!(shape.contains("public static final class Circle extends Shape"));
    assert!(shape.contains("public final String value0;"));
    assert!(shape.contains("writer.writeString(this.value0);"));
    assert!(shape.contains("writer.writeDouble(this.radius);"));
    assert!(shape.contains("reader.readString()"));
    assert!(shape.contains("public boolean isEmpty()"));
    assert!(module.contains("static native byte"));
    assert!(module.contains("static native byte[]"));
}

#[test]
fn java_target_preserves_typed_enum_errors() {
    let output = render(ERROR_ENUMS, CoverageMode::Complete);
    let parse_error = java_source(&output, "com.boltffi.demo", "ParseError");
    let api_error = java_source(&output, "com.boltffi.demo", "ApiError");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(parse_error.contains("public enum ParseError"));
    assert!(parse_error.contains("public static final class Exception extends RuntimeException"));
    assert!(api_error.contains("public abstract class ApiError extends RuntimeException"));
    assert!(api_error.contains("super(message);"));
    assert!(module.contains("throw new ParseError.Exception"));
    assert!(module.contains("throw ApiError.fromReader"));
}

#[test]
fn java_seventeen_uses_sealed_data_enums_when_value_semantics_are_safe() {
    let output = render_with_host(
        ENUMS,
        CoverageMode::Complete,
        JavaHost::for_version("com.boltffi.demo", "Demo", JavaVersion::JAVA_17)
            .expect("Java 17 enum host"),
    );
    let shape = java_source(&output, "com.boltffi.demo", "Shape");

    assert!(shape.contains("public sealed interface Shape permits"));
    assert!(shape.contains("record Circle(double radius) implements Shape"));
    assert!(shape.contains("default boolean isEmpty()"));
}

#[test]
fn java_target_renders_class_ownership_and_handle_calls_from_binding_ir() {
    let output = render(CLASSES, CoverageMode::Complete);
    let counter = java_source(&output, "com.boltffi.demo", "Counter");
    let fallible = java_source(&output, "com.boltffi.demo", "FallibleOnly");
    let factory = java_source(&output, "com.boltffi.demo", "Factory");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(counter.contains("public final class Counter implements AutoCloseable"));
    assert!(counter.contains("private final long handle;"));
    assert!(counter.contains("private final AtomicBoolean closed = new AtomicBoolean(false);"));
    assert!(counter.contains("public Counter(int value)"));
    assert!(counter.contains("private static long __boltffiCreateHandle0(int value)"));
    assert!(counter.contains("return Native.boltffi_init_class_demo_counter_new(value);"));
    assert!(counter.contains("public static Counter tryNew(int value)"));
    assert!(
        counter
            .contains("return new Counter(Native.boltffi_init_class_demo_counter_try_new(value));")
    );
    assert!(counter.contains("throw new RuntimeException(\"Factory constructor failed\")"));
    assert!(counter.contains("if (!closed.compareAndSet(false, true)) return;"));
    assert!(counter.contains("Native.boltffi_release_class_demo_counter(this.handle);"));
    assert!(
        counter
            .contains("if (closed.get()) throw new IllegalStateException(\"Counter is closed\");")
    );
    assert!(counter.contains("public int get()"));
    assert!(counter.contains("public void set(int value)"));
    assert!(
        counter.contains("Native.boltffi_method_class_demo_counter_set(this.rawHandle(), value);")
    );

    assert!(fallible.contains("public FallibleOnly(String name)"));
    assert!(fallible.contains("private static long __boltffiCreateHandle0(String name)"));
    assert!(fallible.contains("catch (BoltFfiErrorBufferException __boltffi_error)"));
    assert!(fallible.contains("throw new RuntimeException(\"Constructor failed\")"));
    assert!(!fallible.contains("this(new FallibleOnly"));

    assert!(factory.contains("public static Counter make(int value)"));
    assert!(
        factory
            .contains("return new Counter(Native.boltffi_method_class_demo_factory_make(value));")
    );
    assert!(factory.contains("public Counter maybe(int value)"));
    assert!(factory.contains(
        "long __boltffi_handle = Native.boltffi_method_class_demo_factory_maybe(this.rawHandle(), value);"
    ));
    assert!(
        factory.contains("return (__boltffi_handle == 0L ? null : new Counter(__boltffi_handle));")
    );
    assert!(factory.contains("public int read(Counter counter)"));
    assert!(factory.contains("counter.rawHandle()"));

    assert!(module.contains("public static String describe(Counter counter)"));
    assert!(module.contains("counter.rawHandle()"));
    assert!(module.contains("public static Counter makeCounter(int value)"));
    assert!(module.contains("public static Counter maybeCounter(int value)"));
    assert!(module.contains("static native long"));
}

#[test]
fn java_target_renders_jvm_owned_callbacks_from_binding_ir() {
    let output = render(CALLBACKS, CoverageMode::Complete);
    let callback = java_source(&output, "com.boltffi.demo", "DataProvider");
    let returned = java_source(&output, "com.boltffi.demo", "ValueCallback");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(callback.contains("public interface DataProvider"));
    assert!(callback.contains("int getCount()"));
    assert!(callback.contains("DataPoint getItem(int index)"));
    assert!(callback.contains("static int get_count(long handle)"));
    assert!(callback.contains("static byte[] get_item(long handle, int index)"));
    assert!(callback.contains("implementation.getItem(index).toByteArray()"));
    assert!(module.contains("public static int consume(DataProvider provider)"));
    assert!(module.contains("DataProviderBridge.create(provider)"));
    assert!(module.contains(
        "public static int invokeOptionalCallback(java.util.Optional<ValueCallback> callback, int value)"
    ));
    assert!(
        module.contains("callback.isPresent() ? ValueCallbackBridge.create(callback.get()) : 0L")
    );
    assert!(module.contains("static native int"));
    assert!(
        returned
            .contains("final class ValueCallbackHandle implements ValueCallback, AutoCloseable")
    );
    assert!(returned.contains("return ((ValueCallbackHandle) value).rawHandle();"));
    assert!(returned.contains("Native.boltffi_callback_handle_release(handle)"));
    assert!(returned.contains(
        "return Native.boltffi_callback_handle_demo_value_callback_apply(this.rawHandle(), value)"
    ));
    assert!(returned.contains(
        "if (closed.get()) throw new IllegalStateException(\"callback handle is closed\");"
    ));
    assert!(module.contains("public static ValueCallback makeCallback(int delta)"));
    assert!(module.contains("return ValueCallbackBridge.wrap"));
}

#[test]
fn java_target_rejects_callback_methods_shadowing_handle_members() {
    let render =
        |source: &str| host().render_with_coverage(&bindings(source), CoverageMode::Complete);

    let error = render(
        r#"
        #[export]
        pub trait Listener {
            fn raw_handle(&self) -> u32;
        }

        #[export]
        pub fn make_listener() -> Box<dyn Listener> {
            loop {}
        }
        "#,
    )
    .expect_err("a callback method shadowing the generated handle members must not render");
    assert!(
        matches!(
            &error,
            Error::JavaNameCollision { scope, name }
                if scope.contains("Listener") && name == "rawHandle()"
        ),
        "{error:?}"
    );

    let error = render(
        r#"
        #[export]
        pub trait Listener {
            fn close(&self);
        }

        #[export]
        pub fn make_listener() -> Box<dyn Listener> {
            loop {}
        }
        "#,
    )
    .expect_err("a callback method shadowing the generated close() must not render");
    assert!(
        matches!(
            &error,
            Error::JavaNameCollision { scope, name }
                if scope.contains("Listener") && name == "close()"
        ),
        "{error:?}"
    );

    render(
        r#"
        #[export]
        pub trait Listener {
            fn raw_handle(&self) -> u32;
        }

        #[export]
        pub fn set_listener(listener: Box<dyn Listener>) {
            let _ = listener;
        }
        "#,
    )
    .expect("callbacks without a handle class keep the reserved names available");
}

#[test]
fn java_target_renders_encoded_and_fallible_callbacks_through_codec_plans() {
    let output = render(ENCODED_CALLBACKS, CoverageMode::Complete);
    let formatter = java_source(&output, "com.boltffi.demo", "MessageFormatter");
    let option = java_source(&output, "com.boltffi.demo", "OptionCallback");
    let result = java_source(&output, "com.boltffi.demo", "ResultCallback");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(formatter.contains("String format(String value)"));
    assert!(formatter.contains("WireReader __boltffi_value_reader"));
    assert!(formatter.contains("writer.writeString(__boltffi_result)"));
    assert!(option.contains("java.util.Optional<Integer> find(int key)"));
    assert!(option.contains("writer.writeOptional"));
    assert!(result.contains("byte[] compute(long handle, long return_out, int value)"));
    assert!(result.contains("Native.boltffi_success_i32(return_out, __boltffi_result)"));
    assert!(result.contains("catch (MathError.Exception __boltffi_error)"));
    assert!(result.contains("__boltffi_error.getError()"));
    assert!(module.contains("static native void boltffi_success_i32(long returnOut, int value)"));
}

#[test]
fn generated_callback_sources_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let source = format!("{CALLBACKS}\n{ENCODED_CALLBACKS}");
    compile_generated_java(
        &compiler,
        &render_with_host(
            &source,
            CoverageMode::Complete,
            JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
        ),
        "boltffi-java-callbacks",
    );
}

#[test]
fn java_target_renders_closures_from_shared_signatures_and_jni_registrations() {
    let output = render(CLOSURES, CoverageMode::Complete);
    let module = java_source(&output, "com.boltffi.demo", "Demo");
    let scalar = java_source(&output, "com.boltffi.demo", "ClosureI32ToI32");
    let record = java_source(&output, "com.boltffi.demo", "ClosureDemoPointToDemoPoint");
    let text = java_source(&output, "com.boltffi.demo", "ClosureStringToString");
    let fallible = java_source(
        &output,
        "com.boltffi.demo",
        "ClosureI32ToResultI32ErrDemoMathError",
    );

    assert!(scalar.contains("public interface ClosureI32ToI32"));
    assert!(scalar.contains("int invoke(int arg0)"));
    assert!(scalar.contains("static int call(long handle, int arg0)"));
    assert!(module.contains("public static int apply(ClosureI32ToI32 callback, int value)"));
    assert!(module.contains("ClosureI32ToI32Callbacks.insert(callback)"));
    assert!(
        record.contains("return implementation.invoke(Point.fromByteArray(arg0)).toByteArray()")
    );
    assert!(text.contains("WireReader __boltffi_arg0_reader"));
    assert!(text.contains("writer.writeString(__boltffi_result)"));
    assert!(fallible.contains("Native.boltffi_success_i32(return_out, __boltffi_result)"));
    assert!(fallible.contains("catch (MathError.Exception __boltffi_error)"));
}

#[test]
fn generated_closure_sources_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    compile_generated_java(
        &compiler,
        &render_with_host(
            CLOSURES,
            CoverageMode::Complete,
            JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
        ),
        "boltffi-java-closures",
    );
}

#[test]
fn java_target_renders_streams_from_shared_protocols_and_item_plans() {
    let output = render(STREAMS, CoverageMode::Complete);
    let event_bus = java_source(&output, "com.boltffi.demo", "EventBus");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(event_bus.contains(
        "public StreamSubscription<Integer> values(java.util.function.Consumer<Integer> callback)"
    ));
    assert!(event_bus.contains("public StreamSubscription<StreamPoint> points("));
    assert!(event_bus.contains("public StreamSubscription<StreamMode> modes("));
    assert!(event_bus.contains("public StreamSubscription<StreamMessage> messages("));
    assert!(
        event_bus.contains("public StreamSubscription<BoltFFIResult<Integer, String>> results(")
    );
    assert!(event_bus.contains("public StreamSubscription<Integer> valueBatches()"));
    assert!(
        event_bus
            .contains("Native.boltffi_stream_demo_event_bus_values_subscribe(this.rawHandle())")
    );
    assert!(!event_bus.contains("subscribe(this.handle)"));
    assert!(event_bus.contains("BoltFfiStreamBatches.ints(bytes)"));
    assert!(event_bus.contains("DirectVectorCodec.readRecords(bytes, 16"));
    assert!(event_bus.contains("BoltFfiStreamBatches.map("));
    assert!(event_bus.contains("reader.readSequence(() -> StreamMessage.fromReader(reader))"));
    assert!(module.contains(
        "static native byte[] boltffi_stream_demo_event_bus_values_pop_batch(long subscription, long maxCount)"
    ));
    assert!(module.contains(
        "static native void boltffi_stream_demo_event_bus_values_poll(long subscription, long callback_data)"
    ));
    assert!(module.contains("final class BoltFfiStream"));
    assert!(module.contains("final class StreamSubscription<T> implements AutoCloseable"));
    assert!(!module.contains("synchronized"));
    assert!(output.coverage().unsupported().is_empty());
}

#[test]
fn generated_stream_sources_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    compile_generated_java(
        &compiler,
        &render_with_host(
            STREAMS,
            CoverageMode::Complete,
            JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
        ),
        "boltffi-java-streams",
    );
}

#[test]
fn generated_stream_runtime_obeys_delivery_and_ownership_contracts() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    compile_and_run_generated_java(
        &compiler,
        &render_with_host(
            STREAMS,
            CoverageMode::Complete,
            JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
        ),
        "boltffi-java-stream-runtime",
        &[(
            "com/boltffi/demo/StreamRuntimeProbe.java",
            STREAM_RUNTIME_PROBE,
        )],
        "com.boltffi.demo.StreamRuntimeProbe",
    );
}

#[test]
fn java_nine_stream_publisher_is_lock_free_and_compiles_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };
    let output = render_with_host(
        STREAMS,
        CoverageMode::Complete,
        JavaHost::for_version("com.boltffi.demo", "Demo", JavaVersion::JAVA_9)
            .expect("Java 9 host"),
    );
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(module.contains("public java.util.concurrent.Flow.Publisher<T> toPublisher()"));
    assert!(module.contains("java.util.concurrent.locks.LockSupport.park(this)"));
    assert!(!module.contains("synchronized"));
    compile_and_run_generated_java_for_release(
        &compiler,
        &output,
        "boltffi-java-stream-flow",
        &[(
            "com/boltffi/demo/StreamFlowRuntimeProbe.java",
            STREAM_FLOW_RUNTIME_PROBE,
        )],
        9,
        "com.boltffi.demo.StreamFlowRuntimeProbe",
    );
}

#[test]
fn generated_async_sources_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    compile_generated_java(
        &compiler,
        &render_with_host(
            ASYNC_FUNCTIONS,
            CoverageMode::Complete,
            JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
        ),
        "boltffi-java-async",
    );
}

#[test]
fn generated_async_runtime_obeys_completion_and_ownership_contracts() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    compile_and_run_generated_java(
        &compiler,
        &render_with_host(
            ASYNC_FUNCTIONS,
            CoverageMode::Complete,
            JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
        ),
        "boltffi-java-async-runtime",
        &[(
            "com/boltffi/demo/AsyncRuntimeProbe.java",
            ASYNC_RUNTIME_PROBE,
        )],
        "com.boltffi.demo.AsyncRuntimeProbe",
    );
}

#[test]
fn java_target_renders_async_callbacks_from_completion_protocols() {
    let output = render(ASYNC_CALLBACKS, CoverageMode::Complete);
    let callback = java_source(&output, "com.boltffi.demo", "AsyncListener");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(callback.contains("java.util.concurrent.CompletableFuture<Void> refresh()"));
    assert!(callback.contains("java.util.concurrent.CompletableFuture<Integer> value(int key)"));
    assert!(callback.contains(
        "static void value(long handle, int key, long callbackToken, long callbackContext)"
    ));
    assert!(callback.contains("__boltffi_future.whenComplete"));
    assert!(callback.contains("BoltFfiCallbackFailure.unwrap"));
    assert!(callback.contains("instanceof AsyncCallbackError.Exception"));
    assert!(module.contains("boltffi_async_callback_complete_U32"));
    assert!(module.contains("boltffi_async_callback_complete_Bytes_error"));
    assert!(output.coverage().unsupported().is_empty());
}

#[test]
fn generated_async_callback_sources_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    compile_generated_java(
        &compiler,
        &render_with_host(
            ASYNC_CALLBACKS,
            CoverageMode::Complete,
            JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
        ),
        "boltffi-java-async-callbacks",
    );
}

#[test]
fn java_target_renders_async_rust_owned_callback_handles() {
    let output = render(ASYNC_RETURNED_CALLBACKS, CoverageMode::Complete);
    let callback = java_source(&output, "com.boltffi.demo", "ReturnedListener");

    assert!(callback.contains("final class ReturnedListenerHandle"));
    assert!(callback.contains("public java.util.concurrent.CompletableFuture<String> name()"));
    assert!(callback.contains("long callback_data = BoltFfiCallbackFutures.insert"));
    assert!(callback.contains("Native.boltffi_callback_handle_demo_returned_listener_name"));
    assert!(callback.contains("static void complete_name(long callbackData, byte[] result)"));
    assert!(callback.contains("static void fail_name(long callbackData)"));
    assert!(callback.contains("BoltFfiCallbackFutures.success"));
    assert!(output.coverage().unsupported().is_empty());
}

#[test]
fn generated_async_rust_owned_callback_sources_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    compile_generated_java(
        &compiler,
        &render_with_host(
            ASYNC_RETURNED_CALLBACKS,
            CoverageMode::Complete,
            JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
        ),
        "boltffi-java-async-returned-callbacks",
    );
}

#[test]
fn java_target_renders_async_functions_and_methods_from_poll_handle_protocols() {
    let output = render(ASYNC_FUNCTIONS, CoverageMode::Complete);
    let module = java_source(&output, "com.boltffi.demo", "Demo");
    let worker = java_source(&output, "com.boltffi.demo", "Worker");

    assert!(
        module.contains("public static java.util.concurrent.CompletableFuture<Void> refresh()")
    );
    assert!(module.contains(
        "public static java.util.concurrent.CompletableFuture<Integer> loadCount(int value)"
    ));
    assert!(
        module.contains("public static java.util.concurrent.CompletableFuture<String> loadName()")
    );
    assert!(module.contains("return BoltFfiAsync.call("));
    assert!(module.contains("boltffi_async_function_demo_refresh_panic_message(future)"));
    assert!(module.contains("BoltFfiAsync.failure(__boltffi_failure, () -> Native."));
    assert!(module.contains("(future, continuation) -> Native."));
    assert!(
        module.contains(
            "static void boltffiFutureContinuationCallback(long handle, byte pollResult)"
        )
    );
    assert!(
        worker.contains("public java.util.concurrent.CompletableFuture<Integer> run(int value)")
    );
    assert!(worker.contains("this.rawHandle()"));
    assert!(output.coverage().unsupported().is_empty());
}

#[test]
fn java_target_rejects_class_lifecycle_signature_collisions() {
    let source = r#"
        pub struct Resource;

        #[export]
        impl Resource {
            pub fn new() -> Self { Self }
            pub fn close(&self) {}
        }
    "#;
    let error = host()
        .render_with_coverage(&bindings(source), CoverageMode::Complete)
        .expect_err("class lifecycle methods must retain their generated signatures");

    assert_eq!(
        error,
        Error::JavaNameCollision {
            scope: "Resource".to_owned(),
            name: "close()".to_owned(),
        }
    );
}

#[test]
fn java_target_renders_data_record_calls_through_shared_jni_carriers() {
    let output = render(ENCODED_RECORD_CALLS, CoverageMode::Complete);
    let point = java_source(&output, "com.boltffi.demo", "Point");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(point.contains("public static Point _new(double x, double y)"));
    assert!(point.contains("public static Point origin()"));
    assert!(point.contains("public static Point tryUnit(double x, double y)"));
    assert!(point.contains("catch (BoltFfiErrorBufferException __boltffi_error)"));
    assert!(point.contains("public static java.util.Optional<Point> checkedUnit"));
    assert!(point.contains("public double distance()"));
    assert!(point.contains("public Point scale(double factor)"));
    assert!(point.contains("public Point add(Point other)"));
    assert!(point.contains("public static double pathLength(java.util.List<Point> points)"));
    assert!(module.contains("public static Point echoPoint(Point point)"));
    assert!(module.contains("WireWriterPool.acquire(point.wireSize())"));
    assert!(module.contains("point.writeTo(__boltffi_point_writer)"));
    assert!(module.contains("__boltffi_point_wire.directBuffer()"));
    assert!(!module.contains("point.toDirectBuffer()"));
}

#[test]
fn java_target_renders_maps_through_shared_codec() {
    let output = render(MAP_FUNCTIONS, CoverageMode::Complete);
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(module.contains(
        "public static java.util.Map<String, Integer> echoScores(java.util.Map<String, Integer> scores)"
    ));
    assert!(module.contains("WireSizes.map(scores,"));
    assert!(module.contains("__boltffi_scores_writer.writeMap(scores,"));
    assert!(module.contains("__boltffi_reader.readMap("));
    assert!(output.coverage().unsupported().is_empty());
}

#[test]
fn java_partial_target_reports_backend_coverage() {
    let source = r#"
        #[export]
        pub fn add(left: i32, right: i32) -> i32 { left + right }

        #[export]
        pub fn increment(values: &mut [u64]) {
            values.iter_mut().for_each(|value| *value += 1);
        }
    "#;
    let bindings = bindings(source);
    let symbol = |name| {
        bindings
            .decls()
            .iter()
            .find_map(|declaration| match DeclarationRef::from(declaration) {
                DeclarationRef::Function(function)
                    if function.name().source_spelling() == Some(name) =>
                {
                    Some(function.symbol().name().as_str())
                }
                _ => None,
            })
            .expect("function source symbol")
    };
    let add_symbol = symbol("add");
    let increment_symbol = symbol("increment");
    let output = host()
        .render_with_coverage(&bindings, CoverageMode::Partial)
        .expect("partial Java target should retain supported bindings");
    let unsupported = output.coverage().unsupported();
    let java = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(java.contains("public static int add(int left, int right)"));
    assert!(java.contains(&format!(
        "static native int {add_symbol}(int left, int right)"
    )));
    assert!(java.contains("public static void increment(long[] values)"));
    assert!(java.contains(&format!(
        "static native void {increment_symbol}(long[] values)"
    )));
    assert!(unsupported.is_empty());
    assert!(
        output
            .files()
            .iter()
            .any(|file| file.contents().contains(add_symbol))
    );
    assert!(
        output
            .files()
            .iter()
            .any(|file| file.contents().contains(increment_symbol))
    );
}

#[test]
fn java_partial_target_retains_dependency_closed_direct_records() {
    let source = r#"
        #[repr(C)]
        #[data]
        pub struct Point {
            pub x: i32,
        }

        #[export]
        pub fn keep_point(point: Point) -> Point { point }

        #[export]
        pub fn add(left: i32, right: i32) -> i32 { left + right }
    "#;
    let bindings = bindings(source);
    let symbol = |name| {
        bindings
            .decls()
            .iter()
            .find_map(|declaration| match DeclarationRef::from(declaration) {
                DeclarationRef::Function(function)
                    if function.name().source_spelling() == Some(name) =>
                {
                    Some(function.symbol().name().as_str())
                }
                _ => None,
            })
            .expect("function source symbol")
    };
    let keep_point_symbol = symbol("keep_point");
    let add_symbol = symbol("add");
    let output = host()
        .render_with_coverage(&bindings, CoverageMode::Partial)
        .expect("dependency-closed Java target");
    let coverage = output.coverage().unsupported();
    let java = java_source(&output, "com.boltffi.demo", "Demo");
    let point = java_source(&output, "com.boltffi.demo", "Point");

    assert!(java.contains("public static int add(int left, int right)"));
    assert!(java.contains("public static Point keepPoint(Point point)"));
    assert!(java.contains("point.toDirectBuffer()"));
    assert!(java.contains("Point.fromByteArray"));
    assert!(point.contains("public final class Point"));
    assert!(point.contains("static final int STRUCT_SIZE = 4;"));
    assert!(
        output
            .files()
            .iter()
            .any(|file| file.contents().contains(add_symbol))
    );
    assert!(
        output
            .files()
            .iter()
            .any(|file| file.contents().contains(keep_point_symbol))
    );
    assert!(coverage.is_empty());
}

#[test]
fn java_partial_target_prunes_direct_and_transitive_interned_strings_before_preflight() {
    let source = r#"
        use boltffi::InternedString;

        boltffi::interned_string_pool! {
            pub BrowserName {
                Chrome = "Chrome",
            }
        }

        #[data]
        pub struct Browser {
            pub name: InternedString<BrowserName>,
        }

        #[data]
        pub enum BrowserKind {
            Named { name: InternedString<BrowserName> },
        }

        #[export]
        pub fn blocked_direct(name: InternedString<BrowserName>) -> InternedString<BrowserName> {
            name
        }

        #[export]
        pub fn blocked_record(browser: Browser) -> Browser { browser }

        #[export]
        pub fn blocked_enum(kind: BrowserKind) -> BrowserKind { kind }

        #[export]
        pub fn add(left: i32, right: i32) -> i32 { left + right }
    "#;
    let bindings = bindings(source);
    let blocked_symbols = bindings
        .decls()
        .iter()
        .filter_map(|declaration| match DeclarationRef::from(declaration) {
            DeclarationRef::Function(function)
                if function.name().source_spelling() != Some("add") =>
            {
                Some(function.symbol().name().as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let output = host()
        .render_with_coverage(&bindings, CoverageMode::Partial)
        .expect("partial Java target prunes InternedString before preflight");
    let java = java_source(&output, "com.boltffi.demo", "Demo");
    let all_output = output
        .files()
        .iter()
        .map(|file| file.contents())
        .collect::<String>();

    assert!(java.contains("public static int add(int left, int right)"));
    assert!(!all_output.contains("BrowserName"));
    assert!(!all_output.contains("BrowserKind"));
    assert!(!all_output.contains("blockedDirect"));
    assert!(!all_output.contains("blockedRecord"));
    assert!(!all_output.contains("blockedEnum"));
    for symbol in blocked_symbols {
        assert!(!all_output.contains(symbol));
    }

    let expected_unsupported = bindings
        .decls()
        .iter()
        .filter(|declaration| {
            !matches!(
                DeclarationRef::from(*declaration),
                DeclarationRef::Function(function)
                    if function.name().source_spelling() == Some("add")
            )
        })
        .map(|declaration| {
            let label = DeclarationLabel::from_ref(DeclarationRef::from(declaration));
            (
                label.kind(),
                label.name().to_owned(),
                "capability was not advertised",
            )
        })
        .collect::<Vec<_>>();
    let unsupported = output
        .coverage()
        .unsupported()
        .iter()
        .map(|unsupported| {
            (
                unsupported.declaration().kind(),
                unsupported.declaration().name().to_owned(),
                unsupported.reason(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(unsupported, expected_unsupported);
}

#[test]
fn java_partial_target_rejects_async_closure_returns_before_building_the_bridge() {
    let source = r#"
        #[export]
        pub fn add(left: i32, right: i32) -> i32 { left + right }

        #[export]
        pub async fn make_counter() -> impl Fn(u32) -> u32 {
            |value| value + 1
        }
    "#;
    let bindings = bindings(source);
    let symbols = bindings
        .decls()
        .iter()
        .filter_map(|declaration| match DeclarationRef::from(declaration) {
            DeclarationRef::Function(function) => Some((
                function
                    .name()
                    .source_spelling()
                    .expect("function source spelling"),
                function.symbol().name().as_str(),
            )),
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let add_symbol = symbols["add"];
    let rejected_symbol = symbols["make_counter"];

    let error = host()
        .render_with_coverage(&bindings, CoverageMode::Complete)
        .expect_err("complete Java target should preserve the JNI rejection");
    assert_eq!(
        error,
        Error::UnsupportedBridge {
            bridge: "jni",
            shape: "closure return out-pointer on a native method",
        }
    );

    let output = host()
        .render_with_coverage(&bindings, CoverageMode::Partial)
        .expect("partial Java target should remove the JNI rejection");
    let coverage = output.coverage().unsupported();
    let java = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(java.contains("public static int add(int left, int right)"));
    assert!(
        output
            .files()
            .iter()
            .any(|file| file.contents().contains(add_symbol))
    );
    assert!(
        output
            .files()
            .iter()
            .all(|file| !file.contents().contains(rejected_symbol))
    );
    assert_eq!(coverage.len(), 1);
    assert_eq!(coverage[0].declaration().kind(), "function");
    assert_eq!(coverage[0].declaration().name(), "make::counter");
    assert_eq!(coverage[0].reason(), "closure function return");
}

#[test]
fn java_target_renders_encoded_byte_functions_in_complete_coverage() {
    let output = render(
        r#"
        #[export]
        pub fn echo_bytes(value: Vec<u8>) -> Vec<u8> { value }
        "#,
        CoverageMode::Complete,
    );
    let java = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(java.contains("public static byte[] echoBytes(byte[] value)"));
    assert!(java.contains("WireWriterPool.acquire((4 + value.length))"));
    assert!(java.contains("reader.readBytes()"));
    assert!(output.coverage().unsupported().is_empty());
}

#[test]
fn java_target_specializes_string_sequences_without_codec_callbacks() {
    let output = render(
        r#"
        #[export]
        pub fn echo_strings(value: Vec<String>) -> Vec<String> { value }
        "#,
        CoverageMode::Complete,
    );
    let java = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(java.contains("WireSizes.stringSequence(value)"));
    assert!(java.contains("writer.writeStringSequence(value)"));
    assert!(java.contains("reader.readStringSequence()"));
    assert!(!java.contains("writer.writeSequence(value"));
}

#[test]
fn java_target_rejects_generated_name_collisions() {
    assert!(matches!(
        JavaHost::new("com.boltffi.demo", "Native"),
        Err(Error::JavaNameCollision { scope, name })
            if scope == "com.boltffi.demo" && name == "Native"
    ));
    assert!(matches!(
        JavaHost::new("com.boltffi.demo", "BoltFFINativeRuntime"),
        Err(Error::JavaNameCollision { scope, name })
            if scope == "com.boltffi.demo" && name == "BoltFFINativeRuntime"
    ));
    assert!(JavaHost::new("com.boltffi.demo", "boltffinativeruntime").is_err());
    assert!(JavaHost::new("com.boltffi.demo", "BoltffiNativeRuntime").is_err());
    let collision = validate_host(
        JavaHost::new("com.boltffi.demo", "Demo")
            .unwrap()
            .desktop_jni_library("NativeCore")
            .unwrap()
            .desktop_fallback_library("nativecore")
            .unwrap(),
    )
    .expect_err("case-insensitive native library paths must not collide");
    assert_eq!(
        collision,
        Error::JavaNameCollision {
            scope: "desktop native libraries".to_owned(),
            name: "nativecore".to_owned(),
        }
    );
    assert!(
        validate_host(
            JavaHost::new("com.boltffi.demo", "Demo")
                .unwrap()
                .desktop_fallback_library("NativeCore")
                .unwrap()
                .desktop_jni_library("nativecore")
                .unwrap(),
        )
        .is_err()
    );
    validate_host(
        JavaHost::new("com.boltffi.demo", "Demo")
            .unwrap()
            .desktop_jni_library("NativeCore")
            .unwrap()
            .desktop_fallback_library("NativeCore")
            .unwrap(),
    )
    .expect("one shared native library should remain valid");
    validate_host(
        JavaHost::new("com.boltffi.demo", "Demo")
            .unwrap()
            .desktop_jni_library("BOLTFFI")
            .unwrap()
            .desktop_fallback_library("BOLTFFI_JNI")
            .unwrap(),
    )
    .expect("final native library pair should validate after both setters");
    validate_host(
        JavaHost::new("com.boltffi.demo", "Demo")
            .unwrap()
            .desktop_fallback_library("BOLTFFI_JNI")
            .unwrap()
            .desktop_jni_library("BOLTFFI")
            .unwrap(),
    )
    .expect("final native library pair should be independent of setter order");

    let bindings = bindings(
        r#"
        #[export]
        pub fn collision(class: i32, class_: i32) -> i32 { class + class_ }
        "#,
    );
    let error = host()
        .render_with_coverage(&bindings, CoverageMode::Complete)
        .expect_err("escaped Java parameter names must remain unique");

    assert!(
        matches!(
            &error,
            Error::JavaNameCollision { scope, name }
            if scope == "boltffi_function_demo_collision" && name == "_class"
        ),
        "{error:?}"
    );
}

#[test]
fn java_target_rejects_static_signatures_inherited_from_object() {
    let bindings = bindings(
        r#"
        #[export]
        pub fn wait() {}
        "#,
    );
    let error = host()
        .render_with_coverage(&bindings, CoverageMode::Complete)
        .expect_err("static Java methods cannot replace Object instance methods");

    assert!(matches!(
        error,
        Error::JavaNameCollision { scope, name }
            if scope.contains("java.lang.Object") && name == "wait()"
    ));
}

#[test]
fn java_target_enforces_jvm_method_parameter_slots() {
    let overflow_parameters = std::iter::repeat_n("i64", 128)
        .enumerate()
        .map(|(index, ty)| format!("value_{index}: {ty}"))
        .collect::<Vec<_>>()
        .join(", ");
    let overflow = bindings(&format!(
        "#[export] pub fn overflow({overflow_parameters}) {{}}"
    ));
    let error = host()
        .render_with_coverage(&overflow, CoverageMode::Complete)
        .expect_err("128 wide parameters must exceed the JVM method limit");

    assert_eq!(
        error,
        Error::UnsupportedTarget {
            target: "jvm",
            shape: "method parameter slots exceed 255 units",
        }
    );

    let overflow_symbol = overflow
        .decls()
        .iter()
        .find_map(|declaration| match DeclarationRef::from(declaration) {
            DeclarationRef::Function(function) => Some(function.symbol().name().as_str()),
            _ => None,
        })
        .expect("overflow function symbol");
    let partial = host()
        .render_with_coverage(&overflow, CoverageMode::Partial)
        .expect("partial Java coverage must remove an invalid JVM descriptor");
    let unsupported = partial.coverage().unsupported();

    assert_eq!(unsupported.len(), 1);
    assert_eq!(
        unsupported[0].reason(),
        "method parameter slots exceed 255 units"
    );
    assert!(
        partial
            .files()
            .iter()
            .all(|file| !file.contents().contains(overflow_symbol))
    );

    let boundary_parameters = std::iter::repeat_n("i64", 127)
        .chain(std::iter::once("i32"))
        .enumerate()
        .map(|(index, ty)| format!("value_{index}: {ty}"))
        .collect::<Vec<_>>()
        .join(", ");
    let boundary = render(
        &format!("#[export] pub fn boundary({boundary_parameters}) {{}}"),
        CoverageMode::Complete,
    );

    assert!(
        java_source(&boundary, "com.boltffi.demo", "Demo").contains("public static void boundary(")
    );
}

#[test]
fn java_target_preserves_contextual_method_and_parameter_names() {
    let source = r#"
        #[export]
        pub fn record(sealed: i32, module: i32) -> i32 { sealed + module }
    "#;
    let output = render_with_host(
        source,
        CoverageMode::Complete,
        JavaHost::new("com.module.demo", "Demo")
            .expect("contextual Java identifiers")
            .version(JavaVersion::JAVA_17)
            .expect("Java 17 host")
            .desktop_loader(JavaDesktopLoader::None),
    );

    assert!(
        java_source(&output, "com.module.demo", "Demo")
            .contains("public static int record(int sealed, int module)")
    );
}

#[test]
fn java_target_derives_names_from_binding_ir_parts() {
    let source = r#"
        #[export]
        pub fn HTTPRequest(r#type: i32) -> i32 { r#type }
    "#;
    let output = render(source, CoverageMode::Complete);
    let java = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(java.contains("public static int httpRequest(int type)"));
}

#[test]
fn java_target_renders_safe_function_javadoc() {
    let source = r#"
        #[doc = "Ends */ safely."]
        #[doc = "Unicode escape \\u002a\\u002f stays text."]
        #[export]
        pub fn documented(value: i32) -> i32 { value }
    "#;
    let output = render(source, CoverageMode::Complete);
    let java = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(java.contains("Ends *&#47; safely."));
    assert!(java.contains("Unicode escape &#92;u002a&#92;u002f stays text."));
    assert!(java.contains("public static int documented(int value)"));
}

#[test]
fn java_host_validates_versioned_unicode_names() {
    let host = JavaHost::new("com.δοκιμή.module", "東京")
        .expect("Unicode Java names")
        .version(JavaVersion::JAVA_17)
        .expect("supported Java version");
    assert_eq!(host.java_version().release(), 17);
    assert_eq!(host.package().to_string(), "com.δοκιμή.module");
    assert_eq!(host.file().as_str(), "東京");

    let modern = JavaHost::for_version("com.𞤀.demo", "𞤀Bindings", JavaVersion::JAVA_17)
        .expect("Java 17 Unicode names");
    assert_eq!(modern.package().to_string(), "com.𞤀.demo");
    assert_eq!(modern.file().as_str(), "𞤀Bindings");

    assert!(
        JavaHost::new("com.\u{1885}.demo", "Demo")
            .unwrap()
            .version(JavaVersion::JAVA_17)
            .is_err()
    );

    assert!(JavaVersion::new(7).is_none());
    assert!(JavaVersion::new(27).is_none());
    assert!(
        JavaHost::new("com.boltffi.demo", "record")
            .unwrap()
            .version(JavaVersion::JAVA_16)
            .is_err()
    );
    assert!(
        JavaHost::new("com._.demo", "Demo")
            .unwrap()
            .version(JavaVersion::JAVA_9)
            .is_err()
    );
}

#[test]
fn generated_primitive_java_compiles_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        PRIMITIVE_FUNCTIONS,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-primitives");
}

#[test]
fn generated_mutable_parameters_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        MUTABLE_PARAMETERS,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-mutable-parameters");
}

#[test]
fn generated_direct_record_java_compiles_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        DIRECT_RECORD_FUNCTIONS,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-direct-records");
}

#[test]
fn generated_direct_record_calls_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        DIRECT_RECORD_CALLS,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-direct-record-calls");
}

#[test]
fn generated_encoded_record_compiles_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        ENCODED_RECORD,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-encoded-record");
}

#[test]
fn generated_custom_types_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        CUSTOM_TYPES,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-custom-types");
}

#[test]
fn generated_encoded_record_calls_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        ENCODED_RECORD_CALLS,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-encoded-record-calls");
}

#[test]
fn generated_maps_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        MAP_FUNCTIONS,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-map");
}

#[test]
fn generated_record_defaults_and_errors_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        &format!("{RECORD_DEFAULTS}\n{ERROR_RECORD}"),
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-record-semantics");
}

#[test]
fn generated_enums_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        ENUMS,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-enums");
}

#[test]
fn generated_sealed_enums_compile_for_java_seventeen_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        ENUMS,
        CoverageMode::Complete,
        JavaHost::for_version("com.boltffi.demo", "Demo", JavaVersion::JAVA_17)
            .expect("Java 17 enum host"),
    );
    compile_generated_java_for_release(&compiler, &output, "boltffi-java-sealed-enums", 17);
}

#[test]
fn generated_enum_errors_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        ERROR_ENUMS,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-enum-errors");
}

#[test]
fn generated_classes_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        CLASSES,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java_with(
        &compiler,
        &output,
        "boltffi-java-classes",
        &[(
            "consumer/ClassConsumer.java",
            r#"
                package consumer;

                import com.boltffi.demo.Counter;
                import com.boltffi.demo.Demo;
                import com.boltffi.demo.Factory;
                import com.boltffi.demo.FallibleOnly;

                public final class ClassConsumer {
                    private ClassConsumer() {}

                    public static int exercise() {
                        try (
                            Counter counter = new Counter(3);
                            Factory factory = new Factory();
                            FallibleOnly opened = new FallibleOnly("ready");
                            Counter made = factory.make(4)
                        ) {
                            counter.set(factory.read(made));
                            return counter.get()
                                + Demo.describe(counter).length()
                                + opened.name().length();
                        }
                    }
                }
            "#,
        )],
    );
}

#[test]
fn generated_result_api_compiles_from_an_external_package_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        RESULT_RECORD,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java_with(
        &compiler,
        &output,
        "boltffi-java-result-api",
        &[(
            "consumer/ResultConsumer.java",
            r#"
                package consumer;

                import com.boltffi.demo.BoltFFIResult;
                import com.boltffi.demo.ResultHolder;

                public final class ResultConsumer {
                    private ResultConsumer() {}

                    public static ResultHolder create() {
                        return new ResultHolder(
                            BoltFFIResult.<int[], java.util.Optional<String>>ok(
                                new int[] {1, 2, 3}
                            )
                        );
                    }

                    public static boolean inspect(
                        BoltFFIResult<int[], java.util.Optional<String>> result
                    ) {
                        return result.isOk()
                            && result.okValue().length == 3
                            && result.errValue() == null;
                    }
                }
            "#,
        )],
    );
}

fn compile_generated_java(
    compiler: &JavaCompiler,
    output: &boltffi_backend::GeneratedOutput,
    prefix: &str,
) {
    compile_generated_java_with(compiler, output, prefix, &[]);
}

fn compile_generated_java_with(
    compiler: &JavaCompiler,
    output: &boltffi_backend::GeneratedOutput,
    prefix: &str,
    additional_sources: &[(&str, &str)],
) {
    compile_generated_java_with_release(compiler, output, prefix, additional_sources, None);
}

fn compile_generated_java_for_release(
    compiler: &JavaCompiler,
    output: &boltffi_backend::GeneratedOutput,
    prefix: &str,
    release: u16,
) {
    compile_generated_java_with_release(compiler, output, prefix, &[], Some(release));
}

fn compile_generated_java_with_release(
    compiler: &JavaCompiler,
    output: &boltffi_backend::GeneratedOutput,
    prefix: &str,
    additional_sources: &[(&str, &str)],
    release: Option<u16>,
) {
    compile_generated_java_with_release_and_main(
        compiler,
        output,
        prefix,
        additional_sources,
        release,
        None,
    );
}

fn compile_and_run_generated_java(
    compiler: &JavaCompiler,
    output: &boltffi_backend::GeneratedOutput,
    prefix: &str,
    additional_sources: &[(&str, &str)],
    main_class: &str,
) {
    compile_generated_java_with_release_and_main(
        compiler,
        output,
        prefix,
        additional_sources,
        None,
        Some(main_class),
    );
}

fn compile_and_run_generated_java_for_release(
    compiler: &JavaCompiler,
    output: &boltffi_backend::GeneratedOutput,
    prefix: &str,
    additional_sources: &[(&str, &str)],
    release: u16,
    main_class: &str,
) {
    compile_generated_java_with_release_and_main(
        compiler,
        output,
        prefix,
        additional_sources,
        Some(release),
        Some(main_class),
    );
}

fn compile_generated_java_with_release_and_main(
    compiler: &JavaCompiler,
    output: &boltffi_backend::GeneratedOutput,
    prefix: &str,
    additional_sources: &[(&str, &str)],
    release: Option<u16>,
    main_class: Option<&str>,
) {
    assert!(output.files().iter().any(|file| {
        file.path().as_path() == Path::new("com/boltffi/demo/BoltFFINativeRuntime.java")
    }));
    let generated = output
        .files()
        .iter()
        .filter(|file| {
            file.path()
                .as_path()
                .extension()
                .is_some_and(|ext| ext == "java")
        })
        .collect::<Vec<_>>();
    assert!(!generated.is_empty(), "Java target should emit source");
    let directory = temporary_directory(prefix);
    let classes = directory.join("classes");
    let mut sources = generated
        .into_iter()
        .map(|generated| {
            let source = directory.join(generated.path().as_path());
            fs::create_dir_all(source.parent().expect("generated Java parent"))
                .expect("create generated Java package");
            fs::write(&source, generated.contents()).expect("write generated Java source");
            source
        })
        .collect::<Vec<_>>();
    sources.extend(additional_sources.iter().map(|(path, contents)| {
        let source = directory.join(path);
        fs::create_dir_all(source.parent().expect("additional Java parent"))
            .expect("create additional Java package");
        fs::write(&source, contents).expect("write additional Java source");
        source
    }));
    fs::create_dir_all(&classes).expect("create Java classes directory");

    let mut javac = Command::new("javac");
    javac.args(["-encoding", "UTF-8"]);
    match release {
        Some(release) if !compiler.configure_release(&mut javac, release) => {
            fs::remove_dir_all(&directory).expect("remove unsupported Java release test directory");
            return;
        }
        Some(_) => {}
        None => compiler.configure_java_eight(&mut javac),
    }
    let compilation = javac
        .arg("-d")
        .arg(&classes)
        .args(&sources)
        .output()
        .expect("javac should execute");

    assert!(
        compilation.status.success(),
        "generated Java failed to compile:\n{}",
        String::from_utf8_lossy(&compilation.stderr)
    );
    if let Some(main_class) = main_class {
        let execution = Command::new("java")
            .arg("-cp")
            .arg(&classes)
            .arg(main_class)
            .output()
            .expect("generated Java should execute");
        assert!(
            execution.status.success(),
            "generated Java failed to execute:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
    fs::remove_dir_all(&directory).expect("remove generated Java test directory");
}

#[test]
fn selects_java_eight_compiler_flags_from_javac_versions() {
    assert_eq!(
        JavaEightCompilation::from_version_output("javac 1.8.0_402"),
        Some(JavaEightCompilation::SourceAndTarget)
    );
    assert_eq!(
        JavaEightCompilation::from_version_output("javac 17.0.12"),
        Some(JavaEightCompilation::Release)
    );
    assert_eq!(
        JavaEightCompilation::from_version_output("javac 26-ea"),
        Some(JavaEightCompilation::Release)
    );
}

fn temporary_directory(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{nonce}", std::process::id()))
}
