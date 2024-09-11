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
	public partial class RawModuleDefV9
	{
		[DataMember(Name = "typespace")]
		public SpacetimeDB.Internal.Typespace Typespace;
		[DataMember(Name = "tables")]
		public System.Collections.Generic.List<SpacetimeDB.Internal.RawTableDefV9> Tables;
		[DataMember(Name = "reducers")]
		public System.Collections.Generic.List<SpacetimeDB.Internal.RawReducerDefV9> Reducers;
		[DataMember(Name = "types")]
		public System.Collections.Generic.List<SpacetimeDB.Internal.RawTypeDefV9> Types;
		[DataMember(Name = "misc_exports")]
		public System.Collections.Generic.List<SpacetimeDB.Internal.RawMiscModuleExportV9> MiscExports;

		public RawModuleDefV9(
			SpacetimeDB.Internal.Typespace Typespace,
			System.Collections.Generic.List<SpacetimeDB.Internal.RawTableDefV9> Tables,
			System.Collections.Generic.List<SpacetimeDB.Internal.RawReducerDefV9> Reducers,
			System.Collections.Generic.List<SpacetimeDB.Internal.RawTypeDefV9> Types,
			System.Collections.Generic.List<SpacetimeDB.Internal.RawMiscModuleExportV9> MiscExports
		)
		{
			this.Typespace = Typespace;
			this.Tables = Tables;
			this.Reducers = Reducers;
			this.Types = Types;
			this.MiscExports = MiscExports;
		}

		public RawModuleDefV9() : this(
			new(),
			new(),
			new(),
			new(),
			new()
		) { }
	}
}