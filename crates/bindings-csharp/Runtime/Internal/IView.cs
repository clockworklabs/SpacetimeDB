namespace SpacetimeDB.Internal;

using SpacetimeDB.BSATN;

public interface IView
{
    RawViewDefV9 MakeViewDef(ITypeRegistrar registrar);

    // This one is not static because we need to be able to store IView in a list.
    byte[] Invoke(BinaryReader reader, IViewContext args);
}

public interface IAnonymousView
{
    RawViewDefV9 MakeAnonymousViewDef(ITypeRegistrar registrar);

    // This one is not static because we need to be able to store IAnonymousView in a list.
    byte[] Invoke(BinaryReader reader, IAnonymousViewContext args);
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
