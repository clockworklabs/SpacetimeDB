using System.Runtime.CompilerServices;
using SpacetimeDB;

// Compiled server module libraries reference these types through SpacetimeDB.Runtime.
[assembly: TypeForwardedTo(typeof(SpacetimeDB.Internal.ColumnAttribute))]
[assembly: TypeForwardedTo(typeof(SpacetimeDB.Internal.ColumnAttrs))]
[assembly: TypeForwardedTo(typeof(TableAttribute))]
[assembly: TypeForwardedTo(typeof(TableCachePolicy))]
[assembly: TypeForwardedTo(typeof(ViewAttribute))]
[assembly: TypeForwardedTo(typeof(SpacetimeDB.Index))]
[assembly: TypeForwardedTo(typeof(AutoIncAttribute))]
[assembly: TypeForwardedTo(typeof(PrimaryKeyAttribute))]
[assembly: TypeForwardedTo(typeof(UniqueAttribute))]
[assembly: TypeForwardedTo(typeof(DefaultAttribute))]
[assembly: TypeForwardedTo(typeof(ReducerKind))]
[assembly: TypeForwardedTo(typeof(ReducerAttribute))]
[assembly: TypeForwardedTo(typeof(ProcedureAttribute))]
#if NET10_0_OR_GREATER
[assembly: TypeForwardedTo(typeof(NamespaceAttribute))]
#endif
