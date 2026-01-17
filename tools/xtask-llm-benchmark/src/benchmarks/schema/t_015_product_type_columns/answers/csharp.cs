using SpacetimeDB;

public static partial class Module
{
    [Type]
    public partial struct Address
    {
        public string Street;
        public int Zip;
    }

    [Type]
    public partial struct Position
    {
        public int X;
        public int Y;
    }

    [Table(Name = "Profile", Public = true)]
    public partial struct Profile
    {
        [PrimaryKey] public int Id;
        public Address Home;
        public Address Work;
        public Position Pos;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.Profile.Insert(new Profile {
            Id = 1,
            Home = new Address { Street = "1 Main", Zip = 11111 },
            Work = new Address { Street = "2 Broad", Zip = 22222 },
            Pos  = new Position { X = 7, Y = 9 }
        });
    }
}
