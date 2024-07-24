namespace SpacetimeDB.Internal;

using SpacetimeDB.BSATN;

public interface IReducer
{
    Module.ReducerDef MakeReducerDef(ITypeRegistrar registrar);

    // This one is not static because we need to be able to store IReducer in a list.
    void Invoke(BinaryReader reader, ReducerContext args);
}
