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
    public static NamespaceRegistry CreateNamespaces() => AssemblyDescriptor.CreateNamespaces();

    public static (byte[] Schema, NamespaceRegistry Namespaces) BuildMounted(string mount)
    {
        var child = new ModuleBuilder();
        global::ModuleRegistration.RegisterModule(child);
        var root = new ModuleBuilder();
        root.RegisterSubmodule(mount, null, child);
        RawModuleDef schema = new RawModuleDef.V10(root.BuildModuleDefinition());
        return (
            IStructuralReadWrite.ToBytes(new RawModuleDef.BSATN(), schema),
            new NamespaceRegistry(
                "test-root",
                CaseConversionPolicy.SnakeCase,
                [(typeof(Entry).Assembly.FullName!, mount, null, CaseConversionPolicy.SnakeCase)]
            )
        );
    }

    public static byte[] Build()
    {
        var builder = new ModuleBuilder();
        global::ModuleRegistration.RegisterModule(builder);
        RawModuleDef schema = new RawModuleDef.V10(builder.BuildModuleDefinition());
        return IStructuralReadWrite.ToBytes(new RawModuleDef.BSATN(), schema);
    }
}
