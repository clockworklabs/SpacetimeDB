namespace SpacetimeDB;

using System.Diagnostics.CodeAnalysis;

#pragma warning disable STDB_UNSTABLE
public abstract class ProcedureContextBase : Internal.IInternalProcedureContext
{
    protected ProcedureContextBase(
        Identity sender,
        ConnectionId? connectionId,
        Random random,
        Timestamp time)
    {
        Sender = sender;
        ConnectionId = connectionId;
        Rng = random;
        Timestamp = time;
        SenderAuth = AuthCtx.BuildFromSystemTables(connectionId, sender);
    }

    public Identity Identity => Internal.IProcedureContext.GetIdentity();
    public Identity Sender { get; }
    public ConnectionId? ConnectionId { get; }
    public Random Rng { get; }
    public Timestamp Timestamp { get; private set; }
    public AuthCtx SenderAuth { get; }

    private Internal.TransactionOffset? pendingTxOffset;
    private Internal.TxContext? txContext;
    private ProcedureTxContextBase? cachedUserTxContext;

    protected abstract ProcedureTxContextBase CreateTxContext(Internal.TxContext inner);
    protected internal abstract LocalBase CreateLocal();

    private protected ProcedureTxContextBase RequireTxContext()
    {
        var inner = txContext ?? throw new InvalidOperationException("Transaction context was not initialised.");
        cachedUserTxContext ??= CreateTxContext(inner);
        cachedUserTxContext.Refresh(inner);
        return cachedUserTxContext;
    }

    public Internal.TxContext EnterTxContext(long timestampMicros)
    {
        var timestamp = new Timestamp(timestampMicros);
        Timestamp = timestamp;
        txContext = txContext?.WithTimestamp(timestamp)
            ?? new Internal.TxContext(CreateLocal(), Sender, ConnectionId, timestamp, SenderAuth, Rng);
        return txContext;
    }

    public void ExitTxContext() => txContext = null;

    public void SetTransactionOffset(Internal.TransactionOffset offset) =>
        pendingTxOffset = offset;

    public bool TryTakeTransactionOffset(out Internal.TransactionOffset offset)
    {
        if (pendingTxOffset is { } value)
        {
            pendingTxOffset = null;
            offset = value;
            return true;
        }

        offset = default;
        return false;
    }

    public readonly struct TxOutcome<TResult>
    {
        public TxOutcome(bool isSuccess, TResult? value, Exception? error)
        {
            IsSuccess = isSuccess;
            Value = value;
            Error = error;
        }
    
        public bool IsSuccess { get; }
        public TResult? Value { get; }
        public Exception? Error { get; }
    
        public static TxOutcome<TResult> Success(TResult value) =>
            new(true, value, null);
    
        public static TxOutcome<TResult> Failure(Exception error) =>
            new(false, default, error);
    
        public TResult UnwrapOrThrow() =>
            IsSuccess ? Value! : throw (Error ?? new InvalidOperationException("Transaction failed without an error object."));
    
        public TResult UnwrapOrThrow(Func<Exception> fallbackFactory) =>
            IsSuccess ? Value! : throw (Error ?? fallbackFactory());
    }
    
    public readonly struct TxResult<TResult, TError>
        where TError : Exception
    {
        public TxResult(bool isSuccess, TResult? value, TError? error)
        {
            IsSuccess = isSuccess;
            Value = value;
            Error = error;
        }
        
        public bool IsSuccess { get; }
        public TResult? Value { get; }
        public TError? Error { get; }
    
        public static TxResult<TResult, TError> Success(TResult value) =>
            new(true, value, null);
    
        public static TxResult<TResult, TError> Failure(TError error) =>
            new(false, default, error);
    
        public TResult UnwrapOrThrow()
        {
            if (IsSuccess)
            {
                return Value!;
            }
    
            if (Error is not null)
            {
                throw Error;
            }
    
            throw new InvalidOperationException("Transaction failed without an error object.");
        }
    }
    
    [Experimental("STDB_UNSTABLE")]
    public TResult WithTx<TResult>(Func<ProcedureTxContextBase, TResult> body) =>
        TryWithTx(tx => TxResult<TResult, Exception>.Success(body(tx))).UnwrapOrThrow();
    
    [Experimental("STDB_UNSTABLE")]
    public TxOutcome<TResult> TryWithTx<TResult, TError>(
        Func<ProcedureTxContextBase, TxResult<TResult, TError>> body)
        where TError : Exception
    {
        try
        {
            var result = RunWithRetry(body);
            return result.IsSuccess
                ? TxOutcome<TResult>.Success(result.Value!)
                : TxOutcome<TResult>.Failure(result.Error!);
        }
        catch (Exception ex)
        {
            return TxOutcome<TResult>.Failure(ex);
        }
    }
    
    private TxResult<TResult, TError> RunWithRetry<TResult, TError>(
        Func<ProcedureTxContextBase, TxResult<TResult, TError>> body)
        where TError : Exception
    {
        var result = RunOnce(body);
        if (!result.IsSuccess)
        {
            return result;
        }
    
        bool Retry()
        {
            result = RunOnce(body);
            return result.IsSuccess;
        }
    
        if (!SpacetimeDB.Internal.Procedure.CommitMutTxWithRetry(Retry))
        {
            return result;
        }
    
        if (TryTakeTransactionOffset(out var offset))
        {
            SetTransactionOffset(offset);
            SpacetimeDB.Internal.Module.RecordProcedureTxOffset(offset);
        }
    
        return result;
    }
    
    private TxResult<TResult, TError> RunOnce<TResult, TError>(
        Func<ProcedureTxContextBase, TxResult<TResult, TError>> body)
        where TError : Exception
    {
        _ = SpacetimeDB.Internal.Procedure.StartMutTx();
        using var guard = new AbortGuard(SpacetimeDB.Internal.Procedure.AbortMutTx);
        var txCtx = RequireTxContext();
        var result = body(txCtx);
        if (result.IsSuccess)
        {
            guard.Disarm();
            return result;
        }
    
        SpacetimeDB.Internal.Procedure.AbortMutTx();
        guard.Disarm();
        return result;
    }
    
    private sealed class AbortGuard : IDisposable
    {
        private readonly Action abort;
        private bool disarmed;
    
        public AbortGuard(Action abort) => this.abort = abort;
    
        public void Disarm() => disarmed = true;
    
        public void Dispose()
        {
            if (!disarmed)
            {
                abort();
            }
        }
    }
}

public abstract class ProcedureTxContextBase
{
    protected ProcedureTxContextBase(Internal.TxContext inner)
    {
        Inner = inner;
    }

    internal Internal.TxContext Inner { get; private set; }

    internal void Refresh(Internal.TxContext inner) => Inner = inner;

    public LocalBase Db => (LocalBase)Inner.Db;
    public Identity Sender => Inner.Sender;
    public ConnectionId? ConnectionId => Inner.ConnectionId;
    public Timestamp Timestamp => Inner.Timestamp;
    public AuthCtx SenderAuth => Inner.SenderAuth;
    public Random Rng => Inner.Rng;
}

public abstract class LocalBase : Internal.Local
{
}

public abstract class LocalReadOnlyBase : Internal.LocalReadOnly
{
}

public sealed class ProcedureContext : ProcedureContextBase
{
    private readonly Local _db = new();

    public ProcedureContext(
        Identity sender,
        ConnectionId? connectionId,
        Random random,
        Timestamp timestamp)
        : base(sender, connectionId, random, timestamp)
    {
    }

    protected internal override LocalBase CreateLocal() => _db;
    protected override ProcedureTxContextBase CreateTxContext(Internal.TxContext inner) =>
        _cached ??= new ProcedureTxContext(inner);

    private ProcedureTxContext? _cached;
}

public sealed class ProcedureTxContext : ProcedureTxContextBase
{
    internal ProcedureTxContext(Internal.TxContext inner)
        : base(inner)
    {
    }

    public new Local Db => (Local)base.Db;
}

public sealed class Local : LocalBase
{
}

public sealed class LocalReadOnly : LocalReadOnlyBase
{
}
#pragma warning restore STDB_UNSTABLE