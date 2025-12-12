namespace SpacetimeDB.Internal;

using System;
using System.IO;
using System.Runtime.InteropServices;
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
    private readonly AsyncLocal<IInternalProcedureContext?> current = new();

    private readonly struct ContextScope : IDisposable
    {
        private readonly IInternalProcedureContext? previous;
        private readonly ProcedureContextManager? _procedure;

        public ContextScope(ProcedureContextManager procedure, IInternalProcedureContext? next)
        {
            _procedure = procedure;
            previous = procedure.current.Value;
            procedure.current.Value = next;
        }

        public void Dispose()
        {
            if (_procedure != null && _procedure.current.Value != null)
            {
                _procedure.current.Value = previous;
            }
        }
    }

    /// <summary>
    /// Pushes a new procedure context onto the context stack.
    /// </summary>
    /// <param name="ctx">The procedure context to push.</param>
    /// <returns>An <see cref="IDisposable"/> that will restore the previous context when disposed.</returns>

    public IDisposable PushContext(IProcedureContext ctx)
    {
        return new ContextScope(this, ctx as IInternalProcedureContext);
    }

    private IInternalProcedureContext RequireContext() =>
        current.Value ?? throw new InvalidOperationException("Transaction syscalls require a procedure context.");

    public long StartMutTx()
    {
        var status = FFI.procedure_start_mut_tx(GCHandle.ToIntPtr(GCHandle.Alloc(this)), out var micros);
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

    public async Task<bool> CommitMutTxWithRetryAsync(Func<Task<bool>> retryBody)
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
            Log.Warn("Committing anonymous transaction failed; retrying once.");
            if (await retryBody().ConfigureAwait(false))
            {
                await CommitMutTxAsync().ConfigureAwait(false);
                return true;
            }
            return false;
        }
    }

    public Task CommitMutTxAsync()
    {
        CommitMutTx();
        return Task.CompletedTask;
    }

    public Task AbortMutTxAsync()
    {
        AbortMutTx();
        return Task.CompletedTask;
    }

    public void Dispose()
    {
        Log.Error("[DEBUG] ProcedureContextManager.Dispose() called");
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
    
    /// <summary>Asynchronously commits a transaction with a retry mechanism.</summary>
    Task<bool> CommitMutTxWithRetryAsync(Func<Task<bool>> retryBody);
    
    /// <summary>Asynchronously commits the current mutable transaction.</summary>
    Task CommitMutTxAsync();
    
    /// <summary>Asynchronously aborts the current mutable transaction.</summary>
    Task AbortMutTxAsync();
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
    public static void VolatileNonatomicScheduleImmediate(
        string name,
        MemoryStream args)
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