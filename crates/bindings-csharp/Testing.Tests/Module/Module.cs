namespace TestingModule;

using SpacetimeDB;
using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;

[Table(Accessor = "Entries")]
public partial struct Entry
{
    [PrimaryKey, AutoInc]
    public ulong Id;
    public string Text;
}

public static class Schema
{
    public static byte[] Build()
    {
        var builder = new ModuleBuilder();
        global::ModuleRegistration.RegisterModule(builder);
        RawModuleDef schema = new RawModuleDef.V10(builder.BuildModuleDefinition());
        return IStructuralReadWrite.ToBytes(new RawModuleDef.BSATN(), schema);
    }
}
