namespace SpacetimeDB.Internal;

/// <summary>Immutable assembly placement installed before module registration.</summary>
public sealed class NamespaceRegistry
{
    private readonly Dictionary<string, string> mounts = new(StringComparer.Ordinal);

    public NamespaceRegistry(string rootIdentity, IEnumerable<KeyValuePair<string, string>> mounts)
    {
        foreach (var mount in mounts)
        {
            if (mount.Key == rootIdentity)
                throw new ArgumentException("The root assembly cannot be mounted.", nameof(mounts));
            if (!this.mounts.TryAdd(mount.Key, mount.Value))
                throw new ArgumentException(
                    $"Assembly '{mount.Key}' is mounted more than once.",
                    nameof(mounts)
                );
        }
    }

    private string? ResolveNamespace(string assemblyIdentity) =>
        mounts.TryGetValue(assemblyIdentity, out var name)
        && !name.Equals("public", StringComparison.OrdinalIgnoreCase)
            ? name
            : null;

    public string Resolve(string assemblyIdentity, string localName) =>
        ResolveNamespace(assemblyIdentity) is { } name ? name + "." + localName : localName;

    public SqlTableName ResolveSqlName(string assemblyIdentity, string localName) =>
        ResolveNamespace(assemblyIdentity) is { } name
            ? new SqlTableName([name], localName)
            : new SqlTableName(localName);
}
