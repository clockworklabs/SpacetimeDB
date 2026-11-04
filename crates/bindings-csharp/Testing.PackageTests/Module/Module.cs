using SpacetimeDB;

namespace PackageModule;

[Table(Accessor = "Entries")]
public partial struct Entry
{
    [PrimaryKey]
    public ulong Id;
    public string Text;
}

public static partial class Module
{
    [Reducer]
    public static void Add(ReducerContext ctx, ulong id, string text)
    {
        ctx.Db.Entries.Insert(new Entry { Id = id, Text = text });
        if (text.Length == 0)
            throw new ArgumentException("Text must not be empty");
    }
}
