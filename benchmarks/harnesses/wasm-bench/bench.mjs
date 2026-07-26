import Benchmark from 'benchmark';
import { writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';

const boltffiOverlay = await import('./build/generated/boltffi/node.js');
await boltffiOverlay.initialized;
const boltffiDemo = await import('./build/generated/boltffi-demo/node.js');
await boltffiDemo.initialized;
const boltffi = new Proxy(boltffiOverlay, {
  get(target, property, receiver) {
    if (Reflect.has(target, property)) {
      return Reflect.get(target, property, receiver);
    }
    return Reflect.get(boltffiDemo, property, receiver);
  },
});

const require = createRequire(import.meta.url);
const wasmbindgen = require('./build/generated/wasmbindgen/demo.js');

console.log('BoltFFI vs wasm-bindgen WASM Benchmark');
console.log('=====================================');
console.log('Note: UniFFI does not have WASM support, defaulting to wasm-bindgen for comparison.\n');

const results = [];

const dataPointTuples1k = Array.from(
  { length: 1000 },
  (_, index) => [index, index * 2, BigInt(index)]
);

function toNs(seconds) {
  return Number.isFinite(seconds) ? seconds * 1e9 : null;
}

function percentile(sortedValues, ratio) {
  if (sortedValues.length === 0) {
    return null;
  }

  const rawIndex = (sortedValues.length - 1) * ratio;
  const lowerIndex = Math.floor(rawIndex);
  const upperIndex = Math.ceil(rawIndex);
  if (lowerIndex === upperIndex) {
    return sortedValues[lowerIndex];
  }

  const weight = rawIndex - lowerIndex;
  return sortedValues[lowerIndex] * (1 - weight) + sortedValues[upperIndex] * weight;
}

function serializeTarget(target) {
  if (!target) {
    return null;
  }

  const sampleNs = Array.isArray(target.stats?.sample)
    ? target.stats.sample
        .map((sample) => toNs(Number(sample)))
        .filter((sample) => sample !== null)
    : [];
  const sortedSampleNs = [...sampleNs].sort((left, right) => left - right);
  const stats = target.stats ?? {};
  const hasFailure = Boolean(target.error) || Boolean(target.aborted);
  const meanNs = !hasFailure || sampleNs.length > 0 ? toNs(Number(stats.mean)) : null;
  const deviationNs = !hasFailure || sampleNs.length > 0 ? toNs(Number(stats.deviation)) : null;
  const semNs = !hasFailure || sampleNs.length > 0 ? toNs(Number(stats.sem)) : null;
  const moeNs = !hasFailure || sampleNs.length > 0 ? toNs(Number(stats.moe)) : null;

  return {
    hz: Number.isFinite(target.hz) ? target.hz : null,
    count: Number.isFinite(target.count) ? target.count : null,
    cycles: Number.isFinite(target.cycles) ? target.cycles : null,
    aborted: Boolean(target.aborted),
    error: target.error ? String(target.error.message ?? target.error) : null,
    times: {
      cycle_seconds: Number.isFinite(target.times?.cycle) ? target.times.cycle : null,
      elapsed_seconds: Number.isFinite(target.times?.elapsed) ? target.times.elapsed : null,
      period_ns: toNs(Number(target.times?.period)),
      timestamp_ms: Number.isFinite(target.times?.timeStamp) ? target.times.timeStamp : null,
    },
    stats: {
      mean_ns: meanNs,
      deviation_ns: deviationNs,
      sem_ns: semNs,
      moe_ns: moeNs,
      rme_percent: Number.isFinite(stats.rme) ? stats.rme : null,
      variance_seconds_sq: Number.isFinite(stats.variance) ? stats.variance : null,
      sample_count: sampleNs.length,
      min_ns: sortedSampleNs[0] ?? null,
      max_ns: sortedSampleNs.at(-1) ?? null,
      median_ns: percentile(sortedSampleNs, 0.5),
      p90_ns: percentile(sortedSampleNs, 0.9),
      p95_ns: percentile(sortedSampleNs, 0.95),
      p99_ns: percentile(sortedSampleNs, 0.99),
    },
  };
}

function formatSpeedupLabel(boltffiNs, wasmbindgenNs) {
  if (boltffiNs === null || wasmbindgenNs === null) {
    return null;
  }
  if (boltffiNs === 0 && wasmbindgenNs === 0) {
    return 'TIE';
  }
  if (boltffiNs === 0) {
    return 'Infinityx';
  }
  if (wasmbindgenNs === 0) {
    return '0.00x';
  }
  return (wasmbindgenNs / boltffiNs).toFixed(2) + 'x';
}

function runSuite(name, boltffiFn, wasmbindgenFn) {
  return new Promise((resolve) => {
    const suite = new Benchmark.Suite(name);
    const isAsync = boltffiFn.constructor.name === 'AsyncFunction';
    
    if (isAsync) {
      suite
        .add(`boltffi_${name}`, { defer: true, fn: async (deferred) => { await boltffiFn(); deferred.resolve(); } })
        .add(`wasmbindgen_${name}`, { defer: true, fn: async (deferred) => { await wasmbindgenFn(); deferred.resolve(); } })
    } else {
      suite
        .add(`boltffi_${name}`, boltffiFn)
        .add(`wasmbindgen_${name}`, wasmbindgenFn)
    }
    suite.on('cycle', (event) => {
        console.log(String(event.target));
      })
      .on('complete', function() {
        const boltffiResult = this.filter((b) => b.name.startsWith('boltffi'))[0];
        const wbResult = this.filter((b) => b.name.startsWith('wasmbindgen'))[0];

        const boltffiStats = serializeTarget(boltffiResult);
        const wasmbindgenStats = serializeTarget(wbResult);
        const boltffiNs = boltffiStats?.stats?.mean_ns ?? null;
        const wbNs = wasmbindgenStats?.stats?.mean_ns ?? null;

        results.push({
          name,
          async: isAsync,
          boltffi_ns: boltffiNs == null ? null : Math.round(boltffiNs),
          wasmbindgen_ns: wbNs == null ? null : Math.round(wbNs),
          speedup: formatSpeedupLabel(boltffiNs, wbNs),
          variants: {
            boltffi: boltffiStats,
            wasmbindgen: wasmbindgenStats,
          }
        });

        resolve();
      })
      .run({ async: true });
  });
}

const boltffiLocations1k = boltffi.generateLocations(1000);
const boltffiLocations10k = boltffi.generateLocations(10000);
const boltffiTrades1k = boltffi.generateTrades(1000);
const boltffiTrades10k = boltffi.generateTrades(10000);
const boltffiParticles1k = boltffi.generateParticles(1000);
const boltffiParticles10k = boltffi.generateParticles(10000);
const boltffiSensors1k = boltffi.generateSensorReadings(1000);
const boltffiSensors10k = boltffi.generateSensorReadings(10000);

const boltffiDirections1k = boltffi.generateDirections(1000);
const boltffiDirections10k = boltffi.generateDirections(10000);
const wasmbindgenDirections1k = wasmbindgen.generate_directions(1000);
const wasmbindgenDirections10k = wasmbindgen.generate_directions(10000);

const boltffiI32Vec10k = boltffi.generateI32Vec(10000);
const boltffiI32Vec100k = boltffi.generateI32Vec(100000);
const wasmbindgenI32Vec10k = wasmbindgen.generate_i32_vec(10000);
const wasmbindgenI32Vec100k = wasmbindgen.generate_i32_vec(100000);

const boltffiF64Vec10k = boltffi.generateF64Vec(10000);
const wasmbindgenF64Vec10k = wasmbindgen.generate_f64_vec(10000);
const echoBytes64k = new Uint8Array(65536).fill(42);
const echoVecI32Values10k = Array.from({ length: 10000 }, (_, index) => index);

const locationSeed1k = boltffiLocations1k.map(({ id, lat, lng, rating, reviewCount, isOpen }) => ({
  id,
  lat,
  lng,
  rating,
  reviewCount,
  isOpen,
}));
const locationSeed10k = boltffiLocations10k.map(({ id, lat, lng, rating, reviewCount, isOpen }) => ({
  id,
  lat,
  lng,
  rating,
  reviewCount,
  isOpen,
}));
const particleSeed1k = boltffiParticles1k.map(({ id, x, y, z, vx, vy, vz, mass, charge, active }) => ({
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
}));
const particleSeed10k = boltffiParticles10k.map(({ id, x, y, z, vx, vy, vz, mass, charge, active }) => ({
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
}));
const tradeSeed1k = boltffiTrades1k.map(({ id, symbolId, price, quantity, bid, ask, volume, timestamp, isBuy }) => ({
  id,
  symbolId,
  price,
  quantity,
  bid,
  ask,
  volume,
  timestamp,
  isBuy,
}));
const tradeSeed10k = boltffiTrades10k.map(({ id, symbolId, price, quantity, bid, ask, volume, timestamp, isBuy }) => ({
  id,
  symbolId,
  price,
  quantity,
  bid,
  ask,
  volume,
  timestamp,
  isBuy,
}));
const sensorSeed1k = boltffiSensors1k.map(({ sensorId, timestamp, temperature, humidity, pressure, light, battery, signalStrength, isValid }) => ({
  sensorId,
  timestamp,
  temperature,
  humidity,
  pressure,
  light,
  battery,
  signalStrength,
  isValid,
}));
const sensorSeed10k = boltffiSensors10k.map(({ sensorId, timestamp, temperature, humidity, pressure, light, battery, signalStrength, isValid }) => ({
  sensorId,
  timestamp,
  temperature,
  humidity,
  pressure,
  light,
  battery,
  signalStrength,
  isValid,
}));

function cloneBoltffiLocations(seed) {
  return seed.map(({ id, lat, lng, rating, reviewCount, isOpen }) => ({
    id,
    lat,
    lng,
    rating,
    reviewCount,
    isOpen,
  }));
}

function cloneWasmBindgenLocations(seed) {
  return seed.map(({ id, lat, lng, rating, reviewCount, isOpen }) =>
    new wasmbindgen.Location(id, lat, lng, rating, reviewCount, isOpen)
  );
}

function cloneBoltffiParticles(seed) {
  return seed.map(({ id, x, y, z, vx, vy, vz, mass, charge, active }) => ({
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
  }));
}

function cloneWasmBindgenParticles(seed) {
  return seed.map(({ id, x, y, z, vx, vy, vz, mass, charge, active }) =>
    new wasmbindgen.Particle(id, x, y, z, vx, vy, vz, mass, charge, active)
  );
}

function cloneBoltffiTrades(seed) {
  return seed.map(({ id, symbolId, price, quantity, bid, ask, volume, timestamp, isBuy }) => ({
    id,
    symbolId,
    price,
    quantity,
    bid,
    ask,
    volume,
    timestamp,
    isBuy,
  }));
}

function cloneWasmBindgenTrades(seed) {
  return seed.map(({ id, symbolId, price, quantity, bid, ask, volume, timestamp, isBuy }) =>
    new wasmbindgen.Trade(id, symbolId, price, quantity, bid, ask, volume, timestamp, isBuy)
  );
}

function cloneBoltffiSensors(seed) {
  return seed.map(({ sensorId, timestamp, temperature, humidity, pressure, light, battery, signalStrength, isValid }) => ({
    sensorId,
    timestamp,
    temperature,
    humidity,
    pressure,
    light,
    battery,
    signalStrength,
    isValid,
  }));
}

function cloneWasmBindgenSensors(seed) {
  return seed.map(({ sensorId, timestamp, temperature, humidity, pressure, light, battery, signalStrength, isValid }) =>
    new wasmbindgen.SensorReading(
      sensorId,
      timestamp,
      temperature,
      humidity,
      pressure,
      light,
      battery,
      signalStrength,
      isValid,
    )
  );
}

function makeDataProvider(count) {
  return {
    getCount() {
      return count;
    },
    getItem(index) {
      return {
        x: index,
        y: index * 2,
        timestamp: BigInt(index),
      };
    },
  };
}

const taskStatusInProgress = { tag: 'InProgress', progress: 50 };
const taskStatusCompleted = { tag: 'Completed', result: 100 };
const boltffiUsers100 = boltffi.generateUserProfiles(100);
const boltffiUsers1k = boltffi.generateUserProfiles(1000);
const wasmbindgenUsers100 = wasmbindgen.generate_user_profiles(100);
const wasmbindgenUsers1k = wasmbindgen.generate_user_profiles(1000);
const callbackProvider100 = makeDataProvider(100);
const callbackProvider1k = makeDataProvider(1000);

const benchmarkCases = [
  { name: 'noop', boltffi: () => boltffi.noop(), wasmbindgen: () => wasmbindgen.noop() },
  { name: 'echo_bool', boltffi: () => boltffi.echoBool(true), wasmbindgen: () => wasmbindgen.echo_bool(true) },
  { name: 'negate_bool', boltffi: () => boltffi.negateBool(true), wasmbindgen: () => wasmbindgen.negate_bool(true) },
  { name: 'echo_i32', boltffi: () => boltffi.echoI32(42), wasmbindgen: () => wasmbindgen.echo_i32(42) },
  { name: 'echo_f64', boltffi: () => boltffi.echoF64(3.14159), wasmbindgen: () => wasmbindgen.echo_f64(3.14159) },
  { name: 'add', boltffi: () => boltffi.add(100, 200), wasmbindgen: () => wasmbindgen.add(100, 200) },
  { name: 'add_f64', boltffi: () => boltffi.addF64(1.25, 2.5), wasmbindgen: () => wasmbindgen.add_f64(1.25, 2.5) },
  { name: 'multiply', boltffi: () => boltffi.multiply(2.5, 4.0), wasmbindgen: () => wasmbindgen.multiply(2.5, 4.0) },
  {
    name: 'inc_u64',
    boltffi: () => {
      const values = BigUint64Array.of(0n);
      boltffi.incU64(values);
      return values[0];
    },
    wasmbindgen: () => {
      const values = BigUint64Array.of(0n);
      return wasmbindgen.inc_u64(values)[0];
    },
  },
  {
    name: 'inc_u64_value',
    boltffi: () => boltffi.incU64Value(0n),
    wasmbindgen: () => wasmbindgen.inc_u64_value(0n),
  },
  { name: 'echo_string_small', boltffi: () => boltffi.echoString('hello'), wasmbindgen: () => wasmbindgen.echo_string('hello') },
  { name: 'echo_string_1k', boltffi: () => boltffi.echoString('x'.repeat(1000)), wasmbindgen: () => wasmbindgen.echo_string('x'.repeat(1000)) },
  { name: 'generate_string_1k', boltffi: () => boltffi.generateString(1000), wasmbindgen: () => wasmbindgen.generate_string(1000) },
  { name: 'echo_bytes_64k', boltffi: () => boltffi.echoBytes(echoBytes64k), wasmbindgen: () => wasmbindgen.echo_bytes(echoBytes64k) },
  { name: 'echo_vec_i32_10k', boltffi: () => boltffi.echoVecI32(echoVecI32Values10k), wasmbindgen: () => wasmbindgen.echo_vec_i32(echoVecI32Values10k) },
  { name: 'generate_locations_1k', boltffi: () => boltffi.generateLocations(1000), wasmbindgen: () => wasmbindgen.generate_locations(1000) },
  { name: 'generate_locations_10k', boltffi: () => boltffi.generateLocations(10000), wasmbindgen: () => wasmbindgen.generate_locations(10000) },
  { name: 'generate_trades_1k', boltffi: () => boltffi.generateTrades(1000), wasmbindgen: () => wasmbindgen.generate_trades(1000) },
  { name: 'generate_trades_10k', boltffi: () => boltffi.generateTrades(10000), wasmbindgen: () => wasmbindgen.generate_trades(10000) },
  { name: 'generate_particles_1k', boltffi: () => boltffi.generateParticles(1000), wasmbindgen: () => wasmbindgen.generate_particles(1000) },
  { name: 'generate_particles_10k', boltffi: () => boltffi.generateParticles(10000), wasmbindgen: () => wasmbindgen.generate_particles(10000) },
  { name: 'generate_sensor_readings_1k', boltffi: () => boltffi.generateSensorReadings(1000), wasmbindgen: () => wasmbindgen.generate_sensor_readings(1000) },
  { name: 'generate_sensor_readings_10k', boltffi: () => boltffi.generateSensorReadings(10000), wasmbindgen: () => wasmbindgen.generate_sensor_readings(10000) },
  { name: 'generate_i32_vec_10k', boltffi: () => boltffi.generateI32Vec(10000), wasmbindgen: () => wasmbindgen.generate_i32_vec(10000) },
  { name: 'generate_i32_vec_100k', boltffi: () => boltffi.generateI32Vec(100000), wasmbindgen: () => wasmbindgen.generate_i32_vec(100000) },
  { name: 'generate_f64_vec_10k', boltffi: () => boltffi.generateF64Vec(10000), wasmbindgen: () => wasmbindgen.generate_f64_vec(10000) },
  { name: 'generate_bytes_64k', boltffi: () => boltffi.generateBytes(65536), wasmbindgen: () => wasmbindgen.generate_bytes(65536) },
  { name: 'sum_ratings_1k', boltffi: () => boltffi.sumRatings(cloneBoltffiLocations(locationSeed1k)), wasmbindgen: () => wasmbindgen.sum_ratings(cloneWasmBindgenLocations(locationSeed1k)) },
  { name: 'sum_ratings_10k', boltffi: () => boltffi.sumRatings(cloneBoltffiLocations(locationSeed10k)), wasmbindgen: () => wasmbindgen.sum_ratings(cloneWasmBindgenLocations(locationSeed10k)) },
  { name: 'process_locations_1k', boltffi: () => boltffi.processLocations(cloneBoltffiLocations(locationSeed1k)), wasmbindgen: () => wasmbindgen.process_locations(cloneWasmBindgenLocations(locationSeed1k)) },
  { name: 'process_locations_10k', boltffi: () => boltffi.processLocations(cloneBoltffiLocations(locationSeed10k)), wasmbindgen: () => wasmbindgen.process_locations(cloneWasmBindgenLocations(locationSeed10k)) },
  { name: 'sum_trade_volumes_1k', boltffi: () => boltffi.sumTradeVolumes(cloneBoltffiTrades(tradeSeed1k)), wasmbindgen: () => wasmbindgen.sum_trade_volumes(cloneWasmBindgenTrades(tradeSeed1k)) },
  { name: 'sum_trade_volumes_10k', boltffi: () => boltffi.sumTradeVolumes(cloneBoltffiTrades(tradeSeed10k)), wasmbindgen: () => wasmbindgen.sum_trade_volumes(cloneWasmBindgenTrades(tradeSeed10k)) },
  { name: 'sum_particle_masses_1k', boltffi: () => boltffi.sumParticleMasses(cloneBoltffiParticles(particleSeed1k)), wasmbindgen: () => wasmbindgen.sum_particle_masses(cloneWasmBindgenParticles(particleSeed1k)) },
  { name: 'sum_particle_masses_10k', boltffi: () => boltffi.sumParticleMasses(cloneBoltffiParticles(particleSeed10k)), wasmbindgen: () => wasmbindgen.sum_particle_masses(cloneWasmBindgenParticles(particleSeed10k)) },
  { name: 'avg_sensor_temp_1k', boltffi: () => boltffi.avgSensorTemperature(cloneBoltffiSensors(sensorSeed1k)), wasmbindgen: () => wasmbindgen.avg_sensor_temperature(cloneWasmBindgenSensors(sensorSeed1k)) },
  { name: 'avg_sensor_temp_10k', boltffi: () => boltffi.avgSensorTemperature(cloneBoltffiSensors(sensorSeed10k)), wasmbindgen: () => wasmbindgen.avg_sensor_temperature(cloneWasmBindgenSensors(sensorSeed10k)) },
  { name: 'sum_i32_vec_10k', boltffi: () => boltffi.sumI32Vec(boltffiI32Vec10k), wasmbindgen: () => wasmbindgen.sum_i32_vec(wasmbindgenI32Vec10k) },
  { name: 'sum_i32_vec_100k', boltffi: () => boltffi.sumI32Vec(boltffiI32Vec100k), wasmbindgen: () => wasmbindgen.sum_i32_vec(wasmbindgenI32Vec100k) },
  { name: 'sum_f64_vec_10k', boltffi: () => boltffi.sumF64Vec(boltffiF64Vec10k), wasmbindgen: () => wasmbindgen.sum_f64_vec(wasmbindgenF64Vec10k) },
  {
    name: 'echo_direction',
    boltffi: () => boltffi.echoDirection(boltffi.Direction.North),
    wasmbindgen: () => wasmbindgen.echo_direction(wasmbindgen.Direction.North),
  },
  {
    name: 'find_direction',
    boltffi: () => boltffi.findDirection(0),
    wasmbindgen: () => wasmbindgen.find_direction(0),
  },
  {
    name: 'simple_enum',
    boltffi: () => {
      boltffi.oppositeDirection(boltffi.Direction.North);
      return boltffi.directionToDegrees(boltffi.Direction.East);
    },
    wasmbindgen: () => {
      wasmbindgen.opposite_direction(wasmbindgen.Direction.North);
      return wasmbindgen.direction_to_degrees(wasmbindgen.Direction.East);
    },
  },
  {
    name: 'data_enum_input',
    boltffi: () => {
      boltffi.getStatusProgress(taskStatusInProgress);
      return boltffi.isStatusComplete(taskStatusCompleted);
    },
    wasmbindgen: () => {
      wasmbindgen.get_status_progress(taskStatusInProgress);
      return wasmbindgen.is_status_complete(taskStatusCompleted);
    },
  },
  {
    name: 'find_positive_f64',
    boltffi: () => boltffi.findPositiveF64(3.14),
    wasmbindgen: () => wasmbindgen.find_positive_f64(3.14),
  },
  {
    name: 'find_name',
    boltffi: () => boltffi.findName(1),
    wasmbindgen: () => wasmbindgen.find_name(1),
  },
  {
    name: 'find_names_100',
    boltffi: () => boltffi.findNames(100),
    wasmbindgen: () => wasmbindgen.find_names(100),
  },
  {
    name: 'find_numbers_100',
    boltffi: () => boltffi.findNumbers(100),
    wasmbindgen: () => wasmbindgen.find_numbers(100),
  },
  {
    name: 'find_locations_100',
    boltffi: () => boltffi.findLocations(100),
    wasmbindgen: () => wasmbindgen.find_locations(100),
  },
  { name: 'generate_directions_1k', boltffi: () => boltffi.generateDirections(1000), wasmbindgen: () => wasmbindgen.generate_directions(1000) },
  { name: 'generate_directions_10k', boltffi: () => boltffi.generateDirections(10000), wasmbindgen: () => wasmbindgen.generate_directions(10000) },
  { name: 'count_north_1k', boltffi: () => boltffi.countNorth(boltffiDirections1k), wasmbindgen: () => wasmbindgen.count_north(wasmbindgenDirections1k) },
  { name: 'count_north_10k', boltffi: () => boltffi.countNorth(boltffiDirections10k), wasmbindgen: () => wasmbindgen.count_north(wasmbindgenDirections10k) },
  {
    name: 'counter_increment_mutex',
    boltffi: () => {
      const counter = boltffi.Counter.new(0);
      for (let index = 0; index < 1000; index += 1) counter.increment();
      const value = counter.get();
      counter.dispose();
      return value;
    },
    wasmbindgen: () => {
      const counter = new wasmbindgen.Counter(0);
      for (let index = 0; index < 1000; index += 1) counter.increment();
      const value = counter.get();
      counter.free();
      return value;
    },
  },
  {
    name: 'counter_increment_single_threaded',
    boltffi: () => {
      const counter = boltffi.CounterSingleThreaded.new();
      for (let index = 0; index < 1000; index += 1) counter.increment();
      const value = counter.get();
      counter.dispose();
      return value;
    },
    wasmbindgen: () => {
      const counter = new wasmbindgen.CounterSingleThreaded();
      for (let index = 0; index < 1000; index += 1) counter.increment();
      const value = counter.get();
      counter.free();
      return value;
    },
  },
  {
    name: 'datastore_add_record_1k',
    boltffi: () => {
      const store = boltffi.DataStore.new();
      dataPointTuples1k.forEach(([x, y, timestamp]) => {
        store.add({ x, y, timestamp });
      });
      const length = store.len();
      store.dispose();
      return length;
    },
    wasmbindgen: () => {
      const store = new wasmbindgen.DataStore();
      dataPointTuples1k.forEach(([x, y, timestamp]) => {
        store.add(new wasmbindgen.DataPoint(x, y, timestamp));
      });
      const length = store.len();
      store.free();
      return length;
    },
  },
  {
    name: 'accumulator_mutex',
    boltffi: () => {
      const accumulator = boltffi.Accumulator.new();
      for (let index = 0n; index < 1000n; index += 1n) accumulator.add(index);
      const value = accumulator.get();
      accumulator.reset();
      accumulator.dispose();
      return value;
    },
    wasmbindgen: () => {
      const accumulator = new wasmbindgen.Accumulator();
      for (let index = 0n; index < 1000n; index += 1n) accumulator.add(index);
      const value = accumulator.get();
      accumulator.reset();
      accumulator.free();
      return value;
    },
  },
  {
    name: 'accumulator_single_threaded',
    boltffi: () => {
      const accumulator = boltffi.AccumulatorSingleThreaded.new();
      for (let index = 0n; index < 1000n; index += 1n) accumulator.add(index);
      const value = accumulator.get();
      accumulator.reset();
      accumulator.dispose();
      return value;
    },
    wasmbindgen: () => {
      const accumulator = new wasmbindgen.AccumulatorSingleThreaded();
      for (let index = 0n; index < 1000n; index += 1n) accumulator.add(index);
      const value = accumulator.get();
      accumulator.reset();
      accumulator.free();
      return value;
    },
  },
  {
    name: 'find_even_100',
    boltffi: () => {
      for (let index = 0; index < 100; index += 1) boltffi.findEven(index);
    },
    wasmbindgen: () => {
      for (let index = 0; index < 100; index += 1) wasmbindgen.find_even(index);
    },
  },
  {
    name: 'async_add',
    boltffi: async () => await boltffi.asyncAdd(100, 200),
    wasmbindgen: async () => await wasmbindgen.async_add(100, 200),
  },
  { name: 'generate_user_profiles_100', boltffi: () => boltffi.generateUserProfiles(100), wasmbindgen: () => wasmbindgen.generate_user_profiles(100) },
  { name: 'generate_user_profiles_1k', boltffi: () => boltffi.generateUserProfiles(1000), wasmbindgen: () => wasmbindgen.generate_user_profiles(1000) },
  { name: 'sum_user_scores_100', boltffi: () => boltffi.sumUserScores(boltffiUsers100), wasmbindgen: () => wasmbindgen.sum_user_scores(wasmbindgenUsers100) },
  { name: 'sum_user_scores_1k', boltffi: () => boltffi.sumUserScores(boltffiUsers1k), wasmbindgen: () => wasmbindgen.sum_user_scores(wasmbindgenUsers1k) },
  { name: 'count_active_users_100', boltffi: () => boltffi.countActiveUsers(boltffiUsers100), wasmbindgen: () => wasmbindgen.count_active_users(wasmbindgenUsers100) },
  { name: 'count_active_users_1k', boltffi: () => boltffi.countActiveUsers(boltffiUsers1k), wasmbindgen: () => wasmbindgen.count_active_users(wasmbindgenUsers1k) },
  {
    name: 'callback_100',
    boltffi: () => {
      const consumer = boltffi.DataConsumer.new();
      consumer.setProvider(callbackProvider100);
      const value = consumer.computeSum();
      consumer.dispose();
      return value;
    },
    wasmbindgen: () => {
      const consumer = new wasmbindgen.DataConsumer();
      consumer.set_provider(callbackProvider100);
      const value = consumer.compute_sum();
      consumer.free();
      return value;
    },
  },
  {
    name: 'callback_1k',
    boltffi: () => {
      const consumer = boltffi.DataConsumer.new();
      consumer.setProvider(callbackProvider1k);
      const value = consumer.computeSum();
      consumer.dispose();
      return value;
    },
    wasmbindgen: () => {
      const consumer = new wasmbindgen.DataConsumer();
      consumer.set_provider(callbackProvider1k);
      const value = consumer.compute_sum();
      consumer.free();
      return value;
    },
  },
];

const benchmarkFilter = process.env.BENCH_FILTER
  ? new RegExp(process.env.BENCH_FILTER)
  : null;
const selectedBenchmarkCases = benchmarkFilter
  ? benchmarkCases.filter(({ name }) => benchmarkFilter.test(name))
  : benchmarkCases;

for (const benchmarkCase of selectedBenchmarkCases) {
  await runSuite(benchmarkCase.name, benchmarkCase.boltffi, benchmarkCase.wasmbindgen);
}

console.log('\n=====================================');
console.log('Results Summary');
console.log('=====================================\n');

console.log('| Benchmark | BoltFFI (ns) | wasm-bindgen (ns) | Speedup |');
console.log('|-----------|--------------|-------------------|---------|');
for (const r of results) {
  const boltffiNs = r.boltffi_ns;
  const wbNs = r.wasmbindgen_ns;
  let speedupStr;
  if (boltffiNs === null || wbNs === null) {
    speedupStr = 'N/A';
  } else if (boltffiNs === 0 && wbNs === 0) {
    speedupStr = 'TIE';
  } else if (boltffiNs === 0) {
    speedupStr = '∞';
  } else if (wbNs === 0) {
    speedupStr = '∞ slower';
  } else {
    const ratio = wbNs / boltffiNs;
    if (ratio >= 0.95 && ratio <= 1.05) {
      speedupStr = 'TIE';
    } else if (ratio > 1) {
      speedupStr = ratio > 1000 ? '>1000x' : `${ratio.toFixed(2)}x`;
    } else {
      const inv = 1 / ratio;
      speedupStr = inv > 1000 ? '>1000x slower' : `${inv.toFixed(2)}x slower`;
    }
  }
  console.log(`| ${r.name} | ${boltffiNs} | ${wbNs} | ${speedupStr} |`);
}

if (process.env.BENCH_OUTPUT_JSON) {
  writeFileSync(
    process.env.BENCH_OUTPUT_JSON,
    JSON.stringify({ benchmarkjs_version: Benchmark.version, benchmarks: results }, null, 2)
  );
}
