namespace SpacetimeDB.Codegen;

using System.Linq;
using Microsoft.CodeAnalysis.CSharp;
using static Utils;

internal static class ImmediateSchedule
{
    internal static string GenerateContextOverload(
        string name,
        string identifier,
        string assembly,
        string? canonicalName,
        EquatableArray<MemberDeclaration> args,
        string dispatcher
    )
    {
        var cache = $"__Schedule{name}Names";
        var contextType = UniqueName("__Context", args);
        var context = UniqueName("__context", args);
        var resolvedName = UniqueName("__name", args);
        return $$"""

            private static class {{cache}}
            {
                private static readonly string?[] Names = new string?[global::SpacetimeDB.Internal.Module.InstanceCount];
                internal static string Get(int contextInstance) => Names[contextInstance] ??= Create(contextInstance);
                private static string Create(int contextInstance)
                {
                    var instance = global::SpacetimeDB.Internal.Module.ResolveInstance(contextInstance, {{SymbolDisplay.FormatLiteral(
                assembly,
                true
            )}});
                    return Names[instance] ??= global::SpacetimeDB.Internal.Module.ResolveFunctionName(instance, nameof({{identifier}}), {{(
                string.IsNullOrEmpty(canonicalName)
                    ? "null"
                    : SymbolDisplay.FormatLiteral(canonicalName!, true)
            )}});
                }
                // Resolve only after the root installs namespace placements.
                static {{cache}}() { }
            }

            [System.Diagnostics.CodeAnalysis.Experimental("STDB_UNSTABLE")]
            public static void VolatileNonatomicScheduleImmediate{{name}}<{{contextType}}>({{contextType}} {{context}}{{string.Concat(
                args.Select(a => $", {a.Type.Name} {a.Identifier}")
            )}})
                where {{contextType}} : global::SpacetimeDB.Internal.IModuleContext
            {
                var {{resolvedName}} = {{cache}}.Get({{context}}.InstanceId);
                using var stream = new MemoryStream();
                using var writer = new BinaryWriter(stream);
                {{string.Join(
                "\n",
                args.Select(a => $"new {a.Type.ToBSATNString()}().Write(writer, {a.Identifier});")
            )}}
                {{dispatcher}}.VolatileNonatomicScheduleImmediate({{resolvedName}}, stream);
            }
            """;
    }

    private static string UniqueName(string name, EquatableArray<MemberDeclaration> args)
    {
        while (args.Select(arg => arg.Identifier.TrimStart('@')).Contains(name))
        {
            name += "_";
        }
        return name;
    }
}
