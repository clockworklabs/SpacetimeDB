using SpacetimeDB;

static partial class Module
{
    [SpacetimeDB.Table(Public = true)]
    public partial struct Person
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKeyAuto)]
        public uint Id;
        public string Name;
        public byte Age;
    }

    [SpacetimeDB.Reducer("add")]
    public static void Add(string name, byte age)
    {
        new Person { Name = name, Age = age }.Insert();
    }

    [SpacetimeDB.Reducer("say_hello")]
    public static void SayHello()
    {
        foreach (var person in Person.Iter())
        {
            Log.Info($"Hello, {person.Name}!");
        }
        Log.Info("Hello, World!");
    }

    [SpacetimeDB.Reducer("list_over_age")]
    public static void ListOverAge(byte age)
    {
        foreach (var person in Person.Query(person => person.Age >= age))
        {
            Log.Info($"{person.Name} has age {person.Age} >= {age}");
        }
    }
}
