namespace SpacetimeDB.Internal;

using System.Runtime.InteropServices;

// We store time information in microseconds in internal usages.
//
// These utils allow to encode it as such in FFI and BSATN contexts
// and convert to standard C# types.

[StructLayout(LayoutKind.Sequential)] // we should be able to use it in FFI
[SpacetimeDB.Type] // we should be able to encode it to BSATN too
public partial struct DateTimeOffsetRepr(DateTimeOffset time)
{
    public ulong MicrosecondsSinceEpoch = ScheduleAt.ToMicrosecondsSinceUnixEpoch(time);

    public readonly DateTimeOffset ToStd() =>
        ScheduleAt.DateTimeOffsetFromMicrosSinceUnixEpoch(MicrosecondsSinceEpoch);
}

[StructLayout(LayoutKind.Sequential)] // we should be able to use it in FFI
[SpacetimeDB.Type] // we should be able to encode it to BSATN too
public partial struct TimeSpanRepr(TimeSpan duration)
{
    public ulong Microseconds = ScheduleAt.ToMicroseconds(duration);

    public readonly TimeSpan ToStd() => ScheduleAt.TimeSpanFromMicroseconds(Microseconds);
}
