using System;
using System.Linq;
using System.Runtime.InteropServices;
using BenchmarkDotNet.Attributes;
using BoltffiAccumulator = Demo.Accumulator;
using BoltffiAccumulatorSingleThreaded = Demo.AccumulatorSingleThreaded;
using BoltffiBenchmarkUserProfile = Demo.BenchmarkUserProfile;
using BoltffiBindings = Demo.Demo;
using BoltffiCounter = Demo.Counter;
using BoltffiCounterSingleThreaded = Demo.CounterSingleThreaded;
using BoltffiDataConsumer = Demo.DataConsumer;
using BoltffiDataPoint = Demo.DataPoint;
using BoltffiDataProvider = Demo.DataProvider;
using BoltffiDataStore = Demo.DataStore;
using BoltffiDirection = Demo.Direction;
using BoltffiLocation = Demo.Location;
using BoltffiParticle = Demo.Particle;
using BoltffiSensorReading = Demo.SensorReading;
using BoltffiTaskStatus = Demo.TaskStatus;
using BoltffiTrade = Demo.Trade;
using UniffiAccumulator = uniffi.demo.Accumulator;
using UniffiBenchmarkUserProfile = uniffi.demo.BenchmarkUserProfile;
using UniffiBindings = uniffi.demo.DemoMethods;
using UniffiCounter = uniffi.demo.Counter;
using UniffiDataConsumer = uniffi.demo.DataConsumer;
using UniffiDataPoint = uniffi.demo.DataPoint;
using UniffiDataProvider = uniffi.demo.DataProvider;
using UniffiDataStore = uniffi.demo.DataStore;
using UniffiDirection = uniffi.demo.Direction;
using UniffiLocation = uniffi.demo.Location;
using UniffiParticle = uniffi.demo.Particle;
using UniffiSensorReading = uniffi.demo.SensorReading;
using UniffiTaskStatus = uniffi.demo.TaskStatus;
using UniffiTrade = uniffi.demo.Trade;

namespace BoltFFIBench;

public interface ISharedSurfaceBindings<TDirection, TTaskStatus, TLocation, TTrade, TParticle, TSensorReading, TUserProfile, TProvider>
    where TDirection : struct, Enum
{
    bool EchoBool(bool value);
    bool NegateBool(bool value);
    double EchoF64(double value);
    double AddF64(double left, double right);
    double Multiply(double left, double right);
    ulong IncU64(ulong[] values);
    ulong IncU64Value(ulong value);
    string EchoString(string value);
    byte[] EchoBytes(byte[] data);
    byte[] GenerateBytes(int size);
    int[] EchoVecI32(int[] values);
    TLocation[] GenerateLocations(int count);
    TTrade[] GenerateTrades(int count);
    TParticle[] GenerateParticles(int count);
    TSensorReading[] GenerateSensorReadings(int count);
    TUserProfile[] GenerateUserProfiles(int count);
    TDirection[] GenerateDirections(int count);
    double SumRatings(TLocation[] locations);
    long SumTradeVolumes(TTrade[] trades);
    double SumParticleMasses(TParticle[] particles);
    double AvgSensorTemperature(TSensorReading[] readings);
    int ProcessLocations(TLocation[] locations);
    double SumUserScores(TUserProfile[] users);
    int CountActiveUsers(TUserProfile[] users);
    int CountNorth(TDirection[] directions);
    int[] GenerateI32Vec(int count);
    double[] GenerateF64Vec(int count);
    long SumI32Vec(int[] values);
    double SumF64Vec(double[] values);
    int CounterIncrementMutex(int iterations);
    ulong DatastoreAddRecord(int count);
    long AccumulatorMutex(int iterations);
    TProvider CreateProvider(int count);
    ulong Callback(TProvider provider);
    TDirection North { get; }
    TDirection East { get; }
    TDirection OppositeDirection(TDirection direction);
    int DirectionToDegrees(TDirection direction);
    TDirection EchoDirection(TDirection direction);
    TDirection? FindDirection(int id);
    TTaskStatus InProgress(int progress);
    TTaskStatus Completed(int result);
    int GetStatusProgress(TTaskStatus status);
    bool IsStatusComplete(TTaskStatus status);
    int? FindEven(int value);
    double? FindPositiveF64(double value);
    string? FindName(int id);
    string[]? FindNames(int count);
    int[]? FindNumbers(int count);
    TLocation[]? FindLocations(int count);
}

internal static class BoltffiSharedSurfaceNativeMethods
{
    [DllImport("demo", EntryPoint = "boltffi_inc_u64")]
    internal static extern void IncU64(ulong[] values, UIntPtr valuesLen);
}

public sealed class BoltffiSharedSurfaceBindings :
    ISharedSurfaceBindings<BoltffiDirection, BoltffiTaskStatus, BoltffiLocation, BoltffiTrade, BoltffiParticle, BoltffiSensorReading, BoltffiBenchmarkUserProfile, BoltffiDataProvider>
{
    public static readonly BoltffiSharedSurfaceBindings Instance = new();

    private BoltffiSharedSurfaceBindings() {}

    public bool EchoBool(bool value) => BoltffiBindings.EchoBool(value);
    public bool NegateBool(bool value) => BoltffiBindings.NegateBool(value);
    public double EchoF64(double value) => BoltffiBindings.EchoF64(value);
    public double AddF64(double left, double right) => BoltffiBindings.AddF64(left, right);
    public double Multiply(double left, double right) => BoltffiBindings.Multiply(left, right);
    public ulong IncU64(ulong[] values)
    {
        BoltffiSharedSurfaceNativeMethods.IncU64(values, (UIntPtr)values.Length);
        return values[0];
    }
    public ulong IncU64Value(ulong value) => BoltffiBindings.IncU64Value(value);
    public string EchoString(string value) => BoltffiBindings.EchoString(value);
    public byte[] EchoBytes(byte[] data) => BoltffiBindings.EchoBytes(data);
    public byte[] GenerateBytes(int size) => BoltffiBindings.GenerateBytes(size);
    public int[] EchoVecI32(int[] values) => BoltffiBindings.EchoVecI32(values);
    public BoltffiLocation[] GenerateLocations(int count) => BoltffiBindings.GenerateLocations(count);
    public BoltffiTrade[] GenerateTrades(int count) => BoltffiBindings.GenerateTrades(count);
    public BoltffiParticle[] GenerateParticles(int count) => BoltffiBindings.GenerateParticles(count);
    public BoltffiSensorReading[] GenerateSensorReadings(int count) => BoltffiBindings.GenerateSensorReadings(count);
    public BoltffiBenchmarkUserProfile[] GenerateUserProfiles(int count) => BoltffiBindings.GenerateUserProfiles(count);
    public BoltffiDirection[] GenerateDirections(int count) => BoltffiBindings.GenerateDirections(count);
    public double SumRatings(BoltffiLocation[] locations) => BoltffiBindings.SumRatings(locations);
    public long SumTradeVolumes(BoltffiTrade[] trades) => BoltffiBindings.SumTradeVolumes(trades);
    public double SumParticleMasses(BoltffiParticle[] particles) => BoltffiBindings.SumParticleMasses(particles);
    public double AvgSensorTemperature(BoltffiSensorReading[] readings) => BoltffiBindings.AvgSensorTemperature(readings);
    public int ProcessLocations(BoltffiLocation[] locations) => BoltffiBindings.ProcessLocations(locations);
    public double SumUserScores(BoltffiBenchmarkUserProfile[] users) => BoltffiBindings.SumUserScores(users);
    public int CountActiveUsers(BoltffiBenchmarkUserProfile[] users) => BoltffiBindings.CountActiveUsers(users);
    public int CountNorth(BoltffiDirection[] directions) => BoltffiBindings.CountNorth(directions);
    public int[] GenerateI32Vec(int count) => BoltffiBindings.GenerateI32Vec(count);
    public double[] GenerateF64Vec(int count) => BoltffiBindings.GenerateF64Vec(count);
    public long SumI32Vec(int[] values) => BoltffiBindings.SumI32Vec(values);
    public double SumF64Vec(double[] values) => BoltffiBindings.SumF64Vec(values);

    public int CounterIncrementMutex(int iterations)
    {
        using var counter = new BoltffiCounter(0);
        for (int index = 0; index < iterations; index++) counter.Increment();
        return counter.Get();
    }

    public ulong DatastoreAddRecord(int count)
    {
        using var store = new BoltffiDataStore();
        for (int index = 0; index < count; index++)
        {
            store.Add(new BoltffiDataPoint((double)index, (double)index * 2.0, index));
        }

        return (ulong)store.Len();
    }

    public long AccumulatorMutex(int iterations)
    {
        using var accumulator = new BoltffiAccumulator();
        for (int index = 0; index < iterations; index++) accumulator.Add(index);
        long value = accumulator.Get();
        accumulator.Reset();
        return value;
    }

    public BoltffiDataProvider CreateProvider(int count) => new BoltffiFixedDataProvider(count);

    public ulong Callback(BoltffiDataProvider provider)
    {
        using var consumer = new BoltffiDataConsumer();
        consumer.SetProvider(provider);
        return consumer.ComputeSum();
    }

    public BoltffiDirection North => BoltffiDirection.North;
    public BoltffiDirection East => BoltffiDirection.East;
    public BoltffiDirection OppositeDirection(BoltffiDirection direction) => BoltffiBindings.OppositeDirection(direction);
    public int DirectionToDegrees(BoltffiDirection direction) => BoltffiBindings.DirectionToDegrees(direction);
    public BoltffiDirection EchoDirection(BoltffiDirection direction) => BoltffiBindings.EchoDirection(direction);
    public BoltffiDirection? FindDirection(int id) => BoltffiBindings.FindDirection(id);
    public BoltffiTaskStatus InProgress(int progress) => new BoltffiTaskStatus.InProgress(progress);
    public BoltffiTaskStatus Completed(int result) => new BoltffiTaskStatus.Completed(result);
    public int GetStatusProgress(BoltffiTaskStatus status) => BoltffiBindings.GetStatusProgress(status);
    public bool IsStatusComplete(BoltffiTaskStatus status) => BoltffiBindings.IsStatusComplete(status);
    public int? FindEven(int value) => BoltffiBindings.FindEven(value);
    public double? FindPositiveF64(double value) => BoltffiBindings.FindPositiveF64(value);
    public string? FindName(int id) => BoltffiBindings.FindName(id);
    public string[]? FindNames(int count) => BoltffiBindings.FindNames(count);
    public int[]? FindNumbers(int count) => BoltffiBindings.FindNumbers(count);
    public BoltffiLocation[]? FindLocations(int count) => BoltffiBindings.FindLocations(count);

    private sealed class BoltffiFixedDataProvider : BoltffiDataProvider
    {
        private readonly BoltffiDataPoint[] _points;

        public BoltffiFixedDataProvider(int count)
        {
            _points = new BoltffiDataPoint[count];
            for (int index = 0; index < count; index++)
            {
                _points[index] = new BoltffiDataPoint((double)index, (double)index * 2.0, index);
            }
        }

        public uint GetCount() => (uint)_points.Length;

        public BoltffiDataPoint GetItem(uint index) => _points[index];
    }
}

public sealed class UniffiSharedSurfaceBindings :
    ISharedSurfaceBindings<UniffiDirection, UniffiTaskStatus, UniffiLocation, UniffiTrade, UniffiParticle, UniffiSensorReading, UniffiBenchmarkUserProfile, UniffiDataProvider>
{
    public static readonly UniffiSharedSurfaceBindings Instance = new();

    private UniffiSharedSurfaceBindings() {}

    public bool EchoBool(bool value) => UniffiBindings.EchoBool(value);
    public bool NegateBool(bool value) => UniffiBindings.NegateBool(value);
    public double EchoF64(double value) => UniffiBindings.EchoF64(value);
    public double AddF64(double left, double right) => UniffiBindings.AddF64(left, right);
    public double Multiply(double left, double right) => UniffiBindings.Multiply(left, right);
    public ulong IncU64(ulong[] values) => UniffiBindings.IncU64Value(values[0]);
    public ulong IncU64Value(ulong value) => UniffiBindings.IncU64Value(value);
    public string EchoString(string value) => UniffiBindings.EchoString(value);
    public byte[] EchoBytes(byte[] data) => UniffiBindings.EchoBytes(data);
    public byte[] GenerateBytes(int size) => UniffiBindings.GenerateBytes(size);
    public int[] EchoVecI32(int[] values) => UniffiBindings.EchoVecI32(values);
    public UniffiLocation[] GenerateLocations(int count) => UniffiBindings.GenerateLocations(count);
    public UniffiTrade[] GenerateTrades(int count) => UniffiBindings.GenerateTrades(count);
    public UniffiParticle[] GenerateParticles(int count) => UniffiBindings.GenerateParticles(count);
    public UniffiSensorReading[] GenerateSensorReadings(int count) => UniffiBindings.GenerateSensorReadings(count);
    public UniffiBenchmarkUserProfile[] GenerateUserProfiles(int count) => UniffiBindings.GenerateUserProfiles(count);
    public UniffiDirection[] GenerateDirections(int count) => UniffiBindings.GenerateDirections(count);
    public double SumRatings(UniffiLocation[] locations) => UniffiBindings.SumRatings(locations);
    public long SumTradeVolumes(UniffiTrade[] trades) => UniffiBindings.SumTradeVolumes(trades);
    public double SumParticleMasses(UniffiParticle[] particles) => UniffiBindings.SumParticleMasses(particles);
    public double AvgSensorTemperature(UniffiSensorReading[] readings) => UniffiBindings.AvgSensorTemperature(readings);
    public int ProcessLocations(UniffiLocation[] locations) => UniffiBindings.ProcessLocations(locations);
    public double SumUserScores(UniffiBenchmarkUserProfile[] users) => UniffiBindings.SumUserScores(users);
    public int CountActiveUsers(UniffiBenchmarkUserProfile[] users) => UniffiBindings.CountActiveUsers(users);
    public int CountNorth(UniffiDirection[] directions) => UniffiBindings.CountNorth(directions);
    public int[] GenerateI32Vec(int count) => UniffiBindings.GenerateI32Vec(count);
    public double[] GenerateF64Vec(int count) => UniffiBindings.GenerateF64Vec(count);
    public long SumI32Vec(int[] values) => UniffiBindings.SumI32Vec(values);
    public double SumF64Vec(double[] values) => UniffiBindings.SumF64Vec(values);

    public int CounterIncrementMutex(int iterations)
    {
        using var counter = new UniffiCounter(0);
        for (int index = 0; index < iterations; index++) counter.Increment();
        return counter.Get();
    }

    public ulong DatastoreAddRecord(int count)
    {
        using var store = new UniffiDataStore();
        for (int index = 0; index < count; index++)
        {
            store.Add(new UniffiDataPoint((double)index, (double)index * 2.0, index));
        }

        return store.Len();
    }

    public long AccumulatorMutex(int iterations)
    {
        using var accumulator = new UniffiAccumulator();
        for (int index = 0; index < iterations; index++) accumulator.Add(index);
        long value = accumulator.Get();
        accumulator.Reset();
        return value;
    }

    public UniffiDataProvider CreateProvider(int count) => new UniffiFixedDataProvider(count);

    public ulong Callback(UniffiDataProvider provider)
    {
        using var consumer = new UniffiDataConsumer();
        consumer.SetProvider(provider);
        return consumer.ComputeSum();
    }

    public UniffiDirection North => UniffiDirection.North;
    public UniffiDirection East => UniffiDirection.East;
    public UniffiDirection OppositeDirection(UniffiDirection direction) => UniffiBindings.OppositeDirection(direction);
    public int DirectionToDegrees(UniffiDirection direction) => UniffiBindings.DirectionToDegrees(direction);
    public UniffiDirection EchoDirection(UniffiDirection direction) => UniffiBindings.EchoDirection(direction);
    public UniffiDirection? FindDirection(int id) => UniffiBindings.FindDirection(id);
    public UniffiTaskStatus InProgress(int progress) => new UniffiTaskStatus.InProgress(progress);
    public UniffiTaskStatus Completed(int result) => new UniffiTaskStatus.Completed(result);
    public int GetStatusProgress(UniffiTaskStatus status) => UniffiBindings.GetStatusProgress(status);
    public bool IsStatusComplete(UniffiTaskStatus status) => UniffiBindings.IsStatusComplete(status);
    public int? FindEven(int value) => UniffiBindings.FindEven(value);
    public double? FindPositiveF64(double value) => UniffiBindings.FindPositiveF64(value);
    public string? FindName(int id) => UniffiBindings.FindName(id);
    public string[]? FindNames(int count) => UniffiBindings.FindNames(count);
    public int[]? FindNumbers(int count) => UniffiBindings.FindNumbers(count);
    public UniffiLocation[]? FindLocations(int count) => UniffiBindings.FindLocations(count);

    private sealed class UniffiFixedDataProvider : UniffiDataProvider
    {
        private readonly UniffiDataPoint[] _points;

        public UniffiFixedDataProvider(int count)
        {
            _points = new UniffiDataPoint[count];
            for (int index = 0; index < count; index++)
            {
                _points[index] = new UniffiDataPoint((double)index, (double)index * 2.0, index);
            }
        }

        public uint GetCount() => (uint)_points.Length;

        public UniffiDataPoint GetItem(uint index) => _points[index];
    }
}

public abstract class SharedSurfaceBenchmarks<TDirection, TTaskStatus, TLocation, TTrade, TParticle, TSensorReading, TUserProfile, TProvider>
    where TDirection : struct, Enum
{
    private string _string1K = null!;
    private byte[] _echoBytes64K = null!;
    private int[] _echoVecI32Values10K = null!;
    private TLocation[] _locations1K = null!;
    private TLocation[] _locations10K = null!;
    private TTrade[] _trades1K = null!;
    private TTrade[] _trades10K = null!;
    private TParticle[] _particles1K = null!;
    private TParticle[] _particles10K = null!;
    private TSensorReading[] _sensors1K = null!;
    private TSensorReading[] _sensors10K = null!;
    private TDirection[] _directions1K = null!;
    private TDirection[] _directions10K = null!;
    private int[] _i32Vec10K = null!;
    private int[] _i32Vec100K = null!;
    private double[] _f64Vec10K = null!;
    private TUserProfile[] _users100 = null!;
    private TUserProfile[] _users1K = null!;
    private TProvider _provider100 = default!;
    private TProvider _provider1K = default!;
    private ulong[] _incU64Values = null!;

    protected abstract ISharedSurfaceBindings<TDirection, TTaskStatus, TLocation, TTrade, TParticle, TSensorReading, TUserProfile, TProvider> Bindings { get; }

    [GlobalSetup]
    public void Setup()
    {
        _string1K = new string('x', 1000);
        _echoBytes64K = new byte[64 * 1024];
        Array.Fill(_echoBytes64K, (byte)42);
        _echoVecI32Values10K = Enumerable.Range(0, 10_000).ToArray();
        _locations1K = Bindings.GenerateLocations(1000);
        _locations10K = Bindings.GenerateLocations(10000);
        _trades1K = Bindings.GenerateTrades(1000);
        _trades10K = Bindings.GenerateTrades(10000);
        _particles1K = Bindings.GenerateParticles(1000);
        _particles10K = Bindings.GenerateParticles(10000);
        _sensors1K = Bindings.GenerateSensorReadings(1000);
        _sensors10K = Bindings.GenerateSensorReadings(10000);
        _directions1K = Bindings.GenerateDirections(1000);
        _directions10K = Bindings.GenerateDirections(10000);
        _i32Vec10K = Bindings.GenerateI32Vec(10000);
        _i32Vec100K = Bindings.GenerateI32Vec(100_000);
        _f64Vec10K = Bindings.GenerateF64Vec(10000);
        _users100 = Bindings.GenerateUserProfiles(100);
        _users1K = Bindings.GenerateUserProfiles(1000);
        _provider100 = Bindings.CreateProvider(100);
        _provider1K = Bindings.CreateProvider(1000);
        _incU64Values = new[] { 0UL };
    }

    [Benchmark]
    public bool EchoBool() => Bindings.EchoBool(true);

    [Benchmark]
    public bool NegateBool() => Bindings.NegateBool(true);

    [Benchmark]
    public double EchoF64() => Bindings.EchoF64(3.14159);

    [Benchmark]
    public double AddF64() => Bindings.AddF64(1.25, 2.5);

    [Benchmark]
    public double Multiply() => Bindings.Multiply(2.5, 4.0);

    [Benchmark]
    public ulong IncU64() => Bindings.IncU64(_incU64Values);

    [Benchmark]
    public ulong IncU64Value() => Bindings.IncU64Value(0UL);

    [Benchmark]
    public string EchoString1K() => Bindings.EchoString(_string1K);

    [Benchmark]
    public byte[] EchoBytes64K() => Bindings.EchoBytes(_echoBytes64K);

    [Benchmark]
    public byte[] GenerateBytes64K() => Bindings.GenerateBytes(64 * 1024);

    [Benchmark]
    public int[] EchoVecI32_10K() => Bindings.EchoVecI32(_echoVecI32Values10K);

    [Benchmark]
    public TLocation[] GenerateLocations_1K() => Bindings.GenerateLocations(1000);

    [Benchmark]
    public TLocation[] GenerateLocations_10K() => Bindings.GenerateLocations(10000);

    [Benchmark]
    public TTrade[] GenerateTrades_1K() => Bindings.GenerateTrades(1000);

    [Benchmark]
    public TTrade[] GenerateTrades_10K() => Bindings.GenerateTrades(10000);

    [Benchmark]
    public TParticle[] GenerateParticles_1K() => Bindings.GenerateParticles(1000);

    [Benchmark]
    public TParticle[] GenerateParticles_10K() => Bindings.GenerateParticles(10000);

    [Benchmark]
    public TSensorReading[] GenerateSensorReadings_1K() => Bindings.GenerateSensorReadings(1000);

    [Benchmark]
    public TSensorReading[] GenerateSensorReadings_10K() => Bindings.GenerateSensorReadings(10000);

    [Benchmark]
    public TUserProfile[] GenerateUserProfiles_100() => Bindings.GenerateUserProfiles(100);

    [Benchmark]
    public TUserProfile[] GenerateUserProfiles_1K() => Bindings.GenerateUserProfiles(1000);

    [Benchmark]
    public TDirection[] GenerateDirections_1K() => Bindings.GenerateDirections(1000);

    [Benchmark]
    public TDirection[] GenerateDirections_10K() => Bindings.GenerateDirections(10000);

    [Benchmark]
    public double SumRatings_1K() => Bindings.SumRatings(_locations1K);

    [Benchmark]
    public double SumRatings_10K() => Bindings.SumRatings(_locations10K);

    [Benchmark]
    public long SumTradeVolumes_1K() => Bindings.SumTradeVolumes(_trades1K);

    [Benchmark]
    public long SumTradeVolumes_10K() => Bindings.SumTradeVolumes(_trades10K);

    [Benchmark]
    public double SumParticleMasses_1K() => Bindings.SumParticleMasses(_particles1K);

    [Benchmark]
    public double SumParticleMasses_10K() => Bindings.SumParticleMasses(_particles10K);

    [Benchmark]
    public double AvgSensorTemp_1K() => Bindings.AvgSensorTemperature(_sensors1K);

    [Benchmark]
    public double AvgSensorTemp_10K() => Bindings.AvgSensorTemperature(_sensors10K);

    [Benchmark]
    public int ProcessLocations_1K() => Bindings.ProcessLocations(_locations1K);

    [Benchmark]
    public int ProcessLocations_10K() => Bindings.ProcessLocations(_locations10K);

    [Benchmark]
    public double SumUserScores_100() => Bindings.SumUserScores(_users100);

    [Benchmark]
    public double SumUserScores_1K() => Bindings.SumUserScores(_users1K);

    [Benchmark]
    public int CountActiveUsers_100() => Bindings.CountActiveUsers(_users100);

    [Benchmark]
    public int CountActiveUsers_1K() => Bindings.CountActiveUsers(_users1K);

    [Benchmark]
    public int CountNorth_1K() => Bindings.CountNorth(_directions1K);

    [Benchmark]
    public int CountNorth_10K() => Bindings.CountNorth(_directions10K);

    [Benchmark]
    public int[] GenerateI32Vec_10K() => Bindings.GenerateI32Vec(10000);

    [Benchmark]
    public int[] GenerateI32Vec_100K() => Bindings.GenerateI32Vec(100_000);

    [Benchmark]
    public double[] GenerateF64Vec_10K() => Bindings.GenerateF64Vec(10000);

    [Benchmark]
    public long SumI32Vec_10K() => Bindings.SumI32Vec(_i32Vec10K);

    [Benchmark]
    public long SumI32Vec_100K() => Bindings.SumI32Vec(_i32Vec100K);

    [Benchmark]
    public double SumF64Vec_10K() => Bindings.SumF64Vec(_f64Vec10K);

    [Benchmark]
    public int CounterIncrementMutex() => Bindings.CounterIncrementMutex(1000);

    [Benchmark]
    public ulong DatastoreAddRecord_1K() => Bindings.DatastoreAddRecord(1000);

    [Benchmark]
    public long AccumulatorMutex() => Bindings.AccumulatorMutex(1000);

    [Benchmark]
    public ulong Callback_100() => Bindings.Callback(_provider100);

    [Benchmark]
    public ulong Callback_1K() => Bindings.Callback(_provider1K);

    [Benchmark]
    public int SimpleEnum()
    {
        _ = Bindings.OppositeDirection(Bindings.North);
        return Bindings.DirectionToDegrees(Bindings.East);
    }

    [Benchmark]
    public TDirection EchoDirection() => Bindings.EchoDirection(Bindings.North);

    [Benchmark]
    public TDirection? FindDirection() => Bindings.FindDirection(0);

    [Benchmark]
    public int DataEnumInput()
    {
        int progress = Bindings.GetStatusProgress(Bindings.InProgress(50));
        bool complete = Bindings.IsStatusComplete(Bindings.Completed(100));
        return progress + (complete ? 1 : 0);
    }

    [Benchmark]
    public int FindEven_100()
    {
        int found = 0;
        for (int index = 0; index < 100; index++)
        {
            if (Bindings.FindEven(index).HasValue) found++;
        }

        return found;
    }

    [Benchmark]
    public double? FindPositiveF64() => Bindings.FindPositiveF64(3.14);

    [Benchmark]
    public string? FindName() => Bindings.FindName(1);

    [Benchmark]
    public string[]? FindNames_100() => Bindings.FindNames(100);

    [Benchmark]
    public int[]? FindNumbers_100() => Bindings.FindNumbers(100);

    [Benchmark]
    public TLocation[]? FindLocations_100() => Bindings.FindLocations(100);
}

[MemoryDiagnoser]
public class BoltffiSharedSurfaceBenchmarks :
    SharedSurfaceBenchmarks<BoltffiDirection, BoltffiTaskStatus, BoltffiLocation, BoltffiTrade, BoltffiParticle, BoltffiSensorReading, BoltffiBenchmarkUserProfile, BoltffiDataProvider>
{
    protected override ISharedSurfaceBindings<BoltffiDirection, BoltffiTaskStatus, BoltffiLocation, BoltffiTrade, BoltffiParticle, BoltffiSensorReading, BoltffiBenchmarkUserProfile, BoltffiDataProvider> Bindings =>
        BoltffiSharedSurfaceBindings.Instance;
}

[MemoryDiagnoser]
public class UniffiSharedSurfaceBenchmarks :
    SharedSurfaceBenchmarks<UniffiDirection, UniffiTaskStatus, UniffiLocation, UniffiTrade, UniffiParticle, UniffiSensorReading, UniffiBenchmarkUserProfile, UniffiDataProvider>
{
    protected override ISharedSurfaceBindings<UniffiDirection, UniffiTaskStatus, UniffiLocation, UniffiTrade, UniffiParticle, UniffiSensorReading, UniffiBenchmarkUserProfile, UniffiDataProvider> Bindings =>
        UniffiSharedSurfaceBindings.Instance;
}

[MemoryDiagnoser]
public class BoltffiOnlySharedSurfaceBenchmarks
{
    [Benchmark]
    public int CounterIncrementSingleThreaded()
    {
        using var counter = new BoltffiCounterSingleThreaded();
        for (int index = 0; index < 1000; index++) counter.Increment();
        return counter.Get();
    }

    [Benchmark]
    public long AccumulatorSingleThreaded()
    {
        using var accumulator = new BoltffiAccumulatorSingleThreaded();
        for (int index = 0; index < 1000; index++) accumulator.Add(index);
        long value = accumulator.Get();
        accumulator.Reset();
        return value;
    }

    [Benchmark]
    public System.Threading.Tasks.Task<int> AsyncAdd() => BoltffiBindings.AsyncAdd(100, 200);
}
