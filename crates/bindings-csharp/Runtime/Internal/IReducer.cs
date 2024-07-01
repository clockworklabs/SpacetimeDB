namespace SpacetimeDB.Internal;

using System.Text;
using SpacetimeDB.BSATN;

public interface IReducer
{
    Module.ReducerDef MakeReducerDef(ITypeRegistrar registrar);

    // This one is not static because we need to be able to store IReducer in a list.
    void Invoke(BinaryReader reader, ReducerContext args);

    public static ScheduleToken Schedule(string name, MemoryStream args, DateTimeOffset time)
    {
        var name_bytes = Encoding.UTF8.GetBytes(name);
        var args_bytes = args.ToArray();

        FFI._schedule_reducer(
            name_bytes,
            (uint)name_bytes.Length,
            args_bytes,
            (uint)args_bytes.Length,
            (ulong)((time - DateTimeOffset.UnixEpoch).Ticks / 10),
            out var handle
        );

        return new(handle);
    }
}
