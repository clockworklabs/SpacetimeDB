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
	public partial class ReducerDef
	{
		[DataMember(Name = "name")]
		public string Name;
		[DataMember(Name = "args")]
		public System.Collections.Generic.List<SpacetimeDB.BSATN.AggregateElement> Args;

		public ReducerDef(
			string Name,
			System.Collections.Generic.List<SpacetimeDB.BSATN.AggregateElement> Args
		)
		{
			this.Name = Name;
			this.Args = Args;
		}

		public ReducerDef()
		{
			this.Name = "";
			this.Args = new();
		}

	}
}
