using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Profile", Public = true)]
    public partial struct Profile
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        [Unique]
        public Identity Identity;
        public string Name;
        public string Bio;
    }

    [SpacetimeDB.View(Accessor = "MyProfile", Public = true)]
    public static Profile? MyProfile(ViewContext ctx)
    {
        return ctx.Db.Profile.Identity.Find(ctx.Sender) as Profile?;
    }
}
