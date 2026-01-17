using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "User")]
    public partial struct User
    {
        [PrimaryKey] public int Id;
        public string Name;
        public int Age;
        public bool Active;
    }

    [Table(Name = "Product")]
    public partial struct Product
    {
        [PrimaryKey] public int Id;
        public string Title;
        public float Price;
        public bool InStock;
    }

    [Table(Name = "Note")]
    public partial struct Note
    {
        [PrimaryKey] public int Id;
        public string Body;
        public long Rating;
        public bool Pinned;
    }
}
