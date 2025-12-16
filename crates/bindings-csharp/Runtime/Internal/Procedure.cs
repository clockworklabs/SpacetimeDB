namespace SpacetimeDB.Internal;

using System;
using System.IO;
using System.Text;
using SpacetimeDB.BSATN;

/// <summary>
/// Represents a procedure that can be registered and invoked by the module runtime.
/// </summary>
public interface IProcedure
{
    /// <summary>
    /// Creates a procedure definition for registration with the module system.
    /// </summary>
    RawProcedureDefV9 MakeProcedureDef(ITypeRegistrar registrar);

    /// <summary>
    /// Invokes the procedure with the given arguments and context.
    /// </summary>
    byte[] Invoke(BinaryReader reader, IProcedureContext ctx);
}

/// <summary>
/// Manages procedure execution context and transactions for SpacetimeDB procedures.
/// This class is responsible for maintaining the current procedure context and
/// managing transaction state.
/// </summary>
public class ProcedureContextManager : IProcedureContextManager
{
    private static IInternalProcedureContext? _current;

    private readonly struct ContextScope : IDisposable
    {
        private readonly IInternalProcedureContext? _previous;

        public ContextScope(IInternalProcedureContext? previous)
        {
            _previous = previous;
        }

        public void Dispose()
        {
            _current = _previous;
        }
    }

    /// <summary>
    /// Pushes a new procedure context onto the context stack.
    /// </summary>
    /// <param name="ctx">The procedure context to push.</param>
    /// <returns>An <see cref="IDisposable"/> that will restore the previous context when disposed.</returns>
    public IDisposable PushContext(IProcedureContext ctx)
    {
        var previous = _current;
        _current = ctx as IInternalProcedureContext;
        return new ContextScope(previous);
    }

    private static IInternalProcedureContext RequireContext() =>
        _current
        ?? throw new InvalidOperationException("Transaction syscalls require a procedure context.");

    public long StartMutTx()
    {
        var status = FFI.procedure_start_mut_tx(out var micros);
        FFI.ErrnoHelpers.ThrowIfError(status);
        var ctx = RequireContext();
        ctx.EnterTxContext(micros);
        return micros;
    }

    public void CommitMutTx()
    {
        var status = FFI.procedure_commit_mut_tx();
        FFI.ErrnoHelpers.ThrowIfError(status);
        var ctx = RequireContext();
        ctx.ExitTxContext();
    }

    public void AbortMutTx()
    {
        var status = FFI.procedure_abort_mut_tx();
        FFI.ErrnoHelpers.ThrowIfError(status);
        var ctx = RequireContext();
        ctx.ExitTxContext();
    }

    public bool CommitMutTxWithRetry(Func<bool> retryBody)
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


    public void Dispose()
    {
        GC.SuppressFinalize(this);
    }
}

/// <summary>
/// Represents the context for a procedure call.
/// </summary>
public interface IProcedureContext
{
    /// <summary>
    /// Gets the identity of the current procedure caller.
    /// </summary>
    /// <returns>The identity of the caller.</returns>
    public static Identity GetIdentity()
    {
        FFI.identity(out var identity);
        return identity;
    }
}

/// <summary>
/// Manages procedure execution context and transactions.
/// </summary>
public interface IProcedureContextManager : IDisposable
{
    /// <summary>Pushes a new procedure context onto the context stack.</summary>
    IDisposable PushContext(IProcedureContext ctx);

    /// <summary>Starts a new mutable transaction.</summary>
    long StartMutTx();

    /// <summary>Commits the current mutable transaction.</summary>
    void CommitMutTx();

    /// <summary>Aborts the current mutable transaction.</summary>
    void AbortMutTx();

    /// <summary>Commits a transaction with a retry mechanism.</summary>
    bool CommitMutTxWithRetry(Func<bool> retryBody);

}

/// <summary>
/// Internal interface for procedure context with additional functionality.
/// </summary>
public interface IInternalProcedureContext : IProcedureContext
{
    TxContext EnterTxContext(long timestampMicros);
    void ExitTxContext();
}

/// <summary>
/// Provides utility methods for procedure-related functionality.
/// </summary>
public static class ProcedureExtensions
{
    /// <summary>
    /// Schedules an immediate volatile, non-atomic procedure call.
    /// </summary>
    public static void VolatileNonatomicScheduleImmediate(string name, MemoryStream args)
    {
        var name_bytes = Encoding.UTF8.GetBytes(name);
        var args_bytes = args.ToArray();

        try
        {
            FFI.volatile_nonatomic_schedule_immediate(
                name_bytes,
                (uint)name_bytes.Length,
                args_bytes,
                (uint)args_bytes.Length
            );
        }
        catch (Exception ex)
        {
            Log.Error($"Failed to schedule procedure {name}: {ex}");
            throw;
        }
    }
}
