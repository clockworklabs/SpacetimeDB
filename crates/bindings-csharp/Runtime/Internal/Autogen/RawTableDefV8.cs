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
	public partial class RawTableDefV8
	{
		[DataMember(Name = "table_name")]
		public string TableName;
		[DataMember(Name = "columns")]
		public System.Collections.Generic.List<SpacetimeDB.Internal.RawColumnDefV8> Columns;
		[DataMember(Name = "indexes")]
		public System.Collections.Generic.List<SpacetimeDB.Internal.RawIndexDefV8> Indexes;
		[DataMember(Name = "constraints")]
		public System.Collections.Generic.List<SpacetimeDB.Internal.RawConstraintDefV8> Constraints;
		[DataMember(Name = "sequences")]
		public System.Collections.Generic.List<SpacetimeDB.Internal.RawSequenceDefV8> Sequences;
		[DataMember(Name = "table_type")]
		public string TableType;
		[DataMember(Name = "table_access")]
		public string TableAccess;
		[DataMember(Name = "scheduled")]
		public string? Scheduled;

		public RawTableDefV8(
			string TableName,
			System.Collections.Generic.List<SpacetimeDB.Internal.RawColumnDefV8> Columns,
			System.Collections.Generic.List<SpacetimeDB.Internal.RawIndexDefV8> Indexes,
			System.Collections.Generic.List<SpacetimeDB.Internal.RawConstraintDefV8> Constraints,
			System.Collections.Generic.List<SpacetimeDB.Internal.RawSequenceDefV8> Sequences,
			string TableType,
			string TableAccess,
			string? Scheduled
		)
		{
			this.TableName = TableName;
			this.Columns = Columns;
			this.Indexes = Indexes;
			this.Constraints = Constraints;
			this.Sequences = Sequences;
			this.TableType = TableType;
			this.TableAccess = TableAccess;
			this.Scheduled = Scheduled;
		}

		public RawTableDefV8() : this(
			"",
			new(),
			new(),
			new(),
			new(),
			"",
			"",
			default!
		) { }
	}
}
