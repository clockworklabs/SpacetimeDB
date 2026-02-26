using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "User")]
    public partial struct User
    {
        [PrimaryKey] public int Id;
        public string Name;
        public int Age;
        public bool Active;
    }

    [Table(Accessor = "Product")]
    public partial struct Product
    {
        [PrimaryKey] public int Id;
        public string Title;
        public float Price;
        public bool InStock;
    }

    [Table(Accessor = "Note")]
    public partial struct Note
    {
        [PrimaryKey] public int Id;
        public string Body;
        public long Rating;
        public bool Pinned;
    }
}
