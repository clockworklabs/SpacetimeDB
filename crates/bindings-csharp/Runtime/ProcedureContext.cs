namespace SpacetimeDB;

#pragma warning disable STDB_UNSTABLE
public abstract class ProcedureContextBase : Internal.IInternalProcedureContext
{
    public DatabaseEnvironment Env { get; } = DatabaseEnvironment.Instance;
    public static Identity Identity => Internal.IProcedureContext.GetIdentity();
    public Identity Sender { get; }
    public ConnectionId? ConnectionId { get; }
    public Random Rng => txState.Rng;
    public Timestamp Timestamp => txState.Timestamp;
    public AuthCtx SenderAuth { get; }

    // NOTE: The host rejects procedure HTTP requests while a mut transaction is open
    // (WOULD_BLOCK_TRANSACTION). Avoid calling `Http.*` inside WithTx.
    public HttpClient Http { get; } = new();

    // **Note:** must be 0..=u32::MAX
    protected int CounterUuid = 0;
#if NET10_0_OR_GREATER
    private protected ProcedureContextBase? SelectionSource;
    protected ref int SharedCounterUuid => ref (SelectionSource ?? this).CounterUuid;

    protected virtual ProcedureTxContextBase SelectTxContext(ProcedureTxContextBase tx) => tx;

    // Keep captured callbacks off the ordinary, unselected transaction path.
    private TResult WithSelectedTx<TResult>(Func<ProcedureTxContextBase, TResult> body) =>
        SelectionSource!.WithTx(tx => body(SelectTxContext(tx)));

    private TxOutcome<TResult> TryWithSelectedTx<TResult, TError>(
        Func<ProcedureTxContextBase, Result<TResult, TError>> body
    )
        where TError : Exception =>
        SelectionSource!.TryWithTx<TResult, TError>(tx => body(SelectTxContext(tx)));
#endif

    private readonly TransactionalContextState<ProcedureTxContextBase> txState;

    protected ProcedureContextBase(
        Identity sender,
        ConnectionId? connectionId,
        Random random,
        Timestamp time
    )
    {
        Sender = sender;
        ConnectionId = connectionId;
        SenderAuth = AuthCtx.BuildFromSystemTables(connectionId, sender);
        txState = new(
            random,
            time,
            timestamp => new Internal.TxContext(
                CreateLocal(),
                Sender,
                ConnectionId,
                timestamp,
                SenderAuth,
                random
            ),
            inner => CreateTxContext(inner)
        );
    }

    protected abstract ProcedureTxContextBase CreateTxContext(Internal.TxContext inner);
    protected internal abstract LocalBase CreateLocal();

    public Internal.TxContext EnterTxContext(long timestampMicros) =>
        txState.EnterTxContext(timestampMicros);

    public void ExitTxContext() => txState.ExitTxContext();

    public readonly struct TxOutcome<TResult>(bool isSuccess, TResult? value, Exception? error)
    {
        public bool IsSuccess { get; } = isSuccess;
        public TResult? Value { get; } = value;
        public Exception? Error { get; } = error;

        public static TxOutcome<TResult> Success(TResult value) => new(true, value, null);

        public static TxOutcome<TResult> Failure(Exception error) => new(false, default, error);

        public TResult UnwrapOrThrow() =>
            IsSuccess
                ? Value!
                : throw (
                    Error
                    ?? new InvalidOperationException("Transaction failed without an error object.")
                );

        public TResult UnwrapOrThrow(Func<Exception> fallbackFactory) =>
            IsSuccess ? Value! : throw (Error ?? fallbackFactory());
    }

    public TResult WithTx<TResult>(Func<ProcedureTxContextBase, TResult> body)
    {
#if NET10_0_OR_GREATER
        if (SelectionSource is not null)
        {
            return WithSelectedTx(body);
        }
#endif
        return txState.WithTx(body);
    }

    public TxOutcome<TResult> TryWithTx<TResult, TError>(
        Func<ProcedureTxContextBase, Result<TResult, TError>> body
    )
        where TError : Exception
    {
#if NET10_0_OR_GREATER
        if (SelectionSource is not null)
        {
            return TryWithSelectedTx<TResult, TError>(body);
        }
#endif
        var outcome = txState.TryWithTx(body);
        return outcome.IsSuccess
            ? TxOutcome<TResult>.Success(outcome.Value!)
            : TxOutcome<TResult>.Failure(
                outcome.Error
                    ?? new InvalidOperationException("Transaction failed without an error object.")
            );
    }
}

public abstract class ProcedureTxContextBase(Internal.TxContext inner) : IRefreshableTxContext
{
#if NET10_0_OR_GREATER
    private protected ProcedureTxContextBase? SelectionSource;
    private protected LocalBase LocalDb = (LocalBase)inner.Db;
    private Internal.TxContext currentInner = inner;

    // Selected wrappers must observe refreshes of the original transaction context.
    internal Internal.TxContext Inner => SelectionSource?.Inner ?? currentInner;

    internal void Refresh(Internal.TxContext inner)
    {
        currentInner = inner;
        LocalDb = (LocalBase)inner.Db;
    }
#else
    internal Internal.TxContext Inner { get; private set; } = inner;

    internal void Refresh(Internal.TxContext inner) => Inner = inner;
#endif

    void IRefreshableTxContext.Refresh(Internal.TxContext inner) => Refresh(inner);

#if NET10_0_OR_GREATER
    public LocalBase Db => LocalDb;
#else
    public LocalBase Db => (LocalBase)Inner.Db;
#endif
    public DatabaseEnvironment Env { get; } = DatabaseEnvironment.Instance;
    public Identity Sender => Inner.Sender;
    public ConnectionId? ConnectionId => Inner.ConnectionId;
    public Timestamp Timestamp => Inner.Timestamp;
    public AuthCtx SenderAuth => Inner.SenderAuth;
    public Random Rng => Inner.Rng;
}

public abstract class LocalBase : Internal.Local { }

internal sealed partial class RuntimeProcedureContext(
    Identity sender,
    ConnectionId? connectionId,
    Random random,
    Timestamp timestamp
) : ProcedureContextBase(sender, connectionId, random, timestamp)
{
    private readonly RuntimeLocal _db = new();

    protected internal override LocalBase CreateLocal() => _db;

    protected override ProcedureTxContextBase CreateTxContext(Internal.TxContext inner) =>
        _cached ??= new RuntimeProcedureTxContext(inner);

    private RuntimeProcedureTxContext? _cached;
}

internal sealed class RuntimeProcedureTxContext : ProcedureTxContextBase
{
    internal RuntimeProcedureTxContext(Internal.TxContext inner)
        : base(inner) { }

    public new RuntimeLocal Db => (RuntimeLocal)base.Db;
}

internal sealed class RuntimeLocal : LocalBase { }

#pragma warning restore STDB_UNSTABLE
