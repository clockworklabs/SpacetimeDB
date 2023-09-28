using SpacetimeDB.Module;
using static SpacetimeDB.Runtime;

static partial class Module
{
    [SpacetimeDB.Table]
    public partial struct Person
    {
        public string Name;
    }

    [SpacetimeDB.Reducer("add")]
    public static void Add(string name)
    {
        new Person { Name = name }.Insert();
    }

    [SpacetimeDB.Reducer("say_hello")]
    public static void SayHello()
    {
        foreach (var person in Person.Iter())
        {
            Log($"Hello, {person.Name}!");
        }
        Log("Hello, World!");
    }
}
