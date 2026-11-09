namespace SpacetimeDB.Internal;

/// <summary>Precomputed child selection for a generated namespace accessor.</summary>
/// <param name="assemblyIdentity">
/// The declaring assembly for Db extensions, or null when a namespace container
/// already supplies the exact parent instance.
/// </param>
/// <param name="accessor">The child's local accessor.</param>
public sealed class NamespaceBinding(string? assemblyIdentity, string accessor)
{
    // Lazy because generated bindings are constructed together on first use. Managed test
    // fixtures never install global namespaces and resolve through their own backend instead.
    private int[] Instances => field ??= Module.BindNamespace(assemblyIdentity, accessor);

#if NET10_0_OR_GREATER
    public int ResolveContext(IModuleContext context, int contextInstance) =>
#if !SPACETIMEDB_WASM_RUNTIME
        Resolve(contextInstance, DatabaseAccess.Require(context.Access));
#else
        this[contextInstance];
#endif

#if SPACETIMEDB_WASM_RUNTIME
    // Generated namespace accessors call these on every table access. In WASM a database
    // handle is just an instance id, so reuse the cached handle per instance instead of allocating.
    public Local Select(Local db) => global::SpacetimeDB.Local.ForInstance(this[db.InstanceId]);

    public LocalReadOnly Select(LocalReadOnly db) => LocalReadOnly.ForInstance(this[db.InstanceId]);

    public QueryBuilder Select(QueryBuilder from) => new(this[from.InstanceId]);
#else
    // Managed handles also carry the test backend's transaction access, so they can't be shared.
    public Local Select(Local db) =>
        new() { InstanceId = Resolve(db.InstanceId, db.Access), Access = db.Access };

    public LocalReadOnly Select(LocalReadOnly db) =>
        new() { InstanceId = Resolve(db.InstanceId, db.Access), Access = db.Access };

    public QueryBuilder Select(QueryBuilder from) =>
        new(
            new LocalReadOnly
            {
                InstanceId = Resolve(from.InstanceId, from.Access),
                Access = from.Access,
            }
        );

    private int Resolve(int contextInstance, DatabaseAccess? access)
    {
        if (access is null)
        {
            return this[contextInstance];
        }

        access.CheckActive();
        var selected = access.Backend.Namespaces.BindNamespace(assemblyIdentity, accessor)[
            contextInstance
        ];
        return selected < 0
            ? throw new InvalidOperationException(
                $"Namespace '{accessor}' is unavailable or ambiguous in this context."
            )
            : selected;
    }
#endif
#endif

    public int this[int contextInstance]
    {
        get
        {
            var instance = Instances[contextInstance];
            return instance < 0
                ? throw new InvalidOperationException(
                    $"Namespace '{accessor}' is unavailable or ambiguous in this context."
                )
                : instance;
        }
    }
}
