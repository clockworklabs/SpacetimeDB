// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.
// <auto-generated />

#nullable enable

using System;
using SpacetimeDB;

namespace SpacetimeDB.Internal
{
	[SpacetimeDB.Type]
	public partial record RawConstraintDataV9 : SpacetimeDB.TaggedEnum<(
		SpacetimeDB.Internal.RawUniqueConstraintDataV9 Unique,
		SpacetimeDB.Unit
	)>;
}
