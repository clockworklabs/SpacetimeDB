using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "users")]
    public partial struct User
    {
        [PrimaryKey] public int UserId;
        public string Name;
    }

    [Table(Name = "groups")]
    public partial struct Group
    {
        [PrimaryKey] public int GroupId;
        public string Title;
    }

    [Table(Name = "memberships")]
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
        ctx.Db.users.Insert(new User { UserId = 1, Name = "Alice" });
        ctx.Db.users.Insert(new User { UserId = 2, Name = "Bob" });

        ctx.Db.groups.Insert(new Group { GroupId = 10, Title = "Admin" });
        ctx.Db.groups.Insert(new Group { GroupId = 20, Title = "Dev" });

        ctx.Db.memberships.Insert(new Membership { Id = 1, UserId = 1, GroupId = 10 });
        ctx.Db.memberships.Insert(new Membership { Id = 2, UserId = 1, GroupId = 20 });
        ctx.Db.memberships.Insert(new Membership { Id = 3, UserId = 2, GroupId = 20 });
    }
}
