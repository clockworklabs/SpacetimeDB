﻿//HintName: Reducers.TestReducerReturnType.cs
// <auto-generated />
#nullable enable

partial class Reducers
{
    [System.Diagnostics.CodeAnalysis.Experimental("STDB_UNSTABLE")]
    public static void VolatileNonatomicScheduleImmediateTestReducerReturnType()
    {
        using var stream = new MemoryStream();
        using var writer = new BinaryWriter(stream);

        SpacetimeDB.Internal.IReducer.VolatileNonatomicScheduleImmediate(
            "TestReducerReturnType",
            stream
        );
    }
} // Reducers