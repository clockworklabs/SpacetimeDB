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
	public partial class RawIndexDefV8
	{
		[DataMember(Name = "index_name")]
		public string IndexName;
		[DataMember(Name = "is_unique")]
		public bool IsUnique;
		[DataMember(Name = "index_type")]
		public SpacetimeDB.Internal.IndexType IndexType;
		[DataMember(Name = "columns")]
		public System.Collections.Generic.List<ushort> Columns;

		public RawIndexDefV8(
			string IndexName,
			bool IsUnique,
			SpacetimeDB.Internal.IndexType IndexType,
			System.Collections.Generic.List<ushort> Columns
		)
		{
			this.IndexName = IndexName;
			this.IsUnique = IsUnique;
			this.IndexType = IndexType;
			this.Columns = Columns;
		}

		public RawIndexDefV8()
		{
			this.IndexName = "";
			this.Columns = new();
		}

	}
}
