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
	public partial class RawIndexDefV9
	{
		[DataMember(Name = "name")]
		public string? Name;
		[DataMember(Name = "accessor_name")]
		public string? AccessorName;
		[DataMember(Name = "algorithm")]
		public SpacetimeDB.Internal.RawIndexAlgorithm Algorithm;

		public RawIndexDefV9(
			string? Name,
			string? AccessorName,
			SpacetimeDB.Internal.RawIndexAlgorithm Algorithm
		)
		{
			this.Name = Name;
			this.AccessorName = AccessorName;
			this.Algorithm = Algorithm;
		}

		public RawIndexDefV9()
		{
			this.Algorithm = null!;
		}

	}
}
