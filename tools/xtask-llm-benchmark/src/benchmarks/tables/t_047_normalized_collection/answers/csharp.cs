using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "CollectionOwner", Public = true)]
    public partial struct CollectionOwner
    {
        [PrimaryKey, AutoInc] public ulong Id;
        public string Name;
    }

    [Table(Accessor = "ChildItem", Public = true)]
    [SpacetimeDB.Index.BTree(Accessor = "by_owner", Columns = new[] { nameof(OwnerId) })]
    public partial struct ChildItem
    {
        [PrimaryKey, AutoInc] public ulong Id;
        public ulong OwnerId;
        public string Value;
    }
}
