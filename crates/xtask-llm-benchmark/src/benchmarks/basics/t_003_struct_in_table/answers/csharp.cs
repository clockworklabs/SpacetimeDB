using SpacetimeDB;

public static partial class Module
{
    [Type]
    public partial struct Position
    {
        public int X;
        public int Y;
    }

    [Table(Name = "entities")]
    public partial struct Entity
    {
        [PrimaryKey] public int Id;
        public Position Pos;
    }
}
