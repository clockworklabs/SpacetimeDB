namespace SpacetimeDB;

using System.Diagnostics.CodeAnalysis;
using Internal;

#pragma warning disable STDB_UNSTABLE
public abstract class ProcedureContextBase(
    Identity sender,
    ConnectionId? connectionId,
    Random random,
    Timestamp time
) : Internal.IInternalProcedureContext
{
    public static Identity Identity => Internal.IProcedureContext.GetIdentity();
    public Identity Sender { get; } = sender;
    public ConnectionId? ConnectionId { get; } = connectionId;
    public Random Rng { get; } = random;
    public Timestamp Timestamp { get; private set; } = time;
    public AuthCtx SenderAuth { get; } = AuthCtx.BuildFromSystemTables(connectionId, sender);
    
    private Internal.TxContext? txContext;
    private ProcedureTxContextBase? cachedUserTxContext;

    protected abstract ProcedureTxContextBase CreateTxContext(Internal.TxContext inner);
    protected internal abstract LocalBase CreateLocal();

    private protected ProcedureTxContextBase RequireTxContext()
    {
        var inner =
            txContext
            ?? throw new InvalidOperationException("Transaction context was not initialised.");
        cachedUserTxContext ??= CreateTxContext(inner);
        cachedUserTxContext.Refresh(inner);
        return cachedUserTxContext;
    }

    public Internal.TxContext EnterTxContext(long timestampMicros)
    {
        var timestamp = new Timestamp(timestampMicros);
        Timestamp = timestamp;
        txContext =
            txContext?.WithTimestamp(timestamp)
            ?? new Internal.TxContext(
                CreateLocal(),
                Sender,
                ConnectionId,
                timestamp,
                SenderAuth,
                Rng
            );
        return txContext;
    }

    public void ExitTxContext() => txContext = null;

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

    public readonly struct TxResult<TResult, TError>(bool isSuccess, TResult? value, TError? error)
        where TError : Exception
    {
        public bool IsSuccess { get; } = isSuccess;
        public TResult? Value { get; } = value;
        public TError? Error { get; } = error;

        public static TxResult<TResult, TError> Success(TResult value) => new(true, value, null);

        public static TxResult<TResult, TError> Failure(TError error) => new(false, default, error);

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
        Func<ProcedureTxContextBase, TxResult<TResult, TError>> body
    )
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
        Func<ProcedureTxContextBase, TxResult<TResult, TError>> body
    )
        where TError : Exception
    {
        using var procedure = new SpacetimeDB.Internal.ProcedureContextManager();
        return RunWithRetry(procedure, body);
    }

    private TxResult<TResult, TError> RunWithRetry<TResult, TError>(
        ProcedureContextManager procedureContextManagerContextManager,
        Func<ProcedureTxContextBase, TxResult<TResult, TError>> body
    )
        where TError : Exception
    {
        var result = RunOnce(procedureContextManagerContextManager, body);
        if (!result.IsSuccess)
        {
            return result;
        }

        bool Retry()
        {
            result = RunOnce(procedureContextManagerContextManager, body);
            return result.IsSuccess;
        }

        if (!procedureContextManagerContextManager.CommitMutTxWithRetry(Retry))
        {
            return result;
        }

        return result;
    }

    private TxResult<TResult, TError> RunOnce<TResult, TError>(
        ProcedureContextManager procedureContextManagerContextManager,
        Func<ProcedureTxContextBase, TxResult<TResult, TError>> body
    )
        where TError : Exception
    {
        _ = procedureContextManagerContextManager.StartMutTx();
        using var guard = new AbortGuard(procedureContextManagerContextManager.AbortMutTx);
        var txCtx = RequireTxContext();
        var result = body(txCtx);
        if (result.IsSuccess)
        {
            guard.Disarm();
            return result;
        }

        procedureContextManagerContextManager.AbortMutTx();
        guard.Disarm();
        return result;
    }

    private sealed class AbortGuard(Action abort) : IDisposable
    {
        private readonly Action abort = abort;
        private bool disarmed;

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

public abstract class ProcedureTxContextBase(Internal.TxContext inner)
{
    internal Internal.TxContext Inner { get; private set; } = inner;

    internal void Refresh(Internal.TxContext inner) => Inner = inner;

    public LocalBase Db => (LocalBase)Inner.Db;
    public Identity Sender => Inner.Sender;
    public ConnectionId? ConnectionId => Inner.ConnectionId;
    public Timestamp Timestamp => Inner.Timestamp;
    public AuthCtx SenderAuth => Inner.SenderAuth;
    public Random Rng => Inner.Rng;
}

public abstract class LocalBase : Internal.Local { }

public abstract class LocalReadOnlyBase : Internal.LocalReadOnly { }

public sealed partial class RuntimeProcedureContext(
    Identity sender,
    ConnectionId? connectionId,
    Random random,
    Timestamp timestamp
) : ProcedureContextBase(sender, connectionId, random, timestamp)
{
    private readonly Local _db = new();

    protected internal override LocalBase CreateLocal() => _db;

    protected override ProcedureTxContextBase CreateTxContext(Internal.TxContext inner) =>
        _cached ??= new ProcedureTxContext(inner);

    private ProcedureTxContext? _cached;
}

public sealed class ProcedureTxContext : ProcedureTxContextBase
{
    internal ProcedureTxContext(Internal.TxContext inner)
        : base(inner) { }

    public new Local Db => (Local)base.Db;
}

public sealed class Local : LocalBase { }

public sealed class LocalReadOnly : LocalReadOnlyBase { }
#pragma warning restore STDB_UNSTABLE
