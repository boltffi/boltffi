package com.example.bench_compare;

import java.util.List;
import java.util.concurrent.TimeUnit;
import org.openjdk.jmh.annotations.*;
import org.openjdk.jmh.infra.Blackhole;

import com.example.bench_boltffi.BenchBoltFFI;
import com.example.bench_boltffi.Location;
import com.example.bench_boltffi.Trade;
import com.example.bench_boltffi.Particle;
import com.example.bench_boltffi.SensorReading;
import com.example.bench_boltffi.BenchmarkUserProfile;
import com.example.bench_boltffi.Direction;
import com.example.bench_boltffi.TaskStatus;
import com.example.bench_boltffi.Counter;
import com.example.bench_boltffi.DataStore;
import com.example.bench_boltffi.DataPoint;
import com.example.bench_boltffi.Accumulator;
import com.example.bench_boltffi.DataConsumer;
import com.example.bench_boltffi.DataProvider;

@BenchmarkMode(Mode.AverageTime)
@OutputTimeUnit(TimeUnit.NANOSECONDS)
@State(Scope.Thread)
public class BoltffiJavaBench {
    private List<Location> locations1k;
    private List<Location> locations10k;
    private List<Trade> trades1k;
    private List<Trade> trades10k;
    private List<Particle> particles1k;
    private List<Particle> particles10k;
    private List<SensorReading> sensors1k;
    private List<SensorReading> sensors10k;
    private List<Direction> directions1k;
    private List<Direction> directions10k;
    private int[] i32Vec10k;
    private int[] i32Vec100k;
    private double[] f64Vec10k;
    private byte[] echoBytes64k;
    private int[] echoVecI32Values10k;
    private List<BenchmarkUserProfile> users100;
    private List<BenchmarkUserProfile> users1k;
    private DataProvider provider100;
    private DataProvider provider1k;

    @Setup
    public void setup() {
        locations1k = BenchBoltFFI.generateLocations(1000);
        locations10k = BenchBoltFFI.generateLocations(10000);
        trades1k = BenchBoltFFI.generateTrades(1000);
        trades10k = BenchBoltFFI.generateTrades(10000);
        particles1k = BenchBoltFFI.generateParticles(1000);
        particles10k = BenchBoltFFI.generateParticles(10000);
        sensors1k = BenchBoltFFI.generateSensorReadings(1000);
        sensors10k = BenchBoltFFI.generateSensorReadings(10000);
        directions1k = BenchBoltFFI.generateDirections(1000);
        directions10k = BenchBoltFFI.generateDirections(10000);
        i32Vec10k = BenchBoltFFI.generateI32Vec(10000);
        i32Vec100k = BenchBoltFFI.generateI32Vec(100_000);
        f64Vec10k = BenchBoltFFI.generateF64Vec(10000);
        echoBytes64k = new byte[65_536];
        java.util.Arrays.fill(echoBytes64k, (byte) 42);
        echoVecI32Values10k = java.util.stream.IntStream.range(0, 10_000).toArray();
        users100 = BenchBoltFFI.generateUserProfiles(100);
        users1k = BenchBoltFFI.generateUserProfiles(1000);
        provider100 = new FixedDataProvider(100);
        provider1k = new FixedDataProvider(1000);
    }

    // --- Call Overhead ---

    @Benchmark
    public void boltffi_java_noop(Blackhole bh) {
        BenchBoltFFI.noop();
        bh.consume(0);
    }

    @Benchmark
    public void boltffi_java_echo_bool(Blackhole bh) {
        bh.consume(BenchBoltFFI.echoBool(true));
    }

    @Benchmark
    public void boltffi_java_negate_bool(Blackhole bh) {
        bh.consume(BenchBoltFFI.negateBool(true));
    }

    @Benchmark
    public void boltffi_java_echo_i32(Blackhole bh) {
        bh.consume(BenchBoltFFI.echoI32(42));
    }

    @Benchmark
    public void boltffi_java_echo_u32(Blackhole bh) {
        bh.consume(BenchBoltFFI.echoU32(-1));
    }

    @Benchmark
    public void boltffi_java_echo_f64(Blackhole bh) {
        bh.consume(BenchBoltFFI.echoF64(3.14159));
    }

    @Benchmark
    public void boltffi_java_add_f64(Blackhole bh) {
        bh.consume(BenchBoltFFI.addF64(1.25, 2.5));
    }

    @Benchmark
    public void boltffi_java_add(Blackhole bh) {
        bh.consume(BenchBoltFFI.add(100, 200));
    }

    @Benchmark
    public void boltffi_java_multiply(Blackhole bh) {
        bh.consume(BenchBoltFFI.multiply(2.5, 4.0));
    }

    @Benchmark
    public void boltffi_java_inc_u64(Blackhole bh) {
        bh.consume(BenchBoltFFI.incU64Value(0L));
    }

    @Benchmark
    public void boltffi_java_inc_u64_value(Blackhole bh) {
        bh.consume(BenchBoltFFI.incU64Value(0L));
    }

    // --- Strings ---

    @Benchmark
    public void boltffi_java_echo_string_small(Blackhole bh) {
        bh.consume(BenchBoltFFI.echoString("hello"));
    }

    @Benchmark
    public void boltffi_java_echo_string_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.echoString("x".repeat(1000)));
    }

    @Benchmark
    public void boltffi_java_generate_string_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateString(1000));
    }

    @Benchmark
    public void boltffi_java_echo_bytes_64k(Blackhole bh) {
        bh.consume(BenchBoltFFI.echoBytes(echoBytes64k));
    }

    @Benchmark
    public void boltffi_java_echo_vec_i32_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.echoVecI32(echoVecI32Values10k));
    }

    // --- Struct Generation ---

    @Benchmark
    public void boltffi_java_generate_locations_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateLocations(1000));
    }

    @Benchmark
    public void boltffi_java_generate_locations_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateLocations(10000));
    }

    @Benchmark
    public void boltffi_java_generate_trades_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateTrades(1000));
    }

    @Benchmark
    public void boltffi_java_generate_trades_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateTrades(10000));
    }

    @Benchmark
    public void boltffi_java_generate_particles_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateParticles(1000));
    }

    @Benchmark
    public void boltffi_java_generate_particles_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateParticles(10000));
    }

    @Benchmark
    public void boltffi_java_generate_sensor_readings_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateSensorReadings(1000));
    }

    @Benchmark
    public void boltffi_java_generate_sensor_readings_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateSensorReadings(10000));
    }

    @Benchmark
    public void boltffi_java_generate_user_profiles_100(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateUserProfiles(100));
    }

    @Benchmark
    public void boltffi_java_generate_user_profiles_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateUserProfiles(1000));
    }

    @Benchmark
    public void boltffi_java_generate_directions_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateDirections(1000));
    }

    @Benchmark
    public void boltffi_java_generate_directions_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateDirections(10000));
    }

    // --- Struct Consumption ---

    @Benchmark
    public void boltffi_java_sum_ratings_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.sumRatings(locations1k));
    }

    @Benchmark
    public void boltffi_java_sum_ratings_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.sumRatings(locations10k));
    }

    @Benchmark
    public void boltffi_java_sum_trade_volumes_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.sumTradeVolumes(trades1k));
    }

    @Benchmark
    public void boltffi_java_sum_trade_volumes_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.sumTradeVolumes(trades10k));
    }

    @Benchmark
    public void boltffi_java_sum_particle_masses_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.sumParticleMasses(particles1k));
    }

    @Benchmark
    public void boltffi_java_sum_particle_masses_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.sumParticleMasses(particles10k));
    }

    @Benchmark
    public void boltffi_java_avg_sensor_temp_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.avgSensorTemperature(sensors1k));
    }

    @Benchmark
    public void boltffi_java_avg_sensor_temp_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.avgSensorTemperature(sensors10k));
    }

    @Benchmark
    public void boltffi_java_process_locations_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.processLocations(locations1k));
    }

    @Benchmark
    public void boltffi_java_process_locations_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.processLocations(locations10k));
    }

    @Benchmark
    public void boltffi_java_sum_user_scores_100(Blackhole bh) {
        bh.consume(BenchBoltFFI.sumUserScores(users100));
    }

    @Benchmark
    public void boltffi_java_sum_user_scores_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.sumUserScores(users1k));
    }

    @Benchmark
    public void boltffi_java_count_active_users_100(Blackhole bh) {
        bh.consume(BenchBoltFFI.countActiveUsers(users100));
    }

    @Benchmark
    public void boltffi_java_count_active_users_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.countActiveUsers(users1k));
    }

    @Benchmark
    public void boltffi_java_count_north_1k(Blackhole bh) {
        bh.consume(BenchBoltFFI.countNorth(directions1k));
    }

    @Benchmark
    public void boltffi_java_count_north_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.countNorth(directions10k));
    }

    // --- Primitive Vectors ---

    @Benchmark
    public void boltffi_java_generate_i32_vec_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateI32Vec(10000));
    }

    @Benchmark
    public void boltffi_java_generate_i32_vec_100k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateI32Vec(100_000));
    }

    @Benchmark
    public void boltffi_java_generate_f64_vec_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateF64Vec(10000));
    }

    @Benchmark
    public void boltffi_java_generate_bytes_64k(Blackhole bh) {
        bh.consume(BenchBoltFFI.generateBytes(65536));
    }

    @Benchmark
    public void boltffi_java_sum_i32_vec_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.sumI32Vec(i32Vec10k));
    }

    @Benchmark
    public void boltffi_java_sum_i32_vec_100k(Blackhole bh) {
        bh.consume(BenchBoltFFI.sumI32Vec(i32Vec100k));
    }

    @Benchmark
    public void boltffi_java_sum_f64_vec_10k(Blackhole bh) {
        bh.consume(BenchBoltFFI.sumF64Vec(f64Vec10k));
    }

    // --- Classes ---

    @Benchmark
    public void boltffi_java_counter_increment_mutex(Blackhole bh) {
        try (var counter = new Counter(0)) {
            for (int i = 0; i < 1000; i++) {
                counter.increment();
            }
            bh.consume(counter.get());
        }
    }

    @Benchmark
    public void boltffi_java_datastore_add_record_1k(Blackhole bh) {
        try (var store = new DataStore()) {
            for (int i = 0; i < 1000; i++) {
                store.add(new DataPoint((double) i, (double) i * 2.0, (long) i));
            }
            bh.consume(store.len());
        }
    }

    @Benchmark
    public void boltffi_java_accumulator_mutex(Blackhole bh) {
        try (var acc = new Accumulator()) {
            for (int i = 0; i < 1000; i++) {
                acc.add((long) i);
            }
            bh.consume(acc.get());
            acc.reset();
        }
    }

    @Benchmark
    public void boltffi_java_callback_100(Blackhole bh) {
        try (var consumer = new DataConsumer()) {
            consumer.setProvider(provider100);
            bh.consume(consumer.computeSum());
        }
    }

    @Benchmark
    public void boltffi_java_callback_1k(Blackhole bh) {
        try (var consumer = new DataConsumer()) {
            consumer.setProvider(provider1k);
            bh.consume(consumer.computeSum());
        }
    }

    // --- Enums ---

    @Benchmark
    public void boltffi_java_simple_enum(Blackhole bh) {
        bh.consume(BenchBoltFFI.oppositeDirection(Direction.NORTH));
        bh.consume(BenchBoltFFI.directionToDegrees(Direction.EAST));
    }

    @Benchmark
    public void boltffi_java_echo_direction(Blackhole bh) {
        bh.consume(BenchBoltFFI.echoDirection(Direction.NORTH));
    }

    @Benchmark
    public void boltffi_java_find_direction(Blackhole bh) {
        bh.consume(BenchBoltFFI.findDirection(0));
    }

    @Benchmark
    public void boltffi_java_data_enum_input(Blackhole bh) {
        bh.consume(BenchBoltFFI.getStatusProgress(new TaskStatus.InProgress(50)));
        bh.consume(BenchBoltFFI.isStatusComplete(new TaskStatus.Completed(100)));
    }

    // --- Optional ---

    @Benchmark
    public void boltffi_java_find_even_100(Blackhole bh) {
        for (int i = 0; i < 100; i++) {
            bh.consume(BenchBoltFFI.findEven(i));
        }
    }

    @Benchmark
    public void boltffi_java_find_positive_f64(Blackhole bh) {
        bh.consume(BenchBoltFFI.findPositiveF64(3.14));
    }

    @Benchmark
    public void boltffi_java_find_name(Blackhole bh) {
        bh.consume(BenchBoltFFI.findName(1));
    }

    @Benchmark
    public void boltffi_java_find_names_100(Blackhole bh) {
        bh.consume(BenchBoltFFI.findNames(100));
    }

    @Benchmark
    public void boltffi_java_find_numbers_100(Blackhole bh) {
        bh.consume(BenchBoltFFI.findNumbers(100));
    }

    @Benchmark
    public void boltffi_java_find_locations_100(Blackhole bh) {
        bh.consume(BenchBoltFFI.findLocations(100));
    }

    @Benchmark
    public void boltffi_java_async_add(Blackhole bh) {
        bh.consume(BenchBoltFFI.asyncAdd(100, 200).join());
    }

    private static final class FixedDataProvider implements DataProvider {
        private final DataPoint[] points;

        private FixedDataProvider(int count) {
            this.points = new DataPoint[count];
            for (int index = 0; index < count; index++) {
                points[index] = new DataPoint((double) index, (double) index * 2.0, (long) index);
            }
        }

        @Override
        public int getCount() {
            return points.length;
        }

        @Override
        public DataPoint getItem(int index) {
            return points[index];
        }
    }
}
