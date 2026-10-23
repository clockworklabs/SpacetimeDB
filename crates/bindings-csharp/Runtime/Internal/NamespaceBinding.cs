namespace SpacetimeDB.Internal;

/// <summary>Precomputed child selection for a generated namespace accessor.</summary>
/// <param name="assemblyIdentity">
/// The declaring assembly for Db extensions, or null when a namespace container
/// already supplies the exact parent instance.
/// </param>
/// <param name="accessor">The child's local accessor.</param>
public sealed class NamespaceBinding(string? assemblyIdentity, string accessor)
{
    private readonly int[] instances = Module.BindNamespace(assemblyIdentity, accessor);

    public int this[int contextInstance]
    {
        get
        {
            var instance = instances[contextInstance];
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
