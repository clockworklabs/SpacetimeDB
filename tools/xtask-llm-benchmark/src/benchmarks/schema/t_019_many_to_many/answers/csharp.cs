using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "User")]
    public partial struct User
    {
        [PrimaryKey, AutoInc] public ulong UserId;
        public string Name;
    }

    [Table(Accessor = "Group")]
    public partial struct Group
    {
        [PrimaryKey, AutoInc] public ulong GroupId;
        public string Title;
    }

    [Table(Accessor = "Membership")]
    [SpacetimeDB.Index.BTree(Accessor = "by_user",  Columns = new[] { nameof(UserId) })]
    [SpacetimeDB.Index.BTree(Accessor = "by_group", Columns = new[] { nameof(GroupId) })]
    public partial struct Membership
    {
        [PrimaryKey, AutoInc] public ulong Id;
        public ulong UserId;
        public ulong GroupId;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.User.Insert(new User { UserId = 0, Name = "Alice" });
        ctx.Db.User.Insert(new User { UserId = 0, Name = "Bob" });

        ctx.Db.Group.Insert(new Group { GroupId = 0, Title = "Admin" });
        ctx.Db.Group.Insert(new Group { GroupId = 0, Title = "Dev" });

        ctx.Db.Membership.Insert(new Membership { Id = 0, UserId = 1, GroupId = 1 });
        ctx.Db.Membership.Insert(new Membership { Id = 0, UserId = 1, GroupId = 2 });
        ctx.Db.Membership.Insert(new Membership { Id = 0, UserId = 2, GroupId = 2 });
    }
}
