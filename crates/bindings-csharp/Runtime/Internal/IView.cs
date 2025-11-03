namespace SpacetimeDB.Internal;

using System.Text;
using SpacetimeDB.BSATN;

public interface IView
{
    RawViewDefV9 MakeViewDef(ITypeRegistrar registrar);

    // This one is not static because we need to be able to store IView in a list.
    void Invoke(BinaryReader reader, IViewContext args);
}

public interface IAnonymousView
{
    RawViewDefV9 MakeAnonymousViewDef(ITypeRegistrar registrar);

    // This one is not static because we need to be able to store IAnonymousView in a list.
    void Invoke(BinaryReader reader, IAnonymousViewContext args);
}

public interface IViewContext
{
    public static Identity GetIdentity()
    {
        FFI.identity(out var identity);
        return identity;
    }
}

public interface IAnonymousViewContext
{
    public static Identity GetIdentity()
    {
        FFI.identity(out var identity);
        return identity;
    }
}