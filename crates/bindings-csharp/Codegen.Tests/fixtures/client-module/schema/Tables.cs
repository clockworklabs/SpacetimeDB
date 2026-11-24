namespace Shared
{
    [SpacetimeDB.Table(Accessor = "Person", Name = "people", Public = true)]
    [SpacetimeDB.Index.BTree(Accessor = "Pair", Columns = new[] { "Id", "Name" })]
    public partial class Person
    {
        [SpacetimeDB.PrimaryKey]
        public int Id;
        public string Name;
    }
}
