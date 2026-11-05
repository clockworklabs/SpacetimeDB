namespace SpacetimeDB.Internal;

using SpacetimeDB.BSATN;

public interface IView
{
    RawViewDefV10 MakeViewDef(ITypeRegistrar registrar);
}

public interface IAnonymousView
{
    RawViewDefV10 MakeAnonymousViewDef(ITypeRegistrar registrar);
}

public interface IViewContext
{
    public static Identity GetIdentity()
    {
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        throw new NotSupportedException(
            "Static view identity lookup is unavailable in managed module tests. Use the fixture's DatabaseIdentity or the context's Sender."
        );
#else
        FFI.identity(out var identity);
        return identity;
#endif
    }
}

public interface IAnonymousViewContext { }
