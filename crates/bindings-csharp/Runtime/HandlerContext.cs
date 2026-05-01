namespace SpacetimeDB;

using System;
using System.Diagnostics.CodeAnalysis;

#pragma warning disable STDB_UNSTABLE
public abstract class HandlerContextBase(Random random, Timestamp time)
{
    public Random Rng { get; } = random;
    public Timestamp Timestamp { get; private set; } = time;

    // NOTE: The host rejects procedure HTTP requests while a mut transaction is open
    // (WOULD_BLOCK_TRANSACTION). Avoid calling `Http.*` inside WithTx.
    public HttpClient Http { get; } = new();

    // **Note:** must be 0..=u32::MAX
    protected int CounterUuid = 0;

    private Internal.TxContext? txContext;
    private HandlerTxContextBase? cachedUserTxContext;

    protected abstract HandlerTxContextBase CreateTxContext(Internal.TxContext inner);
    protected internal abstract LocalBase CreateLocal();

    private protected HandlerTxContextBase RequireTxContext()
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
                default,
                null,
                timestamp,
                AuthCtx.BuildFromSystemTables(null, default),
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
    }

    [Experimental("STDB_UNSTABLE")]
    public TResult WithTx<TResult>(Func<HandlerTxContextBase, TResult> body) =>
        TryWithTx(tx => Result<TResult, Exception>.Ok(body(tx))).UnwrapOrThrow();

    [Experimental("STDB_UNSTABLE")]
    public TxOutcome<TResult> TryWithTx<TResult, TError>(
        Func<HandlerTxContextBase, Result<TResult, TError>> body
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
        Func<HandlerTxContextBase, Result<TResult, TError>> body
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
        Func<HandlerTxContextBase, Result<TResult, TError>> body
    )
        where TError : Exception
    {
        var micros = StartMutTx();
        using var guard = new AbortGuard(AbortMutTx);
        EnterTxContext(micros);
        var txCtx = RequireTxContext();

        Result<TResult, TError> result = body(txCtx);

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

public abstract class HandlerTxContextBase(Internal.TxContext inner)
{
    internal Internal.TxContext Inner { get; private set; } = inner;

    internal void Refresh(Internal.TxContext inner) => Inner = inner;

    public LocalBase Db => (LocalBase)Inner.Db;
    public Timestamp Timestamp => Inner.Timestamp;
    public Random Rng => Inner.Rng;
}

internal sealed partial class RuntimeHandlerContext(Random random, Timestamp timestamp)
    : HandlerContextBase(random, timestamp)
{
    private readonly RuntimeLocal _db = new();

    protected internal override LocalBase CreateLocal() => _db;

    protected override HandlerTxContextBase CreateTxContext(Internal.TxContext inner) =>
        _cached ??= new RuntimeHandlerTxContext(inner);

    private RuntimeHandlerTxContext? _cached;
}

internal sealed class RuntimeHandlerTxContext : HandlerTxContextBase
{
    internal RuntimeHandlerTxContext(Internal.TxContext inner)
        : base(inner) { }

    public new RuntimeLocal Db => (RuntimeLocal)base.Db;
}
#pragma warning restore STDB_UNSTABLE
