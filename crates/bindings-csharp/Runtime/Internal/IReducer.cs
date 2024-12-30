namespace SpacetimeDB.Internal;

using System.Text;
using SpacetimeDB.BSATN;

public interface IReducerContext { }

public interface IReducer
{
    RawReducerDefV9 MakeReducerDef(ITypeRegistrar registrar);

    // This one is not static because we need to be able to store IReducer in a list.
    void Invoke(BinaryReader reader, IReducerContext args);

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
