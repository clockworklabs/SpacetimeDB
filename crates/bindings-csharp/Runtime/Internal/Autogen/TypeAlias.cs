// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.
// <auto-generated />

#nullable enable

using System;
using SpacetimeDB;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.Serialization;

namespace SpacetimeDB.Internal
{
	[SpacetimeDB.Type]
	[DataContract]
	public partial class TypeAlias
	{
		[DataMember(Name = "name")]
		public string Name;
		[DataMember(Name = "ty")]
		public uint Ty;

		public TypeAlias(
			string Name,
			uint Ty
		)
		{
			this.Name = Name;
			this.Ty = Ty;
		}

		public TypeAlias() : this(
			"",
			default!
		) { }
	}
}
