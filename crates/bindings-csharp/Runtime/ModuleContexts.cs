#if NET10_0_OR_GREATER
namespace SpacetimeDB;

using System.Diagnostics.CodeAnalysis;

#pragma warning disable STDB_UNSTABLE
#pragma warning disable CA1822 // Preserve the existing instance-based context API.

public sealed class Local : LocalBase { }

public sealed record ReducerContext
    : DbContext<Local>,
        Internal.IReducerContext,
        Internal.IModuleContext<ReducerContext>
{
    private ReducerContext? selectionSource;

    int Internal.IModuleContext<ReducerContext>.InstanceId => ModuleInstanceId;

    ReducerContext Internal.IModuleContext<ReducerContext>.SelectInstance(int instanceId)
    {
        if (instanceId == ModuleInstanceId)
        {
            return this;
        }
        var selected = this with { Db = new Local { InstanceId = instanceId } };
        selected.selectionSource = selectionSource ?? this;
        return selected;
    }

    internal int ModuleInstanceId
    {
        get => Db.InstanceId;
        set => Db.InstanceId = value;
    }
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
        return Uuid.FromCounterV7(ref (selectionSource ?? this).CounterUuid, Timestamp, bytes);
    }
}

public readonly struct QueryBuilder
{
    internal readonly int InstanceId;

    internal QueryBuilder(int instanceId) => InstanceId = instanceId;
}

public sealed partial class ProcedureContext
    : global::SpacetimeDB.ProcedureContextBase,
        Internal.IModuleContext<ProcedureContext>
{
    int Internal.IModuleContext<ProcedureContext>.InstanceId => ModuleInstanceId;

    ProcedureContext Internal.IModuleContext<ProcedureContext>.SelectInstance(int instanceId)
    {
        if (instanceId == ModuleInstanceId)
        {
            return this;
        }
        var selected = (ProcedureContext)MemberwiseClone();
        selected.Db = new Local { InstanceId = instanceId };
        selected.SelectionSource = SelectionSource ?? this;
        return selected;
    }

    protected override ProcedureTxContextBase SelectTxContext(ProcedureTxContextBase tx) =>
        ((Internal.IModuleContext<ProcedureTxContext>)(ProcedureTxContext)tx).SelectInstance(
            ModuleInstanceId
        );

    internal int ModuleInstanceId
    {
        get => Db.InstanceId;
        set => Db.InstanceId = value;
    }

    internal ProcedureContext(
        Identity identity,
        ConnectionId? connectionId,
        Random random,
        Timestamp time
    )
        : base(identity, connectionId, random, time) { }

    protected internal override global::SpacetimeDB.LocalBase CreateLocal() => Db;

    protected override global::SpacetimeDB.ProcedureTxContextBase CreateTxContext(
        Internal.TxContext inner
    ) => _cached ??= new ProcedureTxContext(inner);

    private ProcedureTxContext? _cached;

    public Local Db { get; private set; } = new();

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
        return Uuid.FromCounterV7(ref SharedCounterUuid, Timestamp, bytes);
    }
}

public sealed partial class HandlerContext
    : global::SpacetimeDB.HandlerContextBase,
        Internal.IModuleContext<HandlerContext>
{
    private Local _db = new();

    int Internal.IModuleContext<HandlerContext>.InstanceId => _db.InstanceId;

    HandlerContext Internal.IModuleContext<HandlerContext>.SelectInstance(int instanceId)
    {
        if (instanceId == _db.InstanceId)
        {
            return this;
        }
        var selected = (HandlerContext)MemberwiseClone();
        selected._db = new Local { InstanceId = instanceId };
        selected.SelectionSource = SelectionSource ?? this;
        return selected;
    }

    protected override HandlerTxContextBase SelectTxContext(HandlerTxContextBase tx) =>
        ((Internal.IModuleContext<HandlerTxContext>)(HandlerTxContext)tx).SelectInstance(
            _db.InstanceId
        );

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
        return Uuid.FromCounterV7(ref SharedCounterUuid, Timestamp, bytes);
    }
}

public sealed class ProcedureTxContext
    : global::SpacetimeDB.ProcedureTxContextBase,
        Internal.IModuleContext<ProcedureTxContext>
{
    int Internal.IModuleContext<ProcedureTxContext>.InstanceId => Db.InstanceId;

    ProcedureTxContext Internal.IModuleContext<ProcedureTxContext>.SelectInstance(int instanceId)
    {
        if (instanceId == Db.InstanceId)
        {
            return this;
        }
        var selected = (ProcedureTxContext)MemberwiseClone();
        selected.SelectionSource = SelectionSource ?? this;
        selected.LocalDb = new Local { InstanceId = instanceId };
        return selected;
    }

    internal ProcedureTxContext(Internal.TxContext inner)
        : base(inner) { }

    public new Local Db => (Local)base.Db;
}

[Experimental("STDB_UNSTABLE")]
public sealed class HandlerTxContext
    : global::SpacetimeDB.HandlerTxContextBase,
        Internal.IModuleContext<HandlerTxContext>
{
    int Internal.IModuleContext<HandlerTxContext>.InstanceId => Db.InstanceId;

    HandlerTxContext Internal.IModuleContext<HandlerTxContext>.SelectInstance(int instanceId)
    {
        if (instanceId == Db.InstanceId)
        {
            return this;
        }
        var selected = (HandlerTxContext)MemberwiseClone();
        selected.SelectionSource = SelectionSource ?? this;
        selected.LocalDb = new Local { InstanceId = instanceId };
        return selected;
    }

    internal HandlerTxContext(Internal.TxContext inner)
        : base(inner) { }

    public new Local Db => (Local)base.Db;
}

public sealed record ViewContext
    : DbContext<Internal.LocalReadOnly>,
        Internal.IViewContext,
        Internal.IModuleContext<ViewContext>
{
    int Internal.IModuleContext<ViewContext>.InstanceId => ModuleInstanceId;

    ViewContext Internal.IModuleContext<ViewContext>.SelectInstance(int instanceId) =>
        instanceId == ModuleInstanceId
            ? this
            : this with
            {
                Db = new Internal.LocalReadOnly { InstanceId = instanceId },
            };

    internal int ModuleInstanceId
    {
        get => Db.InstanceId;
        set => Db.InstanceId = value;
    }
    public DatabaseEnvironment Env => default;
    public Identity Sender { get; }

    public QueryBuilder From => new(ModuleInstanceId);

    internal ViewContext(Identity sender, Internal.LocalReadOnly db)
        : base(db)
    {
        Sender = sender;
    }
}

public sealed record AnonymousViewContext
    : DbContext<Internal.LocalReadOnly>,
        Internal.IAnonymousViewContext,
        Internal.IModuleContext<AnonymousViewContext>
{
    int Internal.IModuleContext<AnonymousViewContext>.InstanceId => ModuleInstanceId;

    AnonymousViewContext Internal.IModuleContext<AnonymousViewContext>.SelectInstance(
        int instanceId
    ) =>
        instanceId == ModuleInstanceId
            ? this
            : this with
            {
                Db = new Internal.LocalReadOnly { InstanceId = instanceId },
            };

    internal int ModuleInstanceId
    {
        get => Db.InstanceId;
        set => Db.InstanceId = value;
    }
    public DatabaseEnvironment Env => default;
    public QueryBuilder From => new(ModuleInstanceId);

    internal AnonymousViewContext(Internal.LocalReadOnly db)
        : base(db) { }
}

#endif
