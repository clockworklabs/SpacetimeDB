// Canonical module-definition metadata; declaration constraints contain no runtime values.
#nullable enable
namespace SpacetimeDB.Internal;

[SpacetimeDB.Type]
public partial record EnvironmentConstraint
    : SpacetimeDB.TaggedEnum<(
        SpacetimeDB.Unit AnyString,
        string Literal,
        System.Collections.Generic.List<string> OneOf
    )>;
