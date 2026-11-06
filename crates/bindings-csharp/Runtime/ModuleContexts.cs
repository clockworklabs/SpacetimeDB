#if NET10_0_OR_GREATER
namespace SpacetimeDB;

using System.Diagnostics.CodeAnalysis;

#pragma warning disable STDB_UNSTABLE
#pragma warning disable CA1822 // Preserve the existing instance-based context API.

public sealed class Local : LocalBase
{
    public Local() { }

#if !SPACETIMEDB_WASM_RUNTIME
    internal Local(Internal.DatabaseAccess access)
        : base(access) { }
#endif

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

#if !SPACETIMEDB_WASM_RUNTIME
    Internal.DatabaseAccess? Internal.IModuleContext.Access => Db.Access;
#endif

    ReducerContext Internal.IModuleContext<ReducerContext>.SelectInstance(int instanceId)
    {
        if (instanceId == ModuleInstanceId)
        {
            return this;
        }
        var selected = this with
        {
#if !SPACETIMEDB_WASM_RUNTIME
            Db = Db.Access is { } access
                ? new Local(access) { InstanceId = instanceId }
                : Local.ForInstance(instanceId),
#else
            Db = Local.ForInstance(instanceId),
#endif
        };
        selected.selectionSource = selectionSource ?? this;
        return selected;
    }

    internal int ModuleInstanceId => Db.InstanceId;
    public DatabaseEnvironment Env =>
#if SPACETIMEDB_WASM_RUNTIME
        default;
#else
        new(Db.Access);
#endif
    public readonly Identity Sender;
    public readonly ConnectionId? ConnectionId;
    public readonly Random Rng;
    public readonly Timestamp Timestamp;
    public readonly AuthCtx SenderAuth;

    // **Note:** must be 0..=u32::MAX
    internal int CounterUuid;
    public Identity DatabaseIdentity
    {
        get
        {
#if SPACETIMEDB_WASM_RUNTIME
            return Internal.IReducerContext.GetDatabaseIdentity();
#else
            var access = Internal.DatabaseAccess.Require(Db.Access);
            access.CheckActive();
            return access.Backend.DatabaseIdentity;
#endif
        }
    }

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
        : this(
            Local.ForInstance(instanceId),
            identity,
            connectionId,
            random,
            time,
            senderAuth ?? AuthCtx.BuildFromSystemTables(connectionId, identity)
        ) { }

    internal ReducerContext(
        Local db,
        Identity identity,
        ConnectionId? connectionId,
        Random random,
        Timestamp time,
        AuthCtx senderAuth
    )
        : base(db)
    {
        Sender = identity;
        ConnectionId = connectionId;
        Rng = random;
        Timestamp = time;
        SenderAuth = senderAuth;
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

#if !SPACETIMEDB_WASM_RUNTIME
    internal readonly Internal.DatabaseAccess? Access;

    internal QueryBuilder(Internal.LocalReadOnly db)
    {
        InstanceId = db.InstanceId;
        Access = db.Access;
    }

    [System.ComponentModel.EditorBrowsable(System.ComponentModel.EditorBrowsableState.Never)]
    public SqlTableName ResolveTableName(string assemblyIdentity, string table)
    {
        var access = Internal.DatabaseAccess.Require(Access);
        access.CheckActive();
        var namespaces = access.Backend.Namespaces;
        return namespaces.ResolveSqlName(
            namespaces.ResolveInstance(InstanceId, assemblyIdentity),
            table
        );
    }
#endif
}

public sealed partial class ProcedureContext
    : global::SpacetimeDB.ProcedureContextBase,
        Internal.IModuleContext<ProcedureContext>
{
    int Internal.IModuleContext.InstanceId => ModuleInstanceId;

#if !SPACETIMEDB_WASM_RUNTIME
    Internal.DatabaseAccess? Internal.IModuleContext.Access => Access;
#endif

    ProcedureContext Internal.IModuleContext<ProcedureContext>.SelectInstance(int instanceId)
    {
        if (instanceId == ModuleInstanceId)
        {
            return this;
        }
        var selected = (ProcedureContext)MemberwiseClone();
#if !SPACETIMEDB_WASM_RUNTIME
        selected.Db = new Local { InstanceId = instanceId };
#else
        selected.Db = Local.ForInstance(instanceId);
#endif
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
    ) =>
#if SPACETIMEDB_WASM_RUNTIME
        _cached ??= new ProcedureTxContext(inner);

    private ProcedureTxContext? _cached;
#else
        new ProcedureTxContext(inner);

    internal ProcedureContext(
        Internal.IDatabaseBackend backend,
        Func<Timestamp> clock,
        Identity sender,
        ConnectionId? connectionId,
        Random random,
        AuthCtx auth,
        HttpClient http,
        Action? afterCommit
    )
        : base(backend, clock, sender, connectionId, random, auth, http, afterCommit) =>
        Db = new Local();
#endif

    public Local Db { get; private set; }

    public TResult WithTx<TResult>(Func<ProcedureTxContext, TResult> body)
    {
#if !SPACETIMEDB_WASM_RUNTIME
        Internal.TransactionCallback.Validate(body, typeof(TResult));
#endif
        return WithTxCore(tx => body((ProcedureTxContext)tx));
    }

    public TxOutcome<TResult> TryWithTx<TResult, TError>(
        Func<ProcedureTxContext, Result<TResult, TError>> body
    )
        where TError : Exception
    {
#if !SPACETIMEDB_WASM_RUNTIME
        Internal.TransactionCallback.Validate(body, typeof(TResult));
#endif
        return TryWithTxCore(tx => body((ProcedureTxContext)tx));
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

    internal HandlerContext(Identity sender, string? senderJwt, Random random, Timestamp time)
        : base(sender, senderJwt, random, time) { }

    protected override global::SpacetimeDB.LocalBase CreateLocal() => _db;

    protected override global::SpacetimeDB.HandlerTxContextBase CreateTxContext(
        Internal.TxContext inner
    ) => _cached ??= new HandlerTxContext(inner);

    private HandlerTxContext? _cached;

    [Experimental("STDB_UNSTABLE")]
    public TResult WithTx<TResult>(Func<HandlerTxContext, TResult> body)
    {
#if !SPACETIMEDB_WASM_RUNTIME
        Internal.TransactionCallback.Validate(body, typeof(TResult));
#endif
        return WithTxCore(tx => body((HandlerTxContext)tx));
    }

    [Experimental("STDB_UNSTABLE")]
    public TxOutcome<TResult> TryWithTx<TResult, TError>(
        Func<HandlerTxContext, Result<TResult, TError>> body
    )
        where TError : Exception
    {
#if !SPACETIMEDB_WASM_RUNTIME
        Internal.TransactionCallback.Validate(body, typeof(TResult));
#endif
        return TryWithTxCore(tx => body((HandlerTxContext)tx));
    }

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

#if !SPACETIMEDB_WASM_RUNTIME
    Internal.DatabaseAccess? Internal.IModuleContext.Access => Db.Access;
#endif

    ProcedureTxContext Internal.IModuleContext<ProcedureTxContext>.SelectInstance(int instanceId)
    {
        if (instanceId == Db.InstanceId)
        {
            return this;
        }
        var selected = (ProcedureTxContext)MemberwiseClone();
        selected.SelectionSource = SelectionSource ?? this;
#if !SPACETIMEDB_WASM_RUNTIME
        selected.LocalDb = Db.Access is { } access
            ? new Local(access) { InstanceId = instanceId }
            : Local.ForInstance(instanceId);
#else
        selected.LocalDb = Local.ForInstance(instanceId);
#endif
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

#if !SPACETIMEDB_WASM_RUNTIME
    Internal.DatabaseAccess? Internal.IModuleContext.Access => Db.Access;
#endif

    ViewContext Internal.IModuleContext<ViewContext>.SelectInstance(int instanceId) =>
        instanceId == ModuleInstanceId
            ? this
            : this with
            {
#if !SPACETIMEDB_WASM_RUNTIME
                Db = Db.Access is { } access
                    ? new Internal.LocalReadOnly(access) { InstanceId = instanceId }
                    : Internal.LocalReadOnly.ForInstance(instanceId),
#else
                Db = Internal.LocalReadOnly.ForInstance(instanceId),
#endif
            };

    internal int ModuleInstanceId => Db.InstanceId;
    public DatabaseEnvironment Env =>
#if SPACETIMEDB_WASM_RUNTIME
        default;
#else
        new(Db.Access);
#endif
    public Identity Sender { get; }

    public QueryBuilder From =>
#if SPACETIMEDB_WASM_RUNTIME
        new(ModuleInstanceId);
#else
        new(Db);
#endif

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

#if !SPACETIMEDB_WASM_RUNTIME
    Internal.DatabaseAccess? Internal.IModuleContext.Access => Db.Access;
#endif

    AnonymousViewContext Internal.IModuleContext<AnonymousViewContext>.SelectInstance(
        int instanceId
    ) =>
        instanceId == ModuleInstanceId
            ? this
            : this with
            {
#if !SPACETIMEDB_WASM_RUNTIME
                Db = Db.Access is { } access
                    ? new Internal.LocalReadOnly(access) { InstanceId = instanceId }
                    : Internal.LocalReadOnly.ForInstance(instanceId),
#else
                Db = Internal.LocalReadOnly.ForInstance(instanceId),
#endif
            };

    internal int ModuleInstanceId => Db.InstanceId;
    public DatabaseEnvironment Env =>
#if SPACETIMEDB_WASM_RUNTIME
        default;
#else
        new(Db.Access);
#endif
    public QueryBuilder From =>
#if SPACETIMEDB_WASM_RUNTIME
        new(ModuleInstanceId);
#else
        new(Db);
#endif

    internal AnonymousViewContext(Internal.LocalReadOnly db)
        : base(db) { }
}

#endif
