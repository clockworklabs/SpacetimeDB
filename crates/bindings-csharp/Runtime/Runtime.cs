namespace SpacetimeDB;

using System.Runtime.CompilerServices;
using SpacetimeDB.Internal;
using static System.Text.Encoding;

public class ReducerContext
{
    public readonly Identity Sender;
    public readonly DateTimeOffset Time;
    public readonly Address? Address;

    internal ReducerContext(byte[] senderIdentity, byte[] senderAddress, ulong timestamp_us)
    {
        Sender = new(senderIdentity);
        Address = Address.From(senderAddress);
        // timestamp is in microseconds; the easiest way to convert those w/o losing precision is to get Unix origin and add ticks which are 0.1ms each.
        Time = DateTimeOffset.UnixEpoch.AddTicks(10 * (long)timestamp_us);
    }
}

public class ScheduleToken
{
    private readonly FFI.ScheduleToken handle;

    internal ScheduleToken(FFI.ScheduleToken handle) => this.handle = handle;

    public void Cancel() => FFI._cancel_reducer(handle);
}

public static class Runtime
{
    public enum LogLevel : byte
    {
        Error,
        Warn,
        Info,
        Debug,
        Trace,
        Panic
    }

    public static void Log(
        string text,
        LogLevel level = LogLevel.Info,
        [CallerMemberName] string target = "",
        [CallerFilePath] string filename = "",
        [CallerLineNumber] uint lineNumber = 0
    )
    {
        var target_bytes = UTF8.GetBytes(target);
        var filename_bytes = UTF8.GetBytes(filename);
        var text_bytes = UTF8.GetBytes(text);

        FFI._console_log(
            (byte)level,
            target_bytes,
            (uint)target_bytes.Length,
            filename_bytes,
            (uint)filename_bytes.Length,
            lineNumber,
            text_bytes,
            (uint)text_bytes.Length
        );
    }

    // An instance of `System.Random` that is reseeded by each reducer's timestamp.
    public static Random Random { get; internal set; } = new();
}
