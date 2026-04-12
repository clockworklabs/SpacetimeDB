#nullable enable

namespace SpacetimeDB.ClientApi
{
    [SpacetimeDB.Type]
    internal partial record ClientFrame : SpacetimeDB.TaggedEnum<(
        byte[] Single,
        byte[][] Batch
    )>;
}
