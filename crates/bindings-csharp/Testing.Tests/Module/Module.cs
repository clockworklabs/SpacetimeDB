namespace TestingModule;

using SpacetimeDB;
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

    /// <summary>Registers this module's declarations. Tests build schemas from it.</summary>
    public static void Register(ModuleBuilder builder) =>
        global::ModuleRegistration.RegisterModule(builder);
}
