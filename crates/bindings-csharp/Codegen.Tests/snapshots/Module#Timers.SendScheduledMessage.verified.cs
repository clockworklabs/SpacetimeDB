﻿//HintName: Timers.SendScheduledMessage.cs

// <auto-generated />
#nullable enable

partial class Timers
{
    public static SpacetimeDB.ScheduleToken ScheduleSendScheduledMessage(
        DateTimeOffset time,
        Timers.SendMessageTimer arg
    )
    {
        using var stream = new MemoryStream();
        using var writer = new BinaryWriter(stream);
        new Timers.SendMessageTimer.BSATN().Write(writer, arg);
        return SpacetimeDB.Internal.IReducer.Schedule("SendScheduledMessage", stream, time);
    }
} // Timers
