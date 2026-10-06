using SpacetimeDB;

[assembly: Namespace(typeof(NestedBranch.Functions), Accessor = "Branch", Name = "branch_data")]
[assembly: Namespace(typeof(NestedLeaf.Functions), Accessor = "public")]

namespace NestedOuter;

public class Marker { }
