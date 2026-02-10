namespace SpacetimeDB;

using System.Diagnostics.CodeAnalysis;

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

    // NOTE: The host rejects procedure HTTP requests while a mut transaction is open
    // (WOULD_BLOCK_TRANSACTION). Avoid calling `Http.*` inside WithTx.
    public HttpClient Http { get; } = new();

    // **Note:** must be 0..=u32::MAX
    protected int CounterUuid = 0;
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

    [Experimental("STDB_UNSTABLE")]
    public TResult WithTx<TResult>(Func<ProcedureTxContextBase, TResult> body) =>
        TryWithTx(tx => Result<TResult, Exception>.Ok(body(tx))).UnwrapOrThrow();

    [Experimental("STDB_UNSTABLE")]
    public TxOutcome<TResult> TryWithTx<TResult, TError>(
        Func<ProcedureTxContextBase, Result<TResult, TError>> body
    )
        where TError : Exception
    {
        try
        {
            var result = RunWithRetry(body);

            return result switch
            {
                Result<TResult, TError>.OkR(var value) => TxOutcome<TResult>.Success(value),
                Result<TResult, TError>.ErrR(var error) => TxOutcome<TResult>.Failure(error),
                _ => throw new InvalidOperationException("Unknown Result variant."),
            };
        }
        catch (Exception ex)
        {
            return TxOutcome<TResult>.Failure(ex);
        }
    }

    // Private transaction management methods (Rust-like encapsulation)
    private long StartMutTx()
    {
        var status = Internal.FFI.procedure_start_mut_tx(out var micros);
        Internal.FFI.ErrnoHelpers.ThrowIfError(status);
        return micros;
    }

    private void CommitMutTx()
    {
        var status = Internal.FFI.procedure_commit_mut_tx();
        Internal.FFI.ErrnoHelpers.ThrowIfError(status);
    }

    private void AbortMutTx()
    {
        var status = Internal.FFI.procedure_abort_mut_tx();
        Internal.FFI.ErrnoHelpers.ThrowIfError(status);
    }

    private bool CommitMutTxWithRetry(Func<bool> retryBody)
    {
        try
        {
            CommitMutTx();
            return true;
        }
        catch (TransactionNotAnonymousException)
        {
            return false;
        }
        catch (StdbException)
        {
            Log.Warn("Committing anonymous transaction failed; retrying once.");
            if (retryBody())
            {
                CommitMutTx();
                return true;
            }
            return false;
        }
    }

    private Result<TResult, TError> RunWithRetry<TResult, TError>(
        Func<ProcedureTxContextBase, Result<TResult, TError>> body
    )
        where TError : Exception
    {
        var result = RunOnce(body);
        if (result is Result<TResult, TError>.ErrR)
        {
            return result;
        }

        bool Retry()
        {
            result = RunOnce(body);
            return result is Result<TResult, TError>.OkR;
        }

        if (!CommitMutTxWithRetry(Retry))
        {
            return result;
        }

        return result;
    }

    private Result<TResult, TError> RunOnce<TResult, TError>(
        Func<ProcedureTxContextBase, Result<TResult, TError>> body
    )
        where TError : Exception
    {
        var micros = StartMutTx();
        using var guard = new AbortGuard(AbortMutTx);
        EnterTxContext(micros);
        var txCtx = RequireTxContext();

        Result<TResult, TError> result;
        try
        {
            result = body(txCtx);
        }
        catch (Exception)
        {
            throw;
        }

        if (result is Result<TResult, TError>.OkR)
        {
            guard.Disarm();
            return result;
        }

        AbortMutTx();
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
