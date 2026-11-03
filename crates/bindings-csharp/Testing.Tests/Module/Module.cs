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

[Table(Accessor = "Events", Event = true)]
public partial struct TestEvent
{
    public string Text;
}

[Table(Accessor = "Accounts")]
public partial class Account
{
    [PrimaryKey]
    public ulong Id;

    [AutoInc]
    public ulong Serial;

    [Unique]
    public string Name = "";
}

[Table(Accessor = "Points")]
[SpacetimeDB.Index.BTree(
    Accessor = "ByGroupScore",
    Columns = [nameof(Group), nameof(Score), nameof(Id)]
)]
public partial struct Point
{
    [PrimaryKey]
    public uint Id;
    public uint Group;

    [SpacetimeDB.Index.BTree]
    public uint Score;
}

[Env]
public struct EnvironmentSchema
{
    public string REQUIRED;
    public string? OPTIONAL;

    [EnvValues("fixed")]
    public string? LITERAL;

    [EnvValues("development", "production")]
    public string? MODE;
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
        var definition = root.BuildModuleDefinition();
        // This fixture only mounts tables; environment and lifecycle declarations are root-only.
        var mounted = definition
            .Sections.OfType<RawModuleDefV10Section.Submodules>()
            .Single()
            .Submodules_.Single()
            .Module;
        mounted.Sections.RemoveAll(section =>
            section is RawModuleDefV10Section.Environment or RawModuleDefV10Section.LifeCycleReducers
        );
        RawModuleDef schema = new RawModuleDef.V10(definition);
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
