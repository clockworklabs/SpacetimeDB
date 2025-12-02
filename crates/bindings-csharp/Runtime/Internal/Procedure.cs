using System.Threading;

namespace SpacetimeDB.Internal;

public static class Procedure
{
    private static readonly AsyncLocal<IInternalProcedureContext?> current = new();

    private readonly struct ContextScope : IDisposable
    {
        private readonly IInternalProcedureContext? previous;

        public ContextScope(IInternalProcedureContext? next)
        {
            previous = current.Value;
            current.Value = next;
        }

        public void Dispose() => current.Value = previous;
    }

    internal static IDisposable PushContext(IProcedureContext ctx) =>
        new ContextScope(ctx as IInternalProcedureContext);

    private static IInternalProcedureContext RequireContext() =>
        current.Value ?? throw new InvalidOperationException(
            "Transaction syscalls require a procedure context."
        );

    public static long StartMutTx()
    {
        var status = FFI.procedure_start_mut_tx(out var micros);
        FFI.ErrnoHelpers.ThrowIfError(status);
        return micros;
    }

    public static void CommitMutTx()
    {
        FFI.procedure_commit_mut_tx(); // throws on error
        if (RequireContext() is IInternalProcedureContext ctx &&
            TryTakeOffsetFromHost(out var offset))
        {
            ctx.SetTransactionOffset(offset);
            Module.RecordProcedureTxOffset(offset);
        }
    }
    
    public static void AbortMutTx()
    {
        FFI.procedure_abort_mut_tx(); // throws on error
        if (RequireContext() is IInternalProcedureContext ctx &&
            TryTakeOffsetFromHost(out var offset))
        {
            ctx.SetTransactionOffset(offset);
            Module.RecordProcedureTxOffset(offset);
        }
    }

    private static bool TryTakeOffsetFromHost(out TransactionOffset offset)
    {
        if (FFI.take_procedure_tx_offset(out var rawOffset))
        {
            offset = TransactionOffset.FromRaw(unchecked((long)rawOffset));
            return true;
        }

        offset = default;
        return false;
    }

    public static bool CommitMutTxWithRetry(Func<bool> retryBody)
    {
        try
        {
            CommitMutTx();
            return true;
        }
        catch (TransactionNotAnonymousException)
        {
            // reducer misuse, abort immediately
            return false;
        }
        catch (StdbException)
        {
            if (retryBody()) {
                CommitMutTx();
                return true;
            }
            return false;
        }
    }
    
    public static async Task<bool> CommitMutTxWithRetryAsync(Func<Task<bool>> retryBody)
    {
        try
        {
            await CommitMutTxAsync().ConfigureAwait(false);
            return true;
        }
        catch (TransactionNotAnonymousException)
        {
            return false;
        }
        catch (StdbException)
        {
            if (await retryBody().ConfigureAwait(false))
            {
                await CommitMutTxAsync().ConfigureAwait(false);
                return true;
            }
            return false;
        }
    }

    public static Task CommitMutTxAsync()
    {
        CommitMutTx(); // existing sync path
        return Task.CompletedTask;
    }

    public static Task AbortMutTxAsync()
    {
        AbortMutTx(); // existing sync path
        return Task.CompletedTask;
    }
}

public readonly struct TransactionOffset
{
    public ulong Value { get; }

    private TransactionOffset(ulong value) => Value = value;

    public static TransactionOffset FromRaw(long raw) =>
        new(unchecked((ulong)raw));
}
