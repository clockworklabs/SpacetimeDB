#if NET10_0_OR_GREATER
namespace SpacetimeDB;

public sealed class Local : LocalBase { }

public sealed record ReducerContext : DbContext<Local>, Internal.IReducerContext {
    public readonly Identity Sender;
    public readonly ConnectionId? ConnectionId;
    public readonly Random Rng;
    public readonly Timestamp Timestamp;
    public readonly AuthCtx SenderAuth;
    // **Note:** must be 0..=u32::MAX
    internal int CounterUuid;
    public Identity DatabaseIdentity => Internal.IReducerContext.GetDatabaseIdentity();
    // We keep this property for compatibility with existing module code.
    [global::System.Obsolete("ReducerContext.Identity is deprecated. Use DatabaseIdentity instead.")]
    public Identity Identity => DatabaseIdentity;

    internal ReducerContext(Identity identity, ConnectionId? connectionId, Random random,
                    Timestamp time, AuthCtx? senderAuth = null)
    {
        Sender = identity;
        ConnectionId = connectionId;
        Rng = random;
        Timestamp = time;
        SenderAuth = senderAuth ?? AuthCtx.BuildFromSystemTables(connectionId, identity);
        CounterUuid = 0;
    }
    /// <summary>
    /// Create a new random <see cref="Uuid"/> `v4` using the built-in RNG.
    /// </summary>
    /// <remarks>
    /// This method fills the random bytes using the context RNG.
    /// </remarks>
    /// <example>
    /// <code>
    /// var uuid = ctx.NewUuidV4();
    /// Log.Info(uuid);
    /// </code>
    /// </example>
    public Uuid NewUuidV4()
    {
        var bytes = new byte[16];
        Rng.NextBytes(bytes);
        return Uuid.FromRandomBytesV4(bytes);
    }

    /// <summary>
    /// Create a new sortable <see cref="Uuid"/> `v7` using the built-in RNG, monotonic counter,
    /// and timestamp.
    /// </summary>
    /// <returns>
    /// A newly generated <see cref="Uuid"/> `v7` that is monotonically ordered
    /// and suitable for use as a primary key or for ordered storage.
    /// </returns>
    /// <exception cref="Exception">
    /// Thrown if <see cref="Uuid"/> generation fails.
    /// </exception>
    /// <example>
    /// <code>
    /// [SpacetimeDB.Reducer]
    /// public static Guid GenerateUuidV7(ReducerContext ctx)
    /// {
    ///     Guid uuid = ctx.NewUuidV7();
    ///     Log.Info(uuid);
    /// }
    /// </code>
    /// </example>
    public Uuid NewUuidV7()
    {
        var bytes = new byte[4];
        Rng.NextBytes(bytes);
        return Uuid.FromCounterV7(ref CounterUuid, Timestamp, bytes);
    }
}
#endif