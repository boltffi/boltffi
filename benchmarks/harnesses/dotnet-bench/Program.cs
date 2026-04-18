using BenchmarkDotNet.Running;

namespace BoltFFIBench;

public static class Program
{
    public static int Main(string[] args)
    {
        var summaries = BenchmarkSwitcher
            .FromTypes([typeof(BoltffiWireReaderBenchmarks)])
            .Run(args, new BoltffiBenchConfig());
        return summaries is null ? 1 : 0;
    }
}
