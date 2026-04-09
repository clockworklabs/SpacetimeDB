#nullable enable

namespace SpacetimeDB.ClientApi
{
    [SpacetimeDB.Type]
    internal partial record ServerFrame : SpacetimeDB.TaggedEnum<(
        byte[] Single,
        byte[][] Batch
    )>;
}
