use boltffi::*;
use demo_bench_macros::benchmark_candidate;

/// A 2D point with double-precision coordinates.
#[data]
#[benchmark_candidate(record, uniffi)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Point {
    /// Horizontal position.
    pub x: f64,
    /// Vertical position.
    pub y: f64,
}

#[data(impl)]
impl Point {
    #[demo_bench_macros::demo_case(
        "records.blittable.point.should_construct_with_static_new",
        description = "Point::new returns a blittable Point containing the provided coordinates.",
        exclude(
            csharp,
            reason = "The C# demo tests do not currently cover the Point::new constructor."
        ),
        exclude(
            java,
            reason = "The Java demo tests use the generated Point data constructor rather than the Point::new method."
        )
    )]
    pub fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    #[demo_bench_macros::demo_case(
        "records.blittable.point.should_return_origin",
        description = "Point::origin returns a Point at zero coordinates."
    )]
    pub fn origin() -> Self {
        Point { x: 0.0, y: 0.0 }
    }

    #[demo_bench_macros::demo_case(
        "records.blittable.point.should_construct_from_polar_coordinates",
        description = "Point::from_polar converts polar coordinates into Cartesian point fields."
    )]
    pub fn from_polar(r: f64, theta: f64) -> Self {
        Point {
            x: r * theta.cos(),
            y: r * theta.sin(),
        }
    }

    #[demo_bench_macros::demo_case(
        "records.blittable.point.should_normalize_unit_vector",
        description = "Point::try_unit returns a normalized Point for non-zero coordinates.",
        exclude(
            csharp,
            reason = "The C# demo tests do not currently cover the fallible Point::try_unit method."
        ),
        exclude(
            python,
            reason = "The Python demo tests do not currently cover fallible Point methods."
        )
    )]
    #[demo_bench_macros::demo_case(
        "records.blittable.point.should_reject_zero_unit_vector",
        description = "Point::try_unit rejects zero coordinates instead of returning an invalid unit vector.",
        exclude(
            csharp,
            reason = "The C# demo tests do not currently cover the fallible Point::try_unit method."
        ),
        exclude(
            python,
            reason = "The Python demo tests do not currently cover fallible Point methods."
        )
    )]
    pub fn try_unit(x: f64, y: f64) -> Result<Self, String> {
        let len = (x * x + y * y).sqrt();
        if len == 0.0 {
            Err("cannot normalize zero vector".to_string())
        } else {
            Ok(Point {
                x: x / len,
                y: y / len,
            })
        }
    }

    #[demo_bench_macros::demo_case(
        "records.blittable.point.should_return_some_for_checked_unit",
        description = "Point::checked_unit returns Some normalized Point for non-zero coordinates.",
        exclude(
            csharp,
            reason = "The C# demo tests do not currently cover the optional Point::checked_unit method."
        ),
        exclude(
            python,
            reason = "The Python demo tests do not currently cover optional Point methods."
        )
    )]
    #[demo_bench_macros::demo_case(
        "records.blittable.point.should_return_none_for_zero_checked_unit",
        description = "Point::checked_unit returns None for zero coordinates.",
        exclude(
            csharp,
            reason = "The C# demo tests do not currently cover the optional Point::checked_unit method."
        ),
        exclude(
            python,
            reason = "The Python demo tests do not currently cover optional Point methods."
        )
    )]
    pub fn checked_unit(x: f64, y: f64) -> Option<Self> {
        let len = (x * x + y * y).sqrt();
        if len == 0.0 {
            None
        } else {
            Some(Point {
                x: x / len,
                y: y / len,
            })
        }
    }

    #[demo_bench_macros::demo_case(
        "records.blittable.point.should_compute_distance",
        description = "Point::distance computes the Euclidean distance from the origin."
    )]
    pub fn distance(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    #[demo_bench_macros::demo_case(
        "records.blittable.point.should_scale_coordinates",
        description = "Point::scale multiplies both coordinates by the provided factor.",
        exclude(
            csharp,
            reason = "The C# demo tests do not currently cover the Point::scale method."
        )
    )]
    pub fn scale(&mut self, factor: f64) {
        self.x *= factor;
        self.y *= factor;
    }

    #[demo_bench_macros::demo_case(
        "records.blittable.point.should_add_coordinates",
        description = "Point::add returns a Point whose coordinates are the pairwise sums."
    )]
    pub fn add(&self, other: Point) -> Point {
        Point {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }

    #[demo_bench_macros::demo_case(
        "records.blittable.point.should_compute_path_length",
        description = "Point::path_length sums the segment lengths across a vector of Points.",
        exclude(
            python,
            reason = "The Python demo tests do not currently cover Point::path_length."
        )
    )]
    pub fn path_length(points: Vec<Point>) -> f64 {
        points
            .windows(2)
            .map(|pair| {
                let dx = pair[1].x - pair[0].x;
                let dy = pair[1].y - pair[0].y;
                (dx * dx + dy * dy).sqrt()
            })
            .sum()
    }

    #[demo_bench_macros::demo_case(
        "records.blittable.point.should_report_dimension_count",
        description = "Point::dimensions reports the fixed two-dimensional shape of Point."
    )]
    pub fn dimensions() -> u32 {
        2
    }
}

#[demo_bench_macros::demo_case(
    "records.blittable.point.should_roundtrip_value",
    description = "A blittable Point crosses the wire and returns unchanged."
)]
#[export]
pub fn echo_point(p: Point) -> Point {
    p
}

#[demo_bench_macros::demo_case(
    "records.blittable.point.should_return_some_for_nonzero_coordinates",
    description = "try_make_point returns Some Point when the provided coordinates are not both zero.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover try_make_point."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover try_make_point."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover try_make_point."
    )
)]
#[demo_bench_macros::demo_case(
    "records.blittable.point.should_return_none_for_origin_coordinates",
    description = "try_make_point returns None when both coordinates are zero.",
    exclude(
        csharp,
        reason = "The C# demo tests do not currently cover try_make_point."
    ),
    exclude(
        java,
        reason = "The Java demo tests do not currently cover try_make_point."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover try_make_point."
    )
)]
#[export]
pub fn try_make_point(x: f64, y: f64) -> Option<Point> {
    if x == 0.0 && y == 0.0 {
        None
    } else {
        Some(Point { x, y })
    }
}

#[demo_bench_macros::demo_case(
    "records.blittable.point.should_make_from_coordinates",
    description = "make_point returns a blittable Point containing the provided coordinates."
)]
#[export]
#[benchmark_candidate(function, uniffi)]
pub fn make_point(x: f64, y: f64) -> Point {
    Point { x, y }
}

#[demo_bench_macros::demo_case(
    "records.blittable.point.should_add_values",
    description = "add_points returns a blittable Point whose fields are the pairwise coordinate sums."
)]
#[export]
pub fn add_points(a: Point, b: Point) -> Point {
    Point {
        x: a.x + b.x,
        y: a.y + b.y,
    }
}

#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[demo_bench_macros::demo_case(
    "records.blittable.color.should_roundtrip_value",
    description = "A blittable Color crosses the wire and returns unchanged.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover Color records."
    )
)]
#[export]
pub fn echo_color(c: Color) -> Color {
    c
}

#[demo_bench_macros::demo_case(
    "records.blittable.color.should_make_from_channels",
    description = "make_color returns a Color containing the provided channel values.",
    exclude(
        java,
        reason = "The Java demo tests do not currently cover Color records."
    )
)]
#[export]
pub fn make_color(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

/// A benchmark-friendly location record containing only primitive fields.
#[benchmark_candidate(record, uniffi, wasm_bindgen)]
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Location {
    pub id: i64,
    pub lat: f64,
    pub lng: f64,
    pub rating: f64,
    pub review_count: i32,
    pub is_open: bool,
}

/// A benchmark-friendly trade record with dense numeric fields.
#[benchmark_candidate(record, uniffi, wasm_bindgen)]
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Trade {
    pub id: i64,
    pub symbol_id: i32,
    pub price: f64,
    pub quantity: i64,
    pub bid: f64,
    pub ask: f64,
    pub volume: i64,
    pub timestamp: i64,
    pub is_buy: bool,
}

/// A densely packed physics particle used for payload-heavy benchmarks.
#[benchmark_candidate(record, uniffi, wasm_bindgen)]
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Particle {
    pub id: i64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub vx: f64,
    pub vy: f64,
    pub vz: f64,
    pub mass: f64,
    pub charge: f64,
    pub active: bool,
}

/// A dense sensor record used for structured benchmark payloads.
#[benchmark_candidate(record, uniffi, wasm_bindgen)]
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct SensorReading {
    pub sensor_id: i64,
    pub timestamp: i64,
    pub temperature: f64,
    pub humidity: f64,
    pub pressure: f64,
    pub light: f64,
    pub battery: f64,
    pub signal_strength: i32,
    pub is_valid: bool,
}

/// A timestamped data point used by callback and object benchmarks.
#[benchmark_candidate(record, uniffi, wasm_bindgen)]
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct DataPoint {
    pub x: f64,
    pub y: f64,
    pub timestamp: i64,
}

#[benchmark_candidate(impl, wasm_bindgen, constructor = "new")]
impl Location {
    pub fn new(id: i64, lat: f64, lng: f64, rating: f64, review_count: i32, is_open: bool) -> Self {
        Self {
            id,
            lat,
            lng,
            rating,
            review_count,
            is_open,
        }
    }
}

#[benchmark_candidate(impl, wasm_bindgen, constructor = "new")]
impl Trade {
    pub fn new(
        id: i64,
        symbol_id: i32,
        price: f64,
        quantity: i64,
        bid: f64,
        ask: f64,
        volume: i64,
        timestamp: i64,
        is_buy: bool,
    ) -> Self {
        Self {
            id,
            symbol_id,
            price,
            quantity,
            bid,
            ask,
            volume,
            timestamp,
            is_buy,
        }
    }
}

#[benchmark_candidate(impl, wasm_bindgen, constructor = "new")]
impl Particle {
    pub fn new(
        id: i64,
        x: f64,
        y: f64,
        z: f64,
        vx: f64,
        vy: f64,
        vz: f64,
        mass: f64,
        charge: f64,
        active: bool,
    ) -> Self {
        Self {
            id,
            x,
            y,
            z,
            vx,
            vy,
            vz,
            mass,
            charge,
            active,
        }
    }
}

#[benchmark_candidate(impl, wasm_bindgen, constructor = "new")]
impl SensorReading {
    pub fn new(
        sensor_id: i64,
        timestamp: i64,
        temperature: f64,
        humidity: f64,
        pressure: f64,
        light: f64,
        battery: f64,
        signal_strength: i32,
        is_valid: bool,
    ) -> Self {
        Self {
            sensor_id,
            timestamp,
            temperature,
            humidity,
            pressure,
            light,
            battery,
            signal_strength,
            is_valid,
        }
    }
}

#[benchmark_candidate(impl, wasm_bindgen, constructor = "new")]
impl DataPoint {
    pub fn new(x: f64, y: f64, timestamp: i64) -> Self {
        Self { x, y, timestamp }
    }
}

#[demo_bench_macros::demo_case(
    "records.blittable.locations.should_generate_sample_vector",
    description = "generate_locations returns the requested number of Location records.",
    exclude(
        wasm,
        reason = "The Wasm demo tests do not currently cover blittable record vector helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn generate_locations(count: i32) -> Vec<Location> {
    (0..count)
        .map(|index| Location {
            id: i64::from(index),
            lat: 37.7749 + f64::from(index) * 0.001,
            lng: -122.4194 + f64::from(index) * 0.001,
            rating: 3.0 + f64::from(index % 20) * 0.1,
            review_count: 10 + index * 5,
            is_open: index % 2 == 0,
        })
        .collect()
}

#[demo_bench_macros::demo_case(
    "records.blittable.locations.should_count_vector_items",
    description = "process_locations receives a vector of Location records and returns its item count.",
    exclude(
        wasm,
        reason = "The Wasm demo tests do not currently cover blittable record vector helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[demo_bench_macros::demo_case(
    "records.blittable.locations.should_count_empty_vector",
    description = "process_locations treats an empty Location vector as count zero.",
    exclude(
        apple,
        reason = "The Apple blittable vector demo does not currently cover the empty Location vector count."
    ),
    exclude(
        java,
        reason = "The Java blittable vector demo does not currently cover the empty Location vector count."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin blittable vector demo does not currently cover the empty Location vector count."
    ),
    exclude(
        wasm,
        reason = "The Wasm demo tests do not currently cover blittable record vector helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[demo_bench_macros::demo_case(
    "records.blittable.locations.should_count_host_constructed_vector",
    description = "process_locations receives host-constructed Location records and returns their item count.",
    exclude(
        apple,
        reason = "The Apple blittable vector demo does not currently cover host-constructed Location vectors."
    ),
    exclude(
        java,
        reason = "The Java blittable vector demo does not currently cover host-constructed Location vectors."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin blittable vector demo does not currently cover host-constructed Location vectors."
    ),
    exclude(
        wasm,
        reason = "The Wasm demo tests do not currently cover blittable record vector helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn process_locations(locations: Vec<Location>) -> i32 {
    locations.len() as i32
}

#[demo_bench_macros::demo_case(
    "records.blittable.locations.should_sum_generated_ratings",
    description = "sum_ratings receives generated Location records and sums their f64 rating fields.",
    exclude(
        wasm,
        reason = "The Wasm demo tests do not currently cover blittable record vector helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[demo_bench_macros::demo_case(
    "records.blittable.locations.should_sum_host_constructed_ratings",
    description = "sum_ratings receives host-constructed Location records and sums their f64 rating fields.",
    exclude(
        apple,
        reason = "The Apple blittable vector demo does not currently cover host-constructed Location vectors."
    ),
    exclude(
        java,
        reason = "The Java blittable vector demo does not currently cover host-constructed Location vectors."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin blittable vector demo does not currently cover host-constructed Location vectors."
    ),
    exclude(
        wasm,
        reason = "The Wasm demo tests do not currently cover blittable record vector helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn sum_ratings(locations: Vec<Location>) -> f64 {
    locations.iter().map(|location| location.rating).sum()
}

#[demo_bench_macros::demo_case(
    "records.blittable.trades.should_generate_sample_vector",
    description = "generate_trades returns the requested number of Trade records.",
    exclude(
        wasm,
        reason = "The Wasm demo tests do not currently cover blittable record vector helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn generate_trades(count: i32) -> Vec<Trade> {
    (0..count)
        .map(|index| Trade {
            id: i64::from(index),
            symbol_id: index % 500,
            price: 100.0 + f64::from(index) * 0.01,
            quantity: i64::from(index % 1000) + 1,
            bid: 99.95 + f64::from(index) * 0.01,
            ask: 100.05 + f64::from(index) * 0.01,
            volume: i64::from(index) * 1000,
            timestamp: 1_700_000_000_000 + i64::from(index) * 1000,
            is_buy: index % 2 == 0,
        })
        .collect()
}

#[demo_bench_macros::demo_case(
    "records.blittable.trades.should_sum_volumes",
    description = "sum_trade_volumes receives Trade records and sums their i64 volume fields.",
    exclude(
        wasm,
        reason = "The Wasm demo tests do not currently cover blittable record vector helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn sum_trade_volumes(trades: Vec<Trade>) -> i64 {
    trades.iter().map(|trade| trade.volume).sum()
}

#[demo_bench_macros::demo_case(
    "records.blittable.trades.should_aggregate_with_locations",
    description = "aggregate_location_trade_stats receives Location and Trade vectors together and combines open-location count with total trade volume.",
    exclude(
        wasm,
        reason = "The Wasm demo tests do not currently cover blittable record vector helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn aggregate_location_trade_stats(locations: Vec<Location>, trades: Vec<Trade>) -> i64 {
    let open_locations = locations.iter().filter(|location| location.is_open).count() as i64;
    let total_trade_volume: i64 = trades.iter().map(|trade| trade.volume).sum();
    open_locations + total_trade_volume
}

#[demo_bench_macros::demo_case(
    "records.blittable.particles.should_generate_sample_vector",
    description = "generate_particles returns the requested number of Particle records.",
    exclude(
        wasm,
        reason = "The Wasm demo tests do not currently cover blittable record vector helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn generate_particles(count: i32) -> Vec<Particle> {
    (0..count)
        .map(|index| Particle {
            id: i64::from(index),
            x: f64::from(index) * 0.1,
            y: f64::from(index) * 0.2,
            z: f64::from(index) * 0.3,
            vx: f64::from(index) * 0.01,
            vy: f64::from(index) * 0.02,
            vz: f64::from(index) * 0.03,
            mass: 1.0 + f64::from(index) * 0.001,
            charge: if index % 2 == 0 { 1.0 } else { -1.0 },
            active: index % 10 != 0,
        })
        .collect()
}

#[demo_bench_macros::demo_case(
    "records.blittable.particles.should_sum_masses",
    description = "sum_particle_masses receives Particle records and sums their f64 mass fields.",
    exclude(
        wasm,
        reason = "The Wasm demo tests do not currently cover blittable record vector helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn sum_particle_masses(particles: Vec<Particle>) -> f64 {
    particles.iter().map(|particle| particle.mass).sum()
}

#[demo_bench_macros::demo_case(
    "records.blittable.sensor_readings.should_generate_sample_vector",
    description = "generate_sensor_readings returns the requested number of SensorReading records.",
    exclude(
        wasm,
        reason = "The Wasm demo tests do not currently cover blittable record vector helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn generate_sensor_readings(count: i32) -> Vec<SensorReading> {
    (0..count)
        .map(|index| SensorReading {
            sensor_id: i64::from(index % 100),
            timestamp: 1_700_000_000_000 + i64::from(index) * 100,
            temperature: 20.0 + f64::from(index % 30),
            humidity: 40.0 + f64::from(index % 40),
            pressure: 1_013.25 + f64::from(index % 20),
            light: f64::from(index % 1000),
            battery: 100.0 - f64::from(index % 100),
            signal_strength: -50 - (index % 50),
            is_valid: index % 20 != 0,
        })
        .collect()
}

#[demo_bench_macros::demo_case(
    "records.blittable.sensor_readings.should_average_generated_temperatures",
    description = "avg_sensor_temperature receives SensorReading records and averages their f64 temperature fields.",
    exclude(
        wasm,
        reason = "The Wasm demo tests do not currently cover blittable record vector helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[demo_bench_macros::demo_case(
    "records.blittable.sensor_readings.should_average_empty_vector_as_zero",
    description = "avg_sensor_temperature treats an empty SensorReading vector as average zero.",
    exclude(
        apple,
        reason = "The Apple blittable vector demo does not currently cover empty SensorReading vectors."
    ),
    exclude(
        java,
        reason = "The Java blittable vector demo does not currently cover empty SensorReading vectors."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin blittable vector demo does not currently cover empty SensorReading vectors."
    ),
    exclude(
        wasm,
        reason = "The Wasm demo tests do not currently cover blittable record vector helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi, wasm_bindgen)]
pub fn avg_sensor_temperature(readings: Vec<SensorReading>) -> f64 {
    let count = readings.len();
    if count == 0 {
        0.0
    } else {
        readings
            .iter()
            .map(|reading| reading.temperature)
            .sum::<f64>()
            / count as f64
    }
}

#[demo_bench_macros::demo_case(
    "records.blittable.locations.find_location.should_return_some_for_positive_id",
    description = "find_location returns Some(Location) for a positive id.",
    exclude(
        csharp,
        reason = "The C# blittable record demo does not currently cover find_location."
    ),
    exclude(
        java,
        reason = "The Java blittable record demo does not currently cover find_location."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin blittable record demo does not currently cover find_location."
    ),
    exclude(
        wasm,
        reason = "The Wasm blittable record demo does not currently cover find_location."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[demo_bench_macros::demo_case(
    "records.blittable.locations.find_location.should_return_none_for_non_positive_id",
    description = "find_location returns None for a non-positive id.",
    exclude(
        csharp,
        reason = "The C# blittable record demo does not currently cover find_location."
    ),
    exclude(
        java,
        reason = "The Java blittable record demo does not currently cover find_location."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin blittable record demo does not currently cover find_location."
    ),
    exclude(
        wasm,
        reason = "The Wasm blittable record demo does not currently cover find_location."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi)]
pub fn find_location(id: i32) -> Option<Location> {
    if id > 0 {
        Some(Location {
            id: i64::from(id),
            lat: 37.7749,
            lng: -122.4194,
            rating: 4.5,
            review_count: 100,
            is_open: true,
        })
    } else {
        None
    }
}

#[demo_bench_macros::demo_case(
    "records.blittable.locations.find_locations.should_return_some_vector_for_positive_count",
    description = "find_locations returns Some generated Location vector for a positive count.",
    exclude(
        csharp,
        reason = "The C# blittable record demo does not currently cover find_locations."
    ),
    exclude(
        java,
        reason = "The Java blittable record demo does not currently cover find_locations."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin blittable record demo does not currently cover find_locations."
    ),
    exclude(
        wasm,
        reason = "The Wasm blittable record demo does not currently cover find_locations."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[demo_bench_macros::demo_case(
    "records.blittable.locations.find_locations.should_return_none_for_non_positive_count",
    description = "find_locations returns None for a non-positive count.",
    exclude(
        csharp,
        reason = "The C# blittable record demo does not currently cover find_locations."
    ),
    exclude(
        java,
        reason = "The Java blittable record demo does not currently cover find_locations."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin blittable record demo does not currently cover find_locations."
    ),
    exclude(
        wasm,
        reason = "The Wasm blittable record demo does not currently cover find_locations."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover blittable record vector helpers."
    )
)]
#[export]
#[benchmark_candidate(function, uniffi)]
pub fn find_locations(count: i32) -> Option<Vec<Location>> {
    if count > 0 {
        Some(generate_locations(count))
    } else {
        None
    }
}
