// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.
// <auto-generated />

#nullable enable

using System;
using SpacetimeDB;
using System.Collections.Generic;
using System.Runtime.Serialization;

namespace SpacetimeDB.Internal
{
	[SpacetimeDB.Type]
	[DataContract]
	public partial class RawColumnDefV8
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
