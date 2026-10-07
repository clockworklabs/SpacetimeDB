// Module-shaped client bindings, in the form that `spacetime generate` writes.

using System.Collections.Generic;
using System.Runtime.Serialization;

namespace ClientGen
{
    [SpacetimeDB.Table(Accessor = "Person", Name = "person", Public = true)]
    [DataContract]
    public sealed partial class Person
    {
        [DataMember(Name = "id")]
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public uint Id;

        [DataMember(Name = "name")]
        [SpacetimeDB.Unique]
        public string Name = "";

        [DataMember(Name = "age")]
        [SpacetimeDB.Index.BTree(Name = "person_age_idx_btree")]
        public byte Age;

        // The client has no index for an index without an accessor, but the query builder indexes it.
        [DataMember(Name = "team")]
        [SpacetimeDB.Index.BTree(Accessor = "", Name = "person_team_idx_btree")]
        public uint Team;
    }

    [SpacetimeDB.Type]
    [DataContract]
    public sealed partial class Visit
    {
        [DataMember(Name = "at")]
        public SpacetimeDB.Timestamp At;

        [DataMember(Name = "person_id")]
        public uint PersonId;
    }

    public static partial class Module
    {
        [SpacetimeDB.Reducer(Name = "add_person")]
        public static partial void AddPerson(SpacetimeDB.ReducerContext ctx, string name, byte age);

        // The client has no index for a Timestamp primary key, but the query builder indexes it.
        [SpacetimeDB.View(
            Accessor = "RecentVisits",
            Name = "recent_visits",
            Public = true,
            PrimaryKey = "At"
        )]
        public static partial List<Visit> RecentVisits(SpacetimeDB.ViewContext ctx);
    }
}
