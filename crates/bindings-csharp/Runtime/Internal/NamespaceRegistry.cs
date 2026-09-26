namespace SpacetimeDB.Internal;

/// <summary>Immutable assembly placement installed before module registration.</summary>
public sealed class NamespaceRegistry
{
    private readonly Dictionary<
        string,
        (string Accessor, string Canonical, CaseConversionPolicy Policy)
    > mounts = new(StringComparer.Ordinal);
    private readonly CaseConversionPolicy rootPolicy;

    public NamespaceRegistry(
        string rootIdentity,
        CaseConversionPolicy rootPolicy,
        IEnumerable<(
            string AssemblyIdentity,
            string Accessor,
            string? Name,
            CaseConversionPolicy Policy
        )> mounts
    )
    {
        this.rootPolicy = rootPolicy;
        foreach (var mount in mounts)
        {
            if (mount.AssemblyIdentity == rootIdentity)
            {
                throw new ArgumentException("The root assembly cannot be mounted.", nameof(mounts));
            }

            if (
                !this.mounts.TryAdd(
                    mount.AssemblyIdentity,
                    (
                        mount.Accessor,
                        mount.Name ?? CanonicalName.Convert(mount.Accessor, rootPolicy),
                        mount.Policy
                    )
                )
            )
            {
                throw new ArgumentException(
                    $"Assembly '{mount.AssemblyIdentity}' is mounted more than once.",
                    nameof(mounts)
                );
            }
        }
    }

    private string? ResolveNamespace(string assemblyIdentity) =>
        mounts.TryGetValue(assemblyIdentity, out var mount)
        && !mount.Accessor.Equals("public", StringComparison.OrdinalIgnoreCase)
            ? mount.Accessor
            : null;

    public string ResolveFunction(string assemblyIdentity, string sourceName, string? explicitName)
    {
        if (
            mounts.TryGetValue(assemblyIdentity, out var mount)
            && !mount.Accessor.Equals("public", StringComparison.OrdinalIgnoreCase)
        )
        {
            return mount.Canonical
                + "."
                + (explicitName ?? CanonicalName.Convert(sourceName, mount.Policy));
        }
        return explicitName ?? CanonicalName.Convert(sourceName, rootPolicy);
    }

    public string Resolve(string assemblyIdentity, string localName) =>
        ResolveNamespace(assemblyIdentity) is { } name ? name + "." + localName : localName;

    public SqlTableName ResolveSqlName(string assemblyIdentity, string localName) =>
        ResolveNamespace(assemblyIdentity) is { } name
            ? new SqlTableName(name, localName)
            : new SqlTableName(localName);
}
