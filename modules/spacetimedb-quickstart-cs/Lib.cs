using SpacetimeDB.Module;
using static SpacetimeDB.Runtime;

static partial class Module
{
    [SpacetimeDB.Table]
    public partial struct Person
    {
        public string Name;
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
