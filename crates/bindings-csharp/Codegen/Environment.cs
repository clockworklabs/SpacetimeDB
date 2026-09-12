namespace SpacetimeDB.Codegen;

using System;
using System.Collections.Generic;
using System.Collections.Immutable;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;

/// <summary>Compile-time declarations and read-only accessors, never live values.</summary>
[Generator]
public sealed class EnvironmentGenerator : IIncrementalGenerator
{
    private static readonly DiagnosticDescriptor InvalidDeclaration =
        new(
            "STDBENV001",
            "Invalid environment declaration",
            "{0}",
            "SpacetimeDB",
            DiagnosticSeverity.Error,
            isEnabledByDefault: true
        );

    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        var declarations = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                "SpacetimeDB.EnvAttribute",
                (_, _) => true,
                (ctx, _) => (INamedTypeSymbol)ctx.TargetSymbol
            )
            .Collect();
        context.RegisterSourceOutput(declarations, Generate);
    }

    private static void Generate(
        SourceProductionContext context,
        ImmutableArray<INamedTypeSymbol> types
    )
    {
        void Report(ISymbol symbol, string message) =>
            context.ReportDiagnostic(
                Diagnostic.Create(InvalidDeclaration, symbol.Locations.FirstOrDefault(), message)
            );
        if (types.Length > 1)
        {
            foreach (var type in types)
                Report(type, "A module may have only one [SpacetimeDB.Env] declaration struct.");
        }
        var fields = types
            .SelectMany(type => type.GetMembers().OfType<IFieldSymbol>())
            .Where(field => !field.IsImplicitlyDeclared)
            .ToArray();
        if (fields.Length > 256 && types.Length != 0)
            Report(types[0], "An environment schema may declare at most 256 variables.");

        var keys = new HashSet<string>(StringComparer.Ordinal);
        var properties = new List<string>();
        var registrations = new List<string>();
        foreach (var field in fields)
        {
            var name = field.Name;
            if (field.IsStatic || field.Type.SpecialType != SpecialType.System_String)
            {
                Report(
                    field,
                    "Environment fields must be instance string or nullable string declarations."
                );
                continue;
            }
            if (
                !Regex.IsMatch(name, "^[A-Za-z_][A-Za-z0-9_]*$")
                || Encoding.UTF8.GetByteCount(name) > 256
                || !keys.Add(name)
            )
            {
                Report(
                    field,
                    "Environment names must be unique POSIX identifiers of at most 256 UTF-8 bytes."
                );
                continue;
            }
            var optional = field.NullableAnnotation == NullableAnnotation.Annotated;
            var attr = field
                .GetAttributes()
                .FirstOrDefault(a =>
                    a.AttributeClass?.ToDisplayString() == "SpacetimeDB.EnvValuesAttribute"
                );
            var constraint =
                "new global::SpacetimeDB.Internal.EnvironmentConstraint.AnyString(default)";
            if (attr is not null)
            {
                var values = attr.ConstructorArguments.FirstOrDefault();
                if (
                    values.Kind != TypedConstantKind.Array
                    || values.IsNull
                    || values.Values.Length == 0
                    || values.Values.Any(value =>
                        value.Value is not string text || Encoding.UTF8.GetByteCount(text) > 8192
                    )
                )
                {
                    Report(
                        field,
                        "EnvValues requires a nonempty list of string literals of at most 8192 UTF-8 bytes each."
                    );
                    continue;
                }
                var strings = values.Values.Select(value => (string)value.Value!).ToArray();
                if (strings.Distinct(StringComparer.Ordinal).Count() != strings.Length)
                {
                    Report(field, "EnvValues must not repeat an allowed literal.");
                    continue;
                }
                constraint =
                    strings.Length == 1
                        ? $"new global::SpacetimeDB.Internal.EnvironmentConstraint.Literal({Literal(strings[0])})"
                        : $"new global::SpacetimeDB.Internal.EnvironmentConstraint.OneOf(new global::System.Collections.Generic.List<string> {{ {string.Join(", ", strings.Select(Literal))} }})";
            }
            registrations.Add(
                $"global::SpacetimeDB.Internal.Module.RegisterEnvironment(new({Literal(name)}, {constraint}, {(optional ? "true" : "false")}));"
            );
            // Preserve the checked generic method, including a key literally
            // named Get, and inherited object members. Keywords are escaped
            // without renaming stored keys.
            if (
                name
                is "Get"
                    or "ModuleEnvironment"
                    or "Equals"
                    or "GetHashCode"
                    or "ToString"
                    or "Finalize"
                    or "GetType"
                    or "MemberwiseClone"
            )
                continue;
            var read = $"Get({Literal(name)})";
            if (!optional)
                read +=
                    " ?? throw new global::System.InvalidOperationException(\"Required environment value is absent\")";
            properties.Add($"public string{(optional ? "?" : "")} @{name} => {read};");
        }
        context.AddSource(
            "Environment.g.cs",
            $$"""
            // <auto-generated />
            #nullable enable
            #pragma warning disable CS0436
            namespace SpacetimeDB {
                public readonly struct ModuleEnvironment {
                    public string? Get(string key) => default(global::SpacetimeDB.DatabaseEnvironment).Get(key);
                    {{string.Join("\n", properties)}}
                }
                internal static class EnvironmentRegistration {
                    [global::System.Runtime.CompilerServices.ModuleInitializer]
                    internal static void Register() {
                        {{string.Join("\n", registrations)}}
                    }
                }
            }
            """
        );
    }

    private static string Literal(string value) => SymbolDisplay.FormatLiteral(value, quote: true);
}
