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
        FFI.identity(out var identity);
        return identity;
    }
}

public interface IAnonymousViewContext { }
