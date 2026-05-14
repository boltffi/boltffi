using BenchmarkDotNet.Attributes;
using BoltffiAddress = Demo.Address;
using BoltffiBindings = Demo.Demo;
using BoltffiLine = Demo.Line;
using BoltffiPerson = Demo.Person;
using BoltffiPoint = Demo.Point;
using UniffiAddress = uniffi.demo.Address;
using UniffiBindings = uniffi.demo.DemoMethods;
using UniffiLine = uniffi.demo.Line;
using UniffiPerson = uniffi.demo.Person;
using UniffiPoint = uniffi.demo.Point;

namespace BoltFFIBench;

public interface IWireReaderBindings<TPoint, TAddress, TPerson, TLine>
{
    void Noop();
    int EchoI32(int value);
    int Add(int left, int right);
    string EchoString(string value);
    string GenerateString(int size);
    TPoint MakePoint(double x, double y);
    TAddress MakeAddress(string street, string city, string zip);
    TAddress EchoAddress(TAddress address);
    TPerson MakePerson(string name, uint age);
    TPerson EchoPerson(TPerson person);
    TLine MakeLine(TPoint start, TPoint end);
    TLine EchoLine(TLine line);
}

public sealed class BoltffiWireReaderBindings : IWireReaderBindings<BoltffiPoint, BoltffiAddress, BoltffiPerson, BoltffiLine>
{
    public static readonly BoltffiWireReaderBindings Instance = new();

    private BoltffiWireReaderBindings() {}

    public void Noop() => BoltffiBindings.Noop();

    public int EchoI32(int value) => BoltffiBindings.EchoI32(value);

    public int Add(int left, int right) => BoltffiBindings.Add(left, right);

    public string EchoString(string value) => BoltffiBindings.EchoString(value);

    public string GenerateString(int size) => BoltffiBindings.GenerateString(size);

    public BoltffiPoint MakePoint(double x, double y) => BoltffiBindings.MakePoint(x, y);

    public BoltffiAddress MakeAddress(string street, string city, string zip) => new(street, city, zip);

    public BoltffiAddress EchoAddress(BoltffiAddress address) => BoltffiBindings.EchoAddress(address);

    public BoltffiPerson MakePerson(string name, uint age) => new(name, age);

    public BoltffiPerson EchoPerson(BoltffiPerson person) => BoltffiBindings.EchoPerson(person);

    public BoltffiLine MakeLine(BoltffiPoint start, BoltffiPoint end) => new(start, end);

    public BoltffiLine EchoLine(BoltffiLine line) => BoltffiBindings.EchoLine(line);
}

public sealed class UniffiWireReaderBindings : IWireReaderBindings<UniffiPoint, UniffiAddress, UniffiPerson, UniffiLine>
{
    public static readonly UniffiWireReaderBindings Instance = new();

    private UniffiWireReaderBindings() {}

    public void Noop() => UniffiBindings.Noop();

    public int EchoI32(int value) => UniffiBindings.EchoI32(value);

    public int Add(int left, int right) => UniffiBindings.Add(left, right);

    public string EchoString(string value) => UniffiBindings.EchoString(value);

    public string GenerateString(int size) => UniffiBindings.GenerateString(size);

    public UniffiPoint MakePoint(double x, double y) => UniffiBindings.MakePoint(x, y);

    public UniffiAddress MakeAddress(string street, string city, string zip) => new(street, city, zip);

    public UniffiAddress EchoAddress(UniffiAddress address) => UniffiBindings.EchoAddress(address);

    public UniffiPerson MakePerson(string name, uint age) => new(name, age);

    public UniffiPerson EchoPerson(UniffiPerson person) => UniffiBindings.EchoPerson(person);

    public UniffiLine MakeLine(UniffiPoint start, UniffiPoint end) => new(start, end);

    public UniffiLine EchoLine(UniffiLine line) => UniffiBindings.EchoLine(line);
}

public abstract class WireReaderBenchmarks<TPoint, TAddress, TPerson, TLine>
{
    private string _smallString = null!;
    private string _largeString = null!;
    private TAddress _address = default!;
    private TPerson _person = default!;
    private TLine _line = default!;

    protected abstract IWireReaderBindings<TPoint, TAddress, TPerson, TLine> Bindings { get; }

    [GlobalSetup]
    public void Setup()
    {
        _smallString = "hello";
        _largeString = new string('x', 64 * 1024);
        _address = Bindings.MakeAddress("123 Main St", "Seattle", "98101");
        _person = Bindings.MakePerson("Alice", 30);
        _line = Bindings.MakeLine(Bindings.MakePoint(1.0, 2.0), Bindings.MakePoint(3.0, 4.0));
    }

    [Benchmark]
    public void Noop() => Bindings.Noop();

    [Benchmark]
    public int EchoI32() => Bindings.EchoI32(42);

    [Benchmark]
    public int Add() => Bindings.Add(17, 25);

    [Benchmark]
    public string EchoStringSmall() => Bindings.EchoString(_smallString);

    [Benchmark]
    public string EchoString64K() => Bindings.EchoString(_largeString);

    [Benchmark]
    public string GenerateString1K() => Bindings.GenerateString(1000);

    [Benchmark]
    public string GenerateString64K() => Bindings.GenerateString(64 * 1024);

    [Benchmark]
    public void MakePoint() => _ = Bindings.MakePoint(1.0, 2.0);

    [Benchmark]
    public void EchoAddress() => _ = Bindings.EchoAddress(_address);

    [Benchmark]
    public void EchoPerson() => _ = Bindings.EchoPerson(_person);

    [Benchmark]
    public void EchoLine() => _ = Bindings.EchoLine(_line);
}

[MemoryDiagnoser]
public class BoltffiWireReaderBenchmarks : WireReaderBenchmarks<BoltffiPoint, BoltffiAddress, BoltffiPerson, BoltffiLine>
{
    protected override IWireReaderBindings<BoltffiPoint, BoltffiAddress, BoltffiPerson, BoltffiLine> Bindings =>
        BoltffiWireReaderBindings.Instance;
}

[MemoryDiagnoser]
public class UniffiWireReaderBenchmarks : WireReaderBenchmarks<UniffiPoint, UniffiAddress, UniffiPerson, UniffiLine>
{
    protected override IWireReaderBindings<UniffiPoint, UniffiAddress, UniffiPerson, UniffiLine> Bindings =>
        UniffiWireReaderBindings.Instance;
}
