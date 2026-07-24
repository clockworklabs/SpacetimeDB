using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "UserRecord", Public = true)]
    public partial struct UserRecord
    {
        [PrimaryKey] public Identity Identity;
        public string Name;
    }

    [ClientVisibilityFilter]
    public static readonly Filter UserRecordFilter = new Filter.Sql(
        "SELECT * FROM UserRecord WHERE Identity = :sender"
    );

    [Reducer]
    public static void RegisterSelf(ReducerContext ctx, string name) =>
        ctx.Db.UserRecord.Insert(new UserRecord { Identity = ctx.Sender, Name = name });
}
