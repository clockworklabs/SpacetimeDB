// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

// This was generated using spacetimedb cli version 1.1.1 (commit 92e49e96f461b4496bdab42facbab2c5d39d20f4).

#nullable enable

using System;
using System.Collections.Generic;
using System.Runtime.Serialization;

namespace SpacetimeDB.Internal
{
    [SpacetimeDB.Type]
    [DataContract]
    public sealed partial class RawColumnDefV8
    {
        [DataMember(Name = "col_name")]
        public string ColName;
        [DataMember(Name = "col_type")]
        public SpacetimeDB.BSATN.AlgebraicType ColType;

        public RawColumnDefV8(
            string ColName,
            SpacetimeDB.BSATN.AlgebraicType ColType
        )
        {
            this.ColName = ColName;
            this.ColType = ColType;
        }

        public RawColumnDefV8()
        {
            this.ColName = "";
            this.ColType = null!;
        }
    }
}
