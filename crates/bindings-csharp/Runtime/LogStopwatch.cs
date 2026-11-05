namespace SpacetimeDB;

#if !NET10_0_OR_GREATER || SPACETIMEDB_WASM_RUNTIME
using System.Text;
using SpacetimeDB.Internal;
#endif

public sealed class LogStopwatch : IDisposable
{
#if !NET10_0_OR_GREATER || SPACETIMEDB_WASM_RUNTIME
    private readonly FFI.ConsoleTimerId StopwatchId;
    private bool WasStopped;
#endif

    public LogStopwatch(string name)
    {
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        throw new NotSupportedException(
            "LogStopwatch host timers are unavailable in managed module tests."
        );
#else
        var name_bytes = Encoding.UTF8.GetBytes(name);
        StopwatchId = FFI.console_timer_start(name_bytes, name_bytes.Length);
#endif
    }

    void IDisposable.Dispose()
    {
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        End();
#else
        if (!WasStopped)
        {
            End();
        }
#endif
    }

    public void End()
    {
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        throw new NotSupportedException(
            "LogStopwatch host timers are unavailable in managed module tests."
        );
#else
        FFI.console_timer_end(StopwatchId);
        WasStopped = true;
#endif
    }
}
