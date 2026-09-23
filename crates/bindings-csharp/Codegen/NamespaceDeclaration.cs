namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using static Utils;

record NamespaceDeclaration(string AssemblyIdentity, string Accessor)
{
    public string AccessorIdentifier => EscapeIdentifier(Accessor);

    public static EquatableArray<NamespaceDeclaration> Parse(
        Compilation compilation,
        EquatableArray<AssemblyDeclaration> assemblies,
        IEnumerable<string> tableAccessors,
        DiagReporter diag,
        CancellationToken cancellationToken
    )
    {
        var attributeType = compilation.GetTypeByMetadataName("SpacetimeDB.NamespaceAttribute");
        var result = ImmutableArray.CreateBuilder<NamespaceDeclaration>();
        if (attributeType is null)
            return new(result.ToImmutable());
        var identities = new Dictionary<string, AttributeData>(StringComparer.Ordinal);
        var accessors = new Dictionary<string, AttributeData>(StringComparer.OrdinalIgnoreCase);
        var tables = new HashSet<string>(tableAccessors, StringComparer.Ordinal);
        var supported = compilation.SyntaxTrees.Any(tree =>
            tree.Options is CSharpParseOptions options
            && options.PreprocessorSymbolNames.Contains("NET10_0_OR_GREATER")
            // Use the numeric value to keep the analyzer compatible with Roslyn 4.3.
            && (int)options.LanguageVersion >= 1400
        );

        foreach (var attribute in compilation.Assembly.GetAttributes())
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (!SymbolEqualityComparer.Default.Equals(attribute.AttributeClass, attributeType))
                continue;

            var valid = true;
            void Error(string message)
            {
                valid = false;
                diag.Report(ErrorDescriptor.InvalidNamespace, (attribute, message));
            }

            if (!supported)
                Error("Namespace mounts require .NET 10 and C# 14.");

            if (
                attribute.ConstructorArguments.Length != 1
                || attribute.ConstructorArguments[0].Value is not INamedTypeSymbol marker
            )
            {
                Error("A namespace mount requires a marker type declared in a module assembly.");
                continue;
            }

            var identity = marker.ContainingAssembly.Identity.ToString();
            if (
                SymbolEqualityComparer.Default.Equals(
                    marker.ContainingAssembly,
                    compilation.Assembly
                )
            )
                Error("The root assembly cannot mount itself.");
            else if (supported && !assemblies.Any(a => a.Identity == identity))
                Error($"Assembly '{identity}' has no discovered module descriptor.");

            var accessor =
                attribute.NamedArguments.FirstOrDefault(a => a.Key == "Accessor").Value.Value
                    as string
                ?? "";
            // Keywords are stored unescaped and escaped only when rendering C#.
            if (
                accessor.Length == 0
                || accessor[0] == '@'
                || !(
                    SyntaxFacts.IsValidIdentifier(accessor)
                    || SyntaxFacts.GetKeywordKind(accessor) != SyntaxKind.None
                )
                || SyntaxFactory.ParseToken(EscapeIdentifier(accessor)).ValueText != accessor
            )
                Error("Accessor must be a nonempty C# identifier (use keyword names without '@').");

            // The accessor is also the database namespace, so both sets of rules apply.
            // The host remains authoritative for full Unicode identifier validation.
            if (
                accessor.Length == 0
                || !(char.IsLetter(accessor[0]) || accessor[0] == '_')
                || accessor.Any(c => !(char.IsLetterOrDigit(c) || c == '_'))
                || !accessor.IsNormalized(NormalizationForm.FormC)
            )
                Error(
                    "Accessor must also be a nonempty database identifier: letters, digits or underscores, starting with a letter or underscore."
                );
            if (Encoding.UTF8.GetByteCount(accessor) > 63)
                Error("Namespace names cannot exceed 63 UTF-8 bytes (the current host limit).");
            if (
                accessor.Equals("st", StringComparison.OrdinalIgnoreCase)
                || accessor.Equals("spacetimedb", StringComparison.OrdinalIgnoreCase)
                || accessor.StartsWith("pg_", StringComparison.OrdinalIgnoreCase)
            )
                Error($"Namespace '{accessor}' is reserved.");

            void CheckDuplicate(Dictionary<string, AttributeData> seen, string key, string message)
            {
                if (seen.TryGetValue(key, out var previous))
                {
                    Error(message);
                    diag.Report(ErrorDescriptor.InvalidNamespace, (previous, message));
                }
                else
                    seen.Add(key, attribute);
            }

            CheckDuplicate(
                identities,
                identity,
                $"Assembly '{identity}' may only be mounted once."
            );
            if (accessor.Length > 0)
                CheckDuplicate(
                    accessors,
                    accessor,
                    $"Namespace accessor '{accessor}' is declared more than once (case-insensitive)."
                );
            if (tables.Contains(accessor))
                Error($"Namespace accessor '{accessor}' conflicts with a root table accessor.");
            if (accessor is "GetType" or "ToString" or "Equals" or "GetHashCode")
                Error($"Namespace accessor '{accessor}' conflicts with an existing context database/query receiver member.");

            if (valid)
                result.Add(new(identity, accessor));
        }
        return new(result.ToImmutable());
    }
}
