#nullable enable

using System;
using SpacetimeDB;
using System.Collections.Generic;
using System.Runtime.Serialization;

namespace SpacetimeDB.ClientApi
{
    [SpacetimeDB.Type]
    [DataContract]
    public partial class IdsToNames
    {
        [DataMember(Name = "reducer_ids")]
        public System.Collections.Generic.List<uint> ReducerIds;
        [DataMember(Name = "reducer_names")]
        public System.Collections.Generic.List<string> ReducerNames;
        [DataMember(Name = "table_ids")]
        public System.Collections.Generic.List<uint> TableIds;
        [DataMember(Name = "table_names")]
        public System.Collections.Generic.List<string> TableNames;

        public IdsToNames(
            System.Collections.Generic.List<uint> ReducerIds,
            System.Collections.Generic.List<string> ReducerNames,
            System.Collections.Generic.List<uint> TableIds,
            System.Collections.Generic.List<string> TableNames
        )
        {
            this.ReducerIds = ReducerIds;
            this.ReducerNames = ReducerNames;
            this.TableIds = TableIds;
            this.TableNames = TableNames;
        }

        public IdsToNames()
        {
            ReducerIds = new();
            ReducerNames = new();
            TableIds = new();
            TableNames = new();
        }

    }
}
