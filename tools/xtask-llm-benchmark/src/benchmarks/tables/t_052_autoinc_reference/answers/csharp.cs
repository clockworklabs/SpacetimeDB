using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Parent", Public = true)]
    public partial struct Parent
    {
        [PrimaryKey, AutoInc] public ulong Id;
        public string Name;
    }

    [Table(Accessor = "Child", Public = true)]
    [SpacetimeDB.Index.BTree(Accessor = "by_parent", Columns = new[] { nameof(ParentId) })]
    public partial struct Child
    {
        [PrimaryKey, AutoInc] public ulong Id;
        public ulong ParentId;
        public string Name;
    }

    [Reducer]
    public static void CreateFamily(ReducerContext ctx, string parentName, List<string> childNames)
    {
        var parent = ctx.Db.Parent.Insert(new Parent { Id = 0, Name = parentName });
        foreach (var name in childNames)
        {
            ctx.Db.Child.Insert(new Child { Id = 0, ParentId = parent.Id, Name = name });
        }
    }
}
