using SpacetimeDB;

namespace HandwrittenModuleBindings;

[SpacetimeDB.Type]
public partial class Address
{
    public string City;
}

[Table(Accessor = "Person", Public = true)]
public partial class Person
{
    [PrimaryKey] public int Id;
    public string Name;
    public Address Address;
    public List<string> Tags;
}

public static partial class Module
{
    [Reducer]
    public static partial void Add(ReducerContext ctx, string name);

    [Procedure]
    public static string Greet(ProcedureContext ctx, string name) => "Hello, " + name;
}
