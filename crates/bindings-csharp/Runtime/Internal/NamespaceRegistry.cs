namespace SpacetimeDB.Internal;

/// <summary>Immutable assembly placement installed before module registration.</summary>
public sealed class NamespaceRegistry
{
    private readonly Dictionary<
        string,
        (string Accessor, string Canonical, CaseConversionPolicy Policy)
    > mounts = new(StringComparer.Ordinal);
    private readonly CaseConversionPolicy rootPolicy;
    private readonly HashSet<string> ambiguous = new(StringComparer.Ordinal);
    private readonly string[] instancePaths;
    private readonly string[] canonicalPaths;
    private readonly CaseConversionPolicy[] policies;
    private readonly Dictionary<string, int[]> instances = new(StringComparer.Ordinal);
    private readonly Dictionary<(int Parent, string Accessor), int> children = [];

    public int InstanceCount => instancePaths.Length;

    public int ResolveInstance(int contextInstance, string assemblyIdentity)
    {
        if ((uint)contextInstance >= (uint)InstanceCount)
        {
            throw new ArgumentOutOfRangeException(nameof(contextInstance));
        }
        if (!instances.TryGetValue(assemblyIdentity, out var candidates))
        {
            return 0;
        }
        var instance = candidates[contextInstance];
        if (instance < 0)
        {
            throw new InvalidOperationException(
                $"Assembly '{assemblyIdentity}' has multiple instances accessible from '{instancePaths[contextInstance]}'. Select a specific submodule context."
            );
        }
        return instance;
    }

    public string Resolve(int instanceId, string localName) =>
        Qualify(instancePaths[instanceId], localName);

    internal int[] BindNamespace(string? assemblyIdentity, string accessor)
    {
        var parents = assemblyIdentity is null ? null : instances[assemblyIdentity];
        var result = new int[InstanceCount];
        for (var context = 0; context < result.Length; context++)
        {
            var parent = parents is null ? context : parents[context];
            result[context] = children.TryGetValue((parent, accessor), out var child) ? child : -1;
        }
        return result;
    }

    public NamespaceRegistry(
        (
            int ParentId,
            string Accessor,
            string? Name,
            CaseConversionPolicy Policy,
            string[] Assemblies
        )[] scopes
    )
    {
        if (scopes.Length == 0 || scopes[0].ParentId != -1)
        {
            throw new ArgumentException(
                "The composition must start with the root scope.",
                nameof(scopes)
            );
        }
        rootPolicy = scopes[0].Policy;
        instancePaths = new string[scopes.Length];
        canonicalPaths = new string[scopes.Length];
        policies = [.. scopes.Select(scope => scope.Policy)];
        for (var id = 0; id < scopes.Length; id++)
        {
            var scope = scopes[id];
            if (id == 0)
            {
                instancePaths[id] = "";
                canonicalPaths[id] = "";
            }
            else
            {
                if (scope.ParentId < 0 || scope.ParentId >= id)
                {
                    throw new ArgumentException("A scope must follow its parent.", nameof(scopes));
                }
                children.Add((scope.ParentId, scope.Accessor), id);
                var name =
                    scope.Name
                    ?? CanonicalName.Convert(scope.Accessor, scopes[scope.ParentId].Policy);
                instancePaths[id] = Qualify(instancePaths[scope.ParentId], scope.Accessor);
                canonicalPaths[id] = Qualify(canonicalPaths[scope.ParentId], name);
            }
            foreach (var assembly in scope.Assemblies)
            {
                if (!mounts.TryAdd(assembly, (instancePaths[id], canonicalPaths[id], scope.Policy)))
                {
                    ambiguous.Add(assembly);
                }
            }
        }
        // Resolve ownership once at startup, not for every table operation.
        var parents = scopes.Select(scope => scope.ParentId).ToArray();
        foreach (var assembly in mounts.Keys)
        {
            var owners = Enumerable
                .Range(0, scopes.Length)
                .Where(id => scopes[id].Assemblies.Contains(assembly))
                .ToArray();
            var resolved = new int[scopes.Length];
            for (var context = 0; context < scopes.Length; context++)
            {
                if (owners.Contains(context))
                {
                    resolved[context] = context;
                    continue;
                }
                var selected = -1;
                foreach (var owner in owners)
                {
                    if (!IsDescendant(owner, context, parents))
                    {
                        continue;
                    }
                    if (selected >= 0)
                    {
                        selected = -1;
                        break;
                    }
                    selected = owner;
                }
                resolved[context] = owners.Length == 1 ? owners[0] : selected;
            }
            instances.Add(assembly, resolved);
        }
    }

    private static bool IsDescendant(int id, int ancestor, int[] parents)
    {
        for (var parent = parents[id]; parent >= 0; parent = parents[parent])
        {
            if (parent == ancestor)
            {
                return true;
            }
        }
        return false;
    }

    private static string Qualify(string path, string name) =>
        path.Length == 0 ? name : path + "." + name;

    private void CheckUnambiguous(string assemblyIdentity)
    {
        if (ambiguous.Contains(assemblyIdentity))
        {
            throw new InvalidOperationException(
                $"Assembly '{assemblyIdentity}' has multiple mount instances. This lookup requires an instance ID."
            );
        }
    }

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
        : this(CreateFlatScopes(rootIdentity, rootPolicy, mounts)) { }

    private static (
        int ParentId,
        string Accessor,
        string? Name,
        CaseConversionPolicy Policy,
        string[] Assemblies
    )[] CreateFlatScopes(
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
        var identities = new HashSet<string>(StringComparer.Ordinal) { rootIdentity };
        var publicAssemblies = new List<string> { rootIdentity };
        var scopes = new List<(int, string, string?, CaseConversionPolicy, string[])> { default };
        foreach (var mount in mounts)
        {
            if (mount.AssemblyIdentity == rootIdentity)
            {
                throw new ArgumentException("The root assembly cannot be mounted.", nameof(mounts));
            }

            if (!identities.Add(mount.AssemblyIdentity))
            {
                throw new ArgumentException(
                    $"Assembly '{mount.AssemblyIdentity}' is mounted more than once.",
                    nameof(mounts)
                );
            }
            if (
                mount.Accessor.Length == 0
                || mount.Accessor.Equals("public", StringComparison.OrdinalIgnoreCase)
            )
            {
                publicAssemblies.Add(mount.AssemblyIdentity);
            }
            else
            {
                scopes.Add((0, mount.Accessor, mount.Name, mount.Policy, [mount.AssemblyIdentity]));
            }
        }
        scopes[0] = (-1, "", null, rootPolicy, [.. publicAssemblies]);
        return [.. scopes];
    }

    private string? ResolveNamespace(string assemblyIdentity)
    {
        CheckUnambiguous(assemblyIdentity);
        return mounts.TryGetValue(assemblyIdentity, out var mount) && mount.Accessor.Length != 0
            ? mount.Accessor
            : null;
    }

    public string ResolveFunction(string assemblyIdentity, string sourceName, string? explicitName)
    {
        CheckUnambiguous(assemblyIdentity);
        if (mounts.TryGetValue(assemblyIdentity, out var mount) && mount.Accessor.Length != 0)
        {
            return mount.Canonical
                + "."
                + (explicitName ?? CanonicalName.Convert(sourceName, mount.Policy));
        }
        return explicitName ?? CanonicalName.Convert(sourceName, rootPolicy);
    }

    public string ResolveFunction(int instanceId, string sourceName, string? explicitName) =>
        Qualify(
            canonicalPaths[instanceId],
            explicitName ?? CanonicalName.Convert(sourceName, policies[instanceId])
        );

    public string Resolve(string assemblyIdentity, string localName) =>
        ResolveNamespace(assemblyIdentity) is { } name ? name + "." + localName : localName;

    public SqlTableName ResolveSqlName(string assemblyIdentity, string localName)
    {
        var name = ResolveNamespace(assemblyIdentity);
        return name is null
            ? new SqlTableName(localName)
            : new SqlTableName(name.Split('.'), localName);
    }

    public SqlTableName ResolveSqlName(int instanceId, string localName) =>
        instancePaths[instanceId].Length == 0
            ? new SqlTableName(localName)
            : new SqlTableName(instancePaths[instanceId].Split('.'), localName);
}
