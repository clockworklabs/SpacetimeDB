using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "UserProfile", Public = true)]
    public partial struct UserProfile
    {
        [PrimaryKey] public Identity Identity;
        public string DisplayName;
        public Timestamp CreatedAt;
    }

    [Table(Accessor = "ConnectionPresence", Public = true)]
    [SpacetimeDB.Index.BTree(Accessor = "by_user", Columns = new[] { nameof(UserIdentity) })]
    public partial struct ConnectionPresence
    {
        [PrimaryKey] public ConnectionId ConnectionId;
        public Identity UserIdentity;
        public Timestamp LastHeartbeat;
        public bool Online;
    }
}
