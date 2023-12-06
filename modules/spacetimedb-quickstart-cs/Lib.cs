using SpacetimeDB.Module;
using static SpacetimeDB.Runtime;

static partial class Module
{
    [SpacetimeDB.Table]
    public partial struct Person
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKeyAuto)]
        public uint Id;
        public string Name;
        public byte Age;
    }

    // Verify that all types compile via codegen successfully.
    // TODO: port actual SDK tests from Rust.
    [SpacetimeDB.Table]
    public partial struct Typecheck
    {
        bool BoolField;
        byte ByteField;
        sbyte SbyteField;
        short ShortField;
        ushort UshortField;
        int IntField;
        uint UintField;
        long LongField;
        ulong UlongField;
        float FloatField;
        double DoubleField;
        string StringField;
        Int128 Int128Field;
        UInt128 Uint128Field;
        Person NestedTableField;
        Person[] NestedTableArrayField;
        List<Person> NestedTableListField;
        Dictionary<string, Person> NestedTableDictionaryField;
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
            Log($"Hello, {person.Name}!");
        }
        Log("Hello, World!");
    }

    [SpacetimeDB.Reducer("list_over_age")]
    public static void ListOverAge(byte age)
    {
        foreach (var person in Person.Query(person => person.Age >= age))
        {
            Log($"{person.Name} has age {person.Age} >= {age}");
        }
    }
}
