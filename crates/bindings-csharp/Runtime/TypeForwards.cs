using System.Runtime.CompilerServices;

// These attributes moved to SpacetimeDB.BSATN.Runtime so that client bindings can use them.
// Forwarding them keeps assemblies compiled against older versions of this assembly working.
[assembly: TypeForwardedTo(typeof(SpacetimeDB.Internal.ColumnAttrs))]
[assembly: TypeForwardedTo(typeof(SpacetimeDB.Internal.ColumnAttribute))]
[assembly: TypeForwardedTo(typeof(SpacetimeDB.TableAttribute))]
[assembly: TypeForwardedTo(typeof(SpacetimeDB.ViewAttribute))]
[assembly: TypeForwardedTo(typeof(SpacetimeDB.Index))]
[assembly: TypeForwardedTo(typeof(SpacetimeDB.AutoIncAttribute))]
[assembly: TypeForwardedTo(typeof(SpacetimeDB.PrimaryKeyAttribute))]
[assembly: TypeForwardedTo(typeof(SpacetimeDB.UniqueAttribute))]
[assembly: TypeForwardedTo(typeof(SpacetimeDB.DefaultAttribute))]
[assembly: TypeForwardedTo(typeof(SpacetimeDB.ReducerKind))]
[assembly: TypeForwardedTo(typeof(SpacetimeDB.ReducerAttribute))]
[assembly: TypeForwardedTo(typeof(SpacetimeDB.ProcedureAttribute))]
