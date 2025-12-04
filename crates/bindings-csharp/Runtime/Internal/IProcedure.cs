namespace SpacetimeDB.Internal;

using System.Text;
using SpacetimeDB.BSATN;

public interface IProcedureContext
{
    public static Identity GetIdentity()
    {
        FFI.identity(out var identity);
        return identity;
    }
}

public interface IProcedure
{
    RawProcedureDefV9 MakeProcedureDef(ITypeRegistrar registrar);

    // Return the serialized payload that should be sent to the host.
    byte[] Invoke(BinaryReader reader, IProcedureContext ctx);

    public static void VolatileNonatomicScheduleImmediate(string name, MemoryStream args)
    {
        var name_bytes = Encoding.UTF8.GetBytes(name);
        var args_bytes = args.ToArray();

        FFI.volatile_nonatomic_schedule_immediate(
            name_bytes,
            (uint)name_bytes.Length,
            args_bytes,
            (uint)args_bytes.Length
        );
    }
}

public interface IInternalProcedureContext : IProcedureContext
{
    bool TryTakeTransactionOffset(out TransactionOffset offset);
    void SetTransactionOffset(TransactionOffset offset);
    TxContext EnterTxContext(long timestampMicros);
    void ExitTxContext();
}
