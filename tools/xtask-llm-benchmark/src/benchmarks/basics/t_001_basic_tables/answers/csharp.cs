using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "users")]
    public partial struct Users
    {
        [PrimaryKey] public int Id;
        public string Name;
        public int Age;
        public bool Active;
    }

    [Table(Name = "products")]
    public partial struct Products
    {
        [PrimaryKey] public int Id;
        public string Title;
        public float Price;
        public bool InStock;
    }

    [Table(Name = "notes")]
    public partial struct Notes
    {
        [PrimaryKey] public int Id;
        public string Body;
        public long Rating;
        public bool Pinned;
    }
}
