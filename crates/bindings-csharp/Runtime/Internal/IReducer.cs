namespace SpacetimeDB.Internal;

using System;
using SpacetimeDB.BSATN;
#if !NET10_0_OR_GREATER || SPACETIMEDB_WASM_RUNTIME
using System.Text;
#endif

public interface IReducerContext
{
    public static Identity GetDatabaseIdentity()
    {
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        throw new NotSupportedException(
            "Static database identity lookup is unavailable in managed module tests. Use the fixture or reducer context's DatabaseIdentity."
        );
#else
        FFI.identity(out var identity);
        return identity;
#endif
    }

    [Obsolete("IReducerContext.GetIdentity() is deprecated. Use GetDatabaseIdentity() instead.")]
    public static Identity GetIdentity() => GetDatabaseIdentity();
}

public interface IReducer
{
    RawReducerDefV10 MakeReducerDef(ITypeRegistrar registrar);

    Lifecycle? Lifecycle { get; }

    public static void VolatileNonatomicScheduleImmediate(string name, MemoryStream args)
    {
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        throw new NotSupportedException(
            "VolatileNonatomicScheduleImmediate is unavailable in managed module tests. Call the function directly."
        );
#else
        var name_bytes = Encoding.UTF8.GetBytes(name);
        var args_bytes = args.ToArray();

        FFI.volatile_nonatomic_schedule_immediate(
            name_bytes,
            name_bytes.Length,
            args_bytes,
            args_bytes.Length
        );
#endif
    }
}
