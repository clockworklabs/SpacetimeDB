namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using static Utils;

// A node is one schema scope. Public contributions share it; named mounts create children.
internal record CompositionNode(
    int Id,
    int? ParentId,
    string AccessorPath,
    NamespaceDeclaration? Mount,
    EquatableArray<string> Contributors,
    EquatableArray<CompositionNode> Children
);

internal record ModuleComposition(CompositionNode Root, bool IsValid)
{
    public IEnumerable<CompositionNode> Nodes => Traverse(Root);

    private static IEnumerable<CompositionNode> Traverse(CompositionNode node)
    {
        yield return node;
        foreach (var child in node.Children)
        {
            foreach (var descendant in Traverse(child))
            {
                yield return descendant;
            }
        }
    }

    public static ModuleComposition Build(
        string rootIdentity,
        IEnumerable<NamespaceDeclaration> rootMounts,
        IEnumerable<AssemblyDeclaration> dependencies,
        DiagReporter diag,
        CancellationToken cancellationToken
    ) => new Builder(rootIdentity, rootMounts, dependencies, diag, cancellationToken).Build();

    private sealed class Builder(
        string rootIdentity,
        IEnumerable<NamespaceDeclaration> rootMounts,
        IEnumerable<AssemblyDeclaration> dependencies,
        DiagReporter diag,
        CancellationToken cancellationToken
    )
    {
        private readonly Dictionary<string, NamespaceDeclaration[]> declarations =
            dependencies.ToDictionary(
                a => a.Identity,
                a => a.Mounts.ToArray(),
                StringComparer.Ordinal
            );
        private bool valid = true;
        private int nextId;

        public ModuleComposition Build()
        {
            declarations.Add(rootIdentity, [.. rootMounts]);
            var visited = new HashSet<string>(StringComparer.Ordinal);
            foreach (
                var identity in declarations.Keys.OrderBy(
                    identity => identity,
                    StringComparer.Ordinal
                )
            )
            {
                CheckCycles(identity, visited, []);
            }

            if (!valid)
            {
                return new(
                    new(
                        0,
                        null,
                        "",
                        null,
                        new(ImmutableArray.Create(rootIdentity)),
                        new(ImmutableArray<CompositionNode>.Empty)
                    ),
                    false
                );
            }

            // Discovery is deduplicated by assembly; composition is not. A mount target
            // belongs to its declared locations, not also to the automatic public set.
            var targets = new HashSet<string>(
                declarations
                    .Values.SelectMany(mounts => mounts)
                    .Select(mount => mount.AssemblyIdentity),
                StringComparer.Ordinal
            );
            var publicDependencies = declarations
                .Keys.Where(identity => identity != rootIdentity && !targets.Contains(identity))
                .OrderBy(identity => identity, StringComparer.Ordinal);
            var root = BuildNode(null, null, "", new[] { rootIdentity }.Concat(publicDependencies));
            return new(root, valid);
        }

        private void CheckCycles(string identity, HashSet<string> visited, List<string> ancestors)
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (ancestors.Contains(identity))
            {
                Report(
                    $"Namespace mount cycle: {string.Join(" -> ", ancestors.Concat([identity]))}."
                );
                return;
            }
            if (!visited.Add(identity))
            {
                return;
            }

            ancestors.Add(identity);
            foreach (var mount in declarations[identity])
            {
                CheckCycles(mount.AssemblyIdentity, visited, ancestors);
            }
            ancestors.RemoveAt(ancestors.Count - 1);
        }

        private CompositionNode BuildNode(
            int? parentId,
            NamespaceDeclaration? mount,
            string path,
            IEnumerable<string> initialContributors
        )
        {
            cancellationToken.ThrowIfCancellationRequested();
            var id = nextId++;
            var contributors = new HashSet<string>(StringComparer.Ordinal);
            var pending = new Queue<string>(initialContributors);
            var first = pending.Peek();
            var childMounts = new List<NamespaceDeclaration>();
            while (pending.Count != 0)
            {
                var identity = pending.Dequeue();
                if (!contributors.Add(identity))
                {
                    continue;
                }
                foreach (var declaration in declarations[identity])
                {
                    if (declaration.Accessor.Equals("public", StringComparison.OrdinalIgnoreCase))
                    {
                        pending.Enqueue(declaration.AssemblyIdentity);
                    }
                    else
                    {
                        childMounts.Add(declaration);
                    }
                }
            }

            var accessors = new Dictionary<string, NamespaceDeclaration>(
                StringComparer.OrdinalIgnoreCase
            );
            var names = new Dictionary<string, NamespaceDeclaration>(StringComparer.Ordinal);
            var children = ImmutableArray.CreateBuilder<CompositionNode>();
            // Preserve the existing one-level registration order. Additional mounts of
            // the same assembly are ordered by accessor, independently of reference order.
            foreach (
                var child in childMounts
                    .OrderBy(m => m.AssemblyIdentity, StringComparer.Ordinal)
                    .ThenBy(m => m.Accessor, StringComparer.Ordinal)
            )
            {
                CheckSiblingName(accessors, child.Accessor, child, path, "accessor");
                if (child.Name is { } name)
                {
                    // Do not reproduce the host's case-conversion policy here.
                    CheckSiblingName(names, name, child, path, "explicit database name");
                }
                children.Add(
                    BuildNode(
                        id,
                        child,
                        path.Length == 0 ? child.Accessor : path + "." + child.Accessor,
                        [child.AssemblyIdentity]
                    )
                );
            }

            return new(
                id,
                parentId,
                path,
                mount,
                new(
                    new[] { first }
                        .Concat(
                            contributors
                                .Where(identity => identity != first)
                                .OrderBy(identity => identity, StringComparer.Ordinal)
                        )
                        .ToImmutableArray()
                ),
                new(children.ToImmutable())
            );
        }

        private void CheckSiblingName(
            Dictionary<string, NamespaceDeclaration> names,
            string name,
            NamespaceDeclaration mount,
            string path,
            string kind
        )
        {
            if (names.TryGetValue(name, out var previous))
            {
                Report(
                    $"Namespace {kind} '{name}' in scope '{(path.Length == 0 ? "public" : path)}' is contributed by both '{previous.DeclaringAssemblyIdentity}' and '{mount.DeclaringAssemblyIdentity}'."
                );
            }
            else
            {
                names.Add(name, mount);
            }
        }

        private void Report(string message)
        {
            valid = false;
            diag.Report(ErrorDescriptor.InvalidNamespaceComposition, message);
        }
    }
}
