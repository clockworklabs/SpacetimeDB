namespace SpacetimeDB;

using System.Text;
using SpacetimeDB.Internal;

public sealed class LogStopwatch : IDisposable
{
    private readonly FFI.ConsoleTimerId StopwatchId;
    private bool WasStopped;

    public LogStopwatch(string name)
    {
        var name_bytes = Encoding.UTF8.GetBytes(name);
        StopwatchId = FFI._console_timer_start(name_bytes, (uint)name_bytes.Length);
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
        FFI._console_timer_end(StopwatchId);
        WasStopped = true;
    }
}
