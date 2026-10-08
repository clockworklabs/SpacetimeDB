#if NET10_0_OR_GREATER
namespace SpacetimeDB;

using System.Diagnostics.CodeAnalysis;

#pragma warning disable STDB_UNSTABLE
#pragma warning disable CA1822 // Preserve the existing instance-based context API.

public sealed class Local : LocalBase
{
    private static readonly Local Root = new();

    private static Local?[] Instances => field ??= new Local?[Internal.Module.InstanceCount];

    internal static Local ForInstance(int instanceId) =>
        instanceId == 0 ? Root : Instances[instanceId] ??= new() { InstanceId = instanceId };
}

public sealed record ReducerContext
    : DbContext<Local>,
        Internal.IReducerContext,
        Internal.IModuleContext<ReducerContext>
{
    private ReducerContext? selectionSource;

    int Internal.IModuleContext.InstanceId => ModuleInstanceId;

    ReducerContext Internal.IModuleContext<ReducerContext>.SelectInstance(int instanceId)
    {
        if (instanceId == ModuleInstanceId)
        {
            return this;
        }
        var selected = this with { Db = Local.ForInstance(instanceId) };
        selected.selectionSource = selectionSource ?? this;
        return selected;
    }

    internal int ModuleInstanceId => Db.InstanceId;
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
        AuthCtx? senderAuth = null,
        int instanceId = 0
    )
        : base(Local.ForInstance(instanceId))
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
    int Internal.IModuleContext.InstanceId => ModuleInstanceId;

    ProcedureContext Internal.IModuleContext<ProcedureContext>.SelectInstance(int instanceId)
    {
        if (instanceId == ModuleInstanceId)
        {
            return this;
        }
        var selected = (ProcedureContext)MemberwiseClone();
        selected.Db = Local.ForInstance(instanceId);
        selected.SelectionSource = SelectionSource ?? this;
        return selected;
    }

    protected override ProcedureTxContextBase SelectTxContext(ProcedureTxContextBase tx) =>
        ((Internal.IModuleContext<ProcedureTxContext>)(ProcedureTxContext)tx).SelectInstance(
            ModuleInstanceId
        );

    internal int ModuleInstanceId => Db.InstanceId;

    internal ProcedureContext(
        Identity identity,
        ConnectionId? connectionId,
        Random random,
        Timestamp time,
        int instanceId = 0
    )
        : base(identity, connectionId, random, time) => Db = Local.ForInstance(instanceId);

    protected internal override global::SpacetimeDB.LocalBase CreateLocal() => Db;

    protected override global::SpacetimeDB.ProcedureTxContextBase CreateTxContext(
        Internal.TxContext inner
    ) => _cached ??= new ProcedureTxContext(inner);

    private ProcedureTxContext? _cached;

    public Local Db { get; private set; }

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
    private Local _db = Local.ForInstance(0);

    int Internal.IModuleContext.InstanceId => _db.InstanceId;

    HandlerContext Internal.IModuleContext<HandlerContext>.SelectInstance(int instanceId)
    {
        if (instanceId == _db.InstanceId)
        {
            return this;
        }
        var selected = (HandlerContext)MemberwiseClone();
        selected._db = Local.ForInstance(instanceId);
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
    int Internal.IModuleContext.InstanceId => Db.InstanceId;

    ProcedureTxContext Internal.IModuleContext<ProcedureTxContext>.SelectInstance(int instanceId)
    {
        if (instanceId == Db.InstanceId)
        {
            return this;
        }
        var selected = (ProcedureTxContext)MemberwiseClone();
        selected.SelectionSource = SelectionSource ?? this;
        selected.LocalDb = Local.ForInstance(instanceId);
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
    int Internal.IModuleContext.InstanceId => Db.InstanceId;

    HandlerTxContext Internal.IModuleContext<HandlerTxContext>.SelectInstance(int instanceId)
    {
        if (instanceId == Db.InstanceId)
        {
            return this;
        }
        var selected = (HandlerTxContext)MemberwiseClone();
        selected.SelectionSource = SelectionSource ?? this;
        selected.LocalDb = Local.ForInstance(instanceId);
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
    int Internal.IModuleContext.InstanceId => ModuleInstanceId;

    ViewContext Internal.IModuleContext<ViewContext>.SelectInstance(int instanceId) =>
        instanceId == ModuleInstanceId
            ? this
            : this with
            {
                Db = Internal.LocalReadOnly.ForInstance(instanceId),
            };

    internal int ModuleInstanceId => Db.InstanceId;
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
    int Internal.IModuleContext.InstanceId => ModuleInstanceId;

    AnonymousViewContext Internal.IModuleContext<AnonymousViewContext>.SelectInstance(
        int instanceId
    ) =>
        instanceId == ModuleInstanceId
            ? this
            : this with
            {
                Db = Internal.LocalReadOnly.ForInstance(instanceId),
            };

    internal int ModuleInstanceId => Db.InstanceId;
    public DatabaseEnvironment Env => default;
    public QueryBuilder From => new(ModuleInstanceId);

    internal AnonymousViewContext(Internal.LocalReadOnly db)
        : base(db) { }
}

#endif
