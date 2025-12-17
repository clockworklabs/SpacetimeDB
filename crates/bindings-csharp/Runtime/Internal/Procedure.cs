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
