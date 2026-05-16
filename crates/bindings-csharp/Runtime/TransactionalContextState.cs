namespace SpacetimeDB;

using System;
using SpacetimeDB.Internal;

#pragma warning disable STDB_UNSTABLE
internal interface IRefreshableTxContext
{
    void Refresh(Internal.TxContext inner);
}

internal readonly struct TxOutcomeCore<TResult>(bool isSuccess, TResult? value, Exception? error)
{
    public bool IsSuccess { get; } = isSuccess;
    public TResult? Value { get; } = value;
    public Exception? Error { get; } = error;

    public static TxOutcomeCore<TResult> Success(TResult value) => new(true, value, null);

    public static TxOutcomeCore<TResult> Failure(Exception error) => new(false, default, error);
}

internal sealed class TransactionalContextState<TTxContext>(
    Random random,
    Timestamp time,
    Func<Timestamp, Internal.TxContext> createInitialTxContext,
    Func<Internal.TxContext, TTxContext> createTxContext
)
    where TTxContext : class, IRefreshableTxContext
{
    public Random Rng { get; } = random;
    public Timestamp Timestamp { get; private set; } = time;

    private Internal.TxContext? txContext;
    private TTxContext? cachedUserTxContext;

    public Internal.TxContext EnterTxContext(long timestampMicros)
    {
        var timestamp = new Timestamp(timestampMicros);
        Timestamp = timestamp;
        txContext = txContext?.WithTimestamp(timestamp) ?? createInitialTxContext(timestamp);
        return txContext;
    }

    public void ExitTxContext() => txContext = null;

    public TTxContext RequireTxContext()
    {
        var inner =
            txContext
            ?? throw new InvalidOperationException("Transaction context was not initialised.");
        cachedUserTxContext ??= createTxContext(inner);
        cachedUserTxContext.Refresh(inner);
        return cachedUserTxContext;
    }

    public TResult WithTx<TResult>(Func<TTxContext, TResult> body) =>
        TryWithTx(tx => Result<TResult, Exception>.Ok(body(tx))).UnwrapOrThrow();

    public TxOutcomeCore<TResult> TryWithTx<TResult, TError>(
        Func<TTxContext, Result<TResult, TError>> body
    )
        where TError : Exception
    {
        try
        {
            var result = RunWithRetry(body);

            return result switch
            {
                Result<TResult, TError>.OkR(var value) => TxOutcomeCore<TResult>.Success(value),
                Result<TResult, TError>.ErrR(var error) => TxOutcomeCore<TResult>.Failure(error),
                _ => throw new InvalidOperationException("Unknown Result variant."),
            };
        }
        catch (Exception ex)
        {
            return TxOutcomeCore<TResult>.Failure(ex);
        }
    }

    private long StartMutTx()
    {
        var status = FFI.procedure_start_mut_tx(out var micros);
        FFI.ErrnoHelpers.ThrowIfError(status);
        return micros;
    }

    private void CommitMutTx()
    {
        var status = FFI.procedure_commit_mut_tx();
        FFI.ErrnoHelpers.ThrowIfError(status);
    }

    private void AbortMutTx()
    {
        var status = FFI.procedure_abort_mut_tx();
        FFI.ErrnoHelpers.ThrowIfError(status);
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
        Func<TTxContext, Result<TResult, TError>> body
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
        Func<TTxContext, Result<TResult, TError>> body
    )
        where TError : Exception
    {
        var micros = StartMutTx();
        using var guard = new AbortGuard(AbortMutTx);
        EnterTxContext(micros);
        var txCtx = RequireTxContext();

        var result = body(txCtx);

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

internal static class TxOutcomeCoreExtensions
{
    public static TResult UnwrapOrThrow<TResult>(this TxOutcomeCore<TResult> outcome) =>
        outcome.IsSuccess
            ? outcome.Value!
            : throw (
                outcome.Error
                ?? new InvalidOperationException("Transaction failed without an error object.")
            );
}
#pragma warning restore STDB_UNSTABLE
