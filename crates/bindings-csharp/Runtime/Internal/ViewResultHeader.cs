namespace SpacetimeDB.Internal;

[SpacetimeDB.Type]
public partial record ViewResultHeader
    : SpacetimeDB.TaggedEnum<(SpacetimeDB.Unit RowData, string RawSql)>;
