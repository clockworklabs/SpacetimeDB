using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "User")]
    public partial struct User
    {
        [PrimaryKey] public int UserId;
        public string Name;
    }

    [Table(Name = "Group")]
    public partial struct Group
    {
        [PrimaryKey] public int GroupId;
        public string Title;
    }

    [Table(Name = "Membership")]
    [SpacetimeDB.Index.BTree(Name = "by_user",  Columns = new[] { nameof(UserId) })]
    [SpacetimeDB.Index.BTree(Name = "by_group", Columns = new[] { nameof(GroupId) })]
    public partial struct Membership
    {
        [PrimaryKey] public int Id;
        public int UserId;
        public int GroupId;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.User.Insert(new User { UserId = 1, Name = "Alice" });
        ctx.Db.User.Insert(new User { UserId = 2, Name = "Bob" });

        ctx.Db.Group.Insert(new Group { GroupId = 10, Title = "Admin" });
        ctx.Db.Group.Insert(new Group { GroupId = 20, Title = "Dev" });

        ctx.Db.Membership.Insert(new Membership { Id = 1, UserId = 1, GroupId = 10 });
        ctx.Db.Membership.Insert(new Membership { Id = 2, UserId = 1, GroupId = 20 });
        ctx.Db.Membership.Insert(new Membership { Id = 3, UserId = 2, GroupId = 20 });
    }
}
