namespace SpacetimeDB.Internal;

/// <summary>Precomputed child selection for a generated namespace accessor.</summary>
/// <param name="assemblyIdentity">
/// The declaring assembly for Db extensions, or null when a namespace container
/// already supplies the exact parent instance.
/// </param>
/// <param name="accessor">The child's local accessor.</param>
public sealed class NamespaceBinding(string? assemblyIdentity, string accessor)
{
    private int[] Instances => field ??= Module.BindNamespace(assemblyIdentity, accessor);

#if NET10_0_OR_GREATER
    public Local Select(Local db) => new() {
        InstanceId = Resolve(db.InstanceId
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
            , db.Access
#endif
        ),
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        Access = db.Access,
#endif
    };

    public LocalReadOnly Select(LocalReadOnly db) => new() {
        InstanceId = Resolve(db.InstanceId
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
            , db.Access
#endif
        ),
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        Access = db.Access,
#endif
    };

    public QueryBuilder Select(QueryBuilder from) => new(new LocalReadOnly {
        InstanceId = Resolve(from.InstanceId
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
            , from.Access
#endif
        ),
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        Access = from.Access,
#endif
    });

    private int Resolve(int contextInstance
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        , DatabaseAccess? access
#endif
    )
    {
#if NET10_0_OR_GREATER && !SPACETIMEDB_WASM_RUNTIME
        if (access is not null)
        {
            access.CheckActive();
            var selected = access.Backend.Namespaces.BindNamespace(assemblyIdentity, accessor)[contextInstance];
            if (selected < 0)
                throw new InvalidOperationException($"Namespace '{accessor}' is unavailable or ambiguous in this context.");
            return selected;
        }
#endif
        return this[contextInstance];
    }

#endif

    public int this[int contextInstance]
    {
        get
        {
            var instance = Instances[contextInstance];
            if (instance < 0)
            {
                throw new InvalidOperationException(
                    $"Namespace '{accessor}' is unavailable or ambiguous in this context."
                );
            }
            return instance;
        }
    }
}
