namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using static Utils;

internal record NamespaceDeclaration(string AssemblyIdentity, string Accessor, string? Name)
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
        {
            return new EquatableArray<NamespaceDeclaration>(result.ToImmutable());
        }

        var identities = new Dictionary<string, AttributeData>(StringComparer.Ordinal);
        var accessors = new Dictionary<string, AttributeData>(StringComparer.OrdinalIgnoreCase);
        var tables = new HashSet<string>(tableAccessors, StringComparer.Ordinal);
        var assemblyIdentities = new HashSet<string>(
            assemblies.Select(static assembly => assembly.Identity),
            StringComparer.Ordinal
        );
        var supported = compilation.SyntaxTrees.Any(static tree =>
            tree.Options is CSharpParseOptions options
            && options.PreprocessorSymbolNames.Contains("NET10_0_OR_GREATER")
            // Use the numeric value to keep the analyzer compatible with Roslyn 4.3.
            && (int)options.LanguageVersion >= 1400
        );

        foreach (var attribute in compilation.Assembly.GetAttributes())
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (!SymbolEqualityComparer.Default.Equals(attribute.AttributeClass, attributeType))
            {
                continue;
            }

            var valid = true;
            if (!supported)
            {
                ReportError(
                    diag,
                    attribute,
                    ref valid,
                    "Namespace mounts require .NET 10 and C# 14."
                );
            }

            if (
                attribute.ConstructorArguments.Length != 1
                || attribute.ConstructorArguments[0].Value is not INamedTypeSymbol marker
            )
            {
                ReportError(
                    diag,
                    attribute,
                    ref valid,
                    "A namespace mount requires a marker type declared in a module assembly."
                );
                continue;
            }

            var identity = marker.ContainingAssembly.Identity.ToString();
            if (
                SymbolEqualityComparer.Default.Equals(
                    marker.ContainingAssembly,
                    compilation.Assembly
                )
            )
            {
                ReportError(diag, attribute, ref valid, "The root assembly cannot mount itself.");
            }
            else if (supported && !assemblyIdentities.Contains(identity))
            {
                ReportError(
                    diag,
                    attribute,
                    ref valid,
                    $"Assembly '{identity}' has no discovered module descriptor."
                );
            }

            var accessor =
                attribute.NamedArguments.FirstOrDefault(static a => a.Key == "Accessor").Value.Value
                    as string
                ?? "";
            var name =
                attribute.NamedArguments.FirstOrDefault(static a => a.Key == "Name").Value.Value
                as string;
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
            {
                ReportError(
                    diag,
                    attribute,
                    ref valid,
                    "Accessor must be a nonempty C# identifier (use keyword names without '@')."
                );
            }

            ValidateDatabaseIdentifier(diag, attribute, ref valid, accessor, "Accessor");
            if (name is not null)
            {
                ValidateDatabaseIdentifier(diag, attribute, ref valid, name, "Name");
                if (
                    accessor.Equals("public", StringComparison.OrdinalIgnoreCase)
                    != name.Equals("public", StringComparison.OrdinalIgnoreCase)
                )
                {
                    ReportError(
                        diag,
                        attribute,
                        ref valid,
                        "The public scope cannot be renamed or targeted by a different accessor."
                    );
                }
            }

            CheckDuplicate(
                diag,
                attribute,
                ref valid,
                identities,
                identity,
                $"Assembly '{identity}' may only be mounted once."
            );
            if (accessor.Length > 0)
            {
                CheckDuplicate(
                    diag,
                    attribute,
                    ref valid,
                    accessors,
                    accessor,
                    $"Namespace accessor '{accessor}' is declared more than once (case-insensitive)."
                );
            }

            if (tables.Contains(accessor))
            {
                ReportError(
                    diag,
                    attribute,
                    ref valid,
                    $"Namespace accessor '{accessor}' conflicts with a root table accessor."
                );
            }

            if (accessor is "GetType" or "ToString" or "Equals" or "GetHashCode")
            {
                ReportError(
                    diag,
                    attribute,
                    ref valid,
                    $"Namespace accessor '{accessor}' conflicts with an existing context database/query receiver member."
                );
            }

            if (valid)
            {
                result.Add(new NamespaceDeclaration(identity, accessor, name));
            }
        }
        return new EquatableArray<NamespaceDeclaration>(result.ToImmutable());
    }

    private static void ValidateDatabaseIdentifier(
        DiagReporter diag,
        AttributeData attribute,
        ref bool valid,
        string value,
        string property
    )
    {
        if (
            value.Length == 0
            || !(char.IsLetter(value[0]) || value[0] == '_')
            || value.Any(static c => !(char.IsLetterOrDigit(c) || c == '_'))
            || !value.IsNormalized(NormalizationForm.FormC)
        )
        {
            ReportError(
                diag,
                attribute,
                ref valid,
                $"{property} must be a nonempty database identifier: letters, digits or underscores, starting with a letter or underscore."
            );
        }
        if (Encoding.UTF8.GetByteCount(value) > 63)
        {
            ReportError(
                diag,
                attribute,
                ref valid,
                $"{property} cannot exceed 63 UTF-8 bytes (the current host limit)."
            );
        }
        if (
            value.Equals("st", StringComparison.OrdinalIgnoreCase)
            || value.Equals("spacetimedb", StringComparison.OrdinalIgnoreCase)
            || value.StartsWith("pg_", StringComparison.OrdinalIgnoreCase)
        )
        {
            ReportError(diag, attribute, ref valid, $"Namespace {property} '{value}' is reserved.");
        }
    }

    private static void ReportError(
        DiagReporter diag,
        AttributeData attribute,
        ref bool valid,
        string message
    )
    {
        valid = false;
        diag.Report(ErrorDescriptor.InvalidNamespace, (attribute, message));
    }

    private static void CheckDuplicate(
        DiagReporter diag,
        AttributeData attribute,
        ref bool valid,
        Dictionary<string, AttributeData> seen,
        string key,
        string message
    )
    {
        if (seen.TryGetValue(key, out var previous))
        {
            ReportError(diag, attribute, ref valid, message);
            diag.Report(ErrorDescriptor.InvalidNamespace, (previous, message));
        }
        else
        {
            seen.Add(key, attribute);
        }
    }
}
