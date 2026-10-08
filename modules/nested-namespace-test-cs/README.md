# Nested C# Schema, Dispatch, and Tables

This .NET 10 fixture registers one Leaf assembly six times: under Branch.Leaf,
Leaf, Promoted, SecondLeaf, class, and class.Branch.Leaf. Their database names are
branch_data.nested_data, leaf_data, promoted_data, second_data, outer_data, and
outer_data.branch_data.nested_data. The otherwise unused Public dependency
contributes its Promoted mount directly to the root scope. Branch itself is
instantiated twice, and `class` exercises a C# keyword accessor. Outer registers
Leaf in public, so its tables appear directly at `ctx.Db.@class.User`.

The integration test verifies independent child schemas and depth-first reducer,
procedure, view, and anonymous-view dispatch, including empty function categories.
Functions return or log their context's instance ID so repeated calls to the same
DLL can be distinguished. Root and Leaf also define different User row types.
Leaf's HTTP route must be ignored at every mount, with publication warnings.

Each Leaf instance writes its own User table using cached writable handles,
primary-key and B-tree indexes, scans, deletes, and clear. Procedure transactions
commit to that instance and roll back failed writes. Both kinds of views read the
same rows through read-only handles. Root reducers and views access tables two
and three levels deep (`ctx.Db.Branch.Leaf.User` and
`ctx.Db.@class.Branch.Leaf.User`). Writing through one path must not change the
other copy. Calling Leaf directly with an ambiguous root context must fail.

The [generated client](../../sdks/csharp/examples~/regression-tests/nested-namespaces/)
also runs against this module on .NET 8 and .NET 10. It covers nested client query
builders, cache updates, reducer events/errors, procedure results/errors, views,
joins, one-off queries, and unsubscribe cleanup.

Module query views also exercise library-local `ctx.From.User()` in every Leaf
instance, child queries in both Branch instances, and three-level root queries
with filters and both semijoin directions. Public contributions and promoted
mounts use the same instance-aware query paths.

Context-selection checks pass `ctx.As` paths to ordinary C# methods in Leaf and
Branch. They cover every Leaf placement, selection again inside Branch, both
view contexts, procedure and handler transactions, shared auth/RNG/UUID state,
and commit/rollback without changing the caller. A retained selected transaction
also observes refreshed timestamps on later attempts. The root `/contexts`
HTTP route exercises handler selection without enabling submodule routes.

The root `/schedule/*` routes exercise immediate reducer and procedure scheduling
against both nested Leaf copies. Calls use `ctx.As` paths directly and resolve
Leaf relative to a selected Branch context. The host test repeats the sequence
to exercise cached names, waits for execution, and rejects ambiguous root calls.
