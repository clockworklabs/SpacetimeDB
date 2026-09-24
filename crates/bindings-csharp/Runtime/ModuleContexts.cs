#if NET10_0_OR_GREATER
namespace SpacetimeDB;

using System.Diagnostics.CodeAnalysis;

#pragma warning disable STDB_UNSTABLE
#pragma warning disable CA1822 // Preserve the existing instance-based context API.

public sealed class Local : LocalBase { }

public sealed record ReducerContext : DbContext<Local>, Internal.IReducerContext
{
    public DatabaseEnvironment Env => default;
    public readonly Identity Sender;
    public readonly ConnectionId? ConnectionId;
    public readonly Random Rng;
    public readonly Timestamp Timestamp;
    public readonly AuthCtx SenderAuth;

    // **Note:** must be 0..=u32::MAX
    internal int CounterUuid;
    public Identity DatabaseIdentity => Internal.IReducerContext.GetDatabaseIdentity();

    // We keep this property for compatibility with existing module code.
    [global::System.Obsolete(
        "ReducerContext.Identity is deprecated. Use DatabaseIdentity instead."
    )]
    public Identity Identity => DatabaseIdentity;

    internal ReducerContext(
        Identity identity,
        ConnectionId? connectionId,
        Random random,
        Timestamp time,
        AuthCtx? senderAuth = null
    )
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

public readonly struct QueryBuilder { }

public sealed partial class ProcedureContext : global::SpacetimeDB.ProcedureContextBase
{
    private readonly Local _db = new();

    internal ProcedureContext(
        Identity identity,
        ConnectionId? connectionId,
        Random random,
        Timestamp time
    )
        : base(identity, connectionId, random, time) { }

    protected internal override global::SpacetimeDB.LocalBase CreateLocal() => _db;

    protected override global::SpacetimeDB.ProcedureTxContextBase CreateTxContext(
        Internal.TxContext inner
    ) => _cached ??= new ProcedureTxContext(inner);

    private ProcedureTxContext? _cached;

    public Local Db => _db;

    public TResult WithTx<TResult>(Func<ProcedureTxContext, TResult> body) =>
        base.WithTx(tx => body((ProcedureTxContext)tx));

    public TxOutcome<TResult> TryWithTx<TResult, TError>(
        Func<ProcedureTxContext, Result<TResult, TError>> body
    )
        where TError : Exception => base.TryWithTx(tx => body((ProcedureTxContext)tx));

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
    /// Thrown if UUID generation fails.
    /// </exception>
    /// <example>
    /// <code>
    /// [SpacetimeDB.Procedure]
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

public sealed partial class HandlerContext : global::SpacetimeDB.HandlerContextBase
{
    private readonly Local _db = new();

    internal HandlerContext(Random random, Timestamp time)
        : base(random, time) { }

    protected override global::SpacetimeDB.LocalBase CreateLocal() => _db;

    protected override global::SpacetimeDB.HandlerTxContextBase CreateTxContext(
        Internal.TxContext inner
    ) => _cached ??= new HandlerTxContext(inner);

    private HandlerTxContext? _cached;

    [Experimental("STDB_UNSTABLE")]
    public TResult WithTx<TResult>(Func<HandlerTxContext, TResult> body) =>
        base.WithTx(tx => body((HandlerTxContext)tx));

    [Experimental("STDB_UNSTABLE")]
    public TxOutcome<TResult> TryWithTx<TResult, TError>(
        Func<HandlerTxContext, Result<TResult, TError>> body
    )
        where TError : Exception => base.TryWithTx(tx => body((HandlerTxContext)tx));

    public Uuid NewUuidV4()
    {
        var bytes = new byte[16];
        Rng.NextBytes(bytes);
        return Uuid.FromRandomBytesV4(bytes);
    }

    public Uuid NewUuidV7()
    {
        var bytes = new byte[4];
        Rng.NextBytes(bytes);
        return Uuid.FromCounterV7(ref CounterUuid, Timestamp, bytes);
    }
}

public sealed class ProcedureTxContext : global::SpacetimeDB.ProcedureTxContextBase
{
    internal ProcedureTxContext(Internal.TxContext inner)
        : base(inner) { }

    public new Local Db => (Local)base.Db;
}

[Experimental("STDB_UNSTABLE")]
public sealed class HandlerTxContext : global::SpacetimeDB.HandlerTxContextBase
{
    internal HandlerTxContext(Internal.TxContext inner)
        : base(inner) { }

    public new Local Db => (Local)base.Db;
}

public sealed record ViewContext : DbContext<Internal.LocalReadOnly>, Internal.IViewContext
{
    public DatabaseEnvironment Env => default;
    public Identity Sender { get; }

    public QueryBuilder From => default;

    internal ViewContext(Identity sender, Internal.LocalReadOnly db)
        : base(db)
    {
        Sender = sender;
    }
}

public sealed record AnonymousViewContext
    : DbContext<Internal.LocalReadOnly>,
        Internal.IAnonymousViewContext
{
    public DatabaseEnvironment Env => default;
    public QueryBuilder From => default;

    internal AnonymousViewContext(Internal.LocalReadOnly db)
        : base(db) { }
}

#endif
