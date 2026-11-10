namespace SpacetimeDB;

#if !NET10_0_OR_GREATER || SPACETIMEDB_WASM_RUNTIME
using System.Text;
using SpacetimeDB.Internal;
#endif

/// <summary>
/// Logs the time between creation and <see cref="End"/> through the host. In managed module
/// tests there is no host, so it does nothing.
/// </summary>
public sealed class LogStopwatch : IDisposable
{
#if !NET10_0_OR_GREATER || SPACETIMEDB_WASM_RUNTIME
    private readonly FFI.ConsoleTimerId StopwatchId;
    private bool WasStopped;

    public LogStopwatch(string name)
    {
        var name_bytes = Encoding.UTF8.GetBytes(name);
        StopwatchId = FFI.console_timer_start(name_bytes, name_bytes.Length);
    }

    void IDisposable.Dispose()
    {
        if (!WasStopped)
        {
            End();
        }
    }

    public void End()
    {
        FFI.console_timer_end(StopwatchId);
        WasStopped = true;
    }
#else
    // Keep the WASM shape so module code compiles unchanged against both builds.
#pragma warning disable IDE0290, CA1822
    public LogStopwatch(string name) { }

    void IDisposable.Dispose() { }

    public void End() { }
#pragma warning restore IDE0290, CA1822
#endif
}
