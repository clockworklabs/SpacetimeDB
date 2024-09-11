﻿//HintName: Reducers.ScheduleImmediate.cs
// <auto-generated />
#nullable enable

partial class Reducers
{
    public static void VolatileNonatomicScheduleImmediateScheduleImmediate(PublicTable data)
    {
        using var stream = new MemoryStream();
        using var writer = new BinaryWriter(stream);
        new PublicTable.BSATN().Write(writer, data);
        SpacetimeDB.Internal.IReducer.VolatileNonatomicScheduleImmediate(
            "ScheduleImmediate",
            stream
        );
    }
} // Reducers
