namespace SpacetimeDB;

using System;
using System.Collections.Generic;
using System.Collections.Immutable;
using System.Linq;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;

public static class Utils
{
    public static string SymbolToName(ISymbol symbol)
    {
        return symbol.ToDisplayString(
            SymbolDisplayFormat
                .FullyQualifiedFormat.WithMemberOptions(
                    SymbolDisplayMemberOptions.IncludeContainingType
                )
                .WithGenericsOptions(SymbolDisplayGenericsOptions.IncludeTypeParameters)
                .WithGlobalNamespaceStyle(SymbolDisplayGlobalNamespaceStyle.Omitted)
        );
    }

    public static void RegisterSourceOutputs(
        this IncrementalValuesProvider<KeyValuePair<string, string>> methods,
        IncrementalGeneratorInitializationContext context
    )
    {
        context.RegisterSourceOutput(
            methods,
            (context, method) =>
            {
                context.AddSource(
                    $"{string.Join("_", method.Key.Split(System.IO.Path.GetInvalidFileNameChars()))}.cs",
                    $@"
                    // <auto-generated />
                    #nullable enable

                    {method.Value}
                    "
                );
            }
        );
    }

    public static string MakeRwTypeParam(string typeParam) => typeParam + "RW";

    public static string GetTypeInfo(ITypeSymbol type)
    {
        // We need to distinguish handle nullable reference types specially:
        // compiler expands something like `int?` to `System.Nullable<int>` but with the nullable annotation set to `Annotated`
        // while something like `string?` is expanded to `string` with the nullable annotation set to `Annotated`...
        // Beautiful design requires beautiful hacks.
        if (
            type.NullableAnnotation == NullableAnnotation.Annotated
            && type.OriginalDefinition.ToString() != "System.Nullable<T>"
        )
        {
            // if we're here, then this is a nullable reference type like `string?`.
            type = type.WithNullableAnnotation(NullableAnnotation.None);
            return $"SpacetimeDB.BSATN.RefOption<{type}, {GetTypeInfo(type)}>";
        }
        return type switch
        {
            ITypeParameterSymbol typeParameter => MakeRwTypeParam(typeParameter.Name),
            INamedTypeSymbol namedType
                => type.SpecialType switch
                {
                    SpecialType.System_Boolean => "SpacetimeDB.BSATN.Bool",
                    SpecialType.System_SByte => "SpacetimeDB.BSATN.I8",
                    SpecialType.System_Byte => "SpacetimeDB.BSATN.U8",
                    SpecialType.System_Int16 => "SpacetimeDB.BSATN.I16",
                    SpecialType.System_UInt16 => "SpacetimeDB.BSATN.U16",
                    SpecialType.System_Int32 => "SpacetimeDB.BSATN.I32",
                    SpecialType.System_UInt32 => "SpacetimeDB.BSATN.U32",
                    SpecialType.System_Int64 => "SpacetimeDB.BSATN.I64",
                    SpecialType.System_UInt64 => "SpacetimeDB.BSATN.U64",
                    SpecialType.System_Single => "SpacetimeDB.BSATN.F32",
                    SpecialType.System_Double => "SpacetimeDB.BSATN.F64",
                    SpecialType.System_String => "SpacetimeDB.BSATN.String",
                    SpecialType.None => GetTypeInfoForNamedType(namedType),
                    _
                        => throw new InvalidOperationException(
                            $"Unsupported special type {type} ({type.SpecialType})"
                        )
                },
            IArrayTypeSymbol { ElementType: var elementType }
                => elementType is INamedTypeSymbol namedType
                && namedType.SpecialType == SpecialType.System_Byte
                    ? "SpacetimeDB.BSATN.ByteArray"
                    : $"SpacetimeDB.BSATN.Array<{elementType}, {GetTypeInfo(elementType)}>",
            _ => throw new InvalidOperationException($"Unsupported type {type}")
        };

        static string GetTypeInfoForNamedType(INamedTypeSymbol type)
        {
            if (type.TypeKind == TypeKind.Error)
            {
                throw new InvalidOperationException($"Could not resolve type {type}");
            }
            if (type.TypeKind == TypeKind.Enum)
            {
                if (
                    !type.GetAttributes()
                        .Any(a =>
                            a.AttributeClass?.ToDisplayString() == "SpacetimeDB.TypeAttribute"
                        )
                )
                {
                    throw new InvalidOperationException(
                        $"Enum {type} does not have a [SpacetimeDB.Type] attribute"
                    );
                }
                return $"SpacetimeDB.BSATN.Enum<{SymbolToName(type)}>";
            }
            var result = type.OriginalDefinition.ToString() switch
            {
                // (U)Int128 are not treated by C# as regular primitives, so we need to match them by type name.
                "System.Int128" => "SpacetimeDB.BSATN.I128",
                "System.UInt128" => "SpacetimeDB.BSATN.U128",
                "System.Collections.Generic.List<T>" => $"SpacetimeDB.BSATN.List",
                "System.Collections.Generic.Dictionary<TKey, TValue>"
                    => $"SpacetimeDB.BSATN.Dictionary",
                // If we're here, then this is nullable *value* type like `int?`.
                "System.Nullable<T>" => $"SpacetimeDB.BSATN.ValueOption",
                var name when name.StartsWith("System.")
                    => throw new InvalidOperationException($"Unsupported system type {name}"),
                _ => $"{SymbolToName(type)}.BSATN"
            };
            if (type.IsGenericType)
            {
                result +=
                    $"<{string.Join(", ", type.TypeArguments.Select(SymbolToName).Concat(type.TypeArguments.Select(GetTypeInfo)))}>";
            }
            return result;
        }
    }

    // Borrowed & modified code for generating in-place extensions for partial structs/classes/etc. Source:
    // https://andrewlock.net/creating-a-source-generator-part-5-finding-a-type-declarations-namespace-and-type-hierarchy/

    public readonly record struct Scope
    {
        // Reversed list of typescopes, from innermost to outermost.
        private readonly ImmutableArray<TypeScope> typeScopes;

        // Reversed list of namespaces, from innermost to outermost.
        private readonly ImmutableArray<string> namespaces;

        public Scope(MemberDeclarationSyntax? node)
        {
            var typeScopes_ = ImmutableArray.CreateBuilder<TypeScope>();
            // Keep looping while we're in a supported nested type
            while (node is TypeDeclarationSyntax type)
            {
                // Record the parent type keyword (class/struct etc), name, and constraints
                typeScopes_.Add(
                    new TypeScope(
                        Keyword: type.Keyword.ValueText,
                        Name: type.Identifier.ToString() + type.TypeParameterList,
                        Constraints: type.ConstraintClauses.ToString()
                    )
                ); // set the child link (null initially)

                // Move to the next outer type
                node = type.Parent as MemberDeclarationSyntax;
            }
            typeScopes = typeScopes_.ToImmutable();

            // We've now reached the outermost type, so we can determine the namespace
            var namespaces_ = ImmutableArray.CreateBuilder<string>();
            while (node is BaseNamespaceDeclarationSyntax ns)
            {
                namespaces_.Add(ns.Name.ToString());
                node = node.Parent as MemberDeclarationSyntax;
            }
            namespaces = namespaces_.ToImmutable();
        }

        public readonly record struct TypeScope(string Keyword, string Name, string Constraints);

        public string GenerateExtensions(string contents, string? interface_ = null)
        {
            var sb = new StringBuilder();

            // Join all namespaces into a single namespace statement, starting with the outermost.
            if (namespaces.Length > 0)
            {
                sb.Append("namespace ");
                var first = true;
                foreach (var ns in namespaces.Reverse())
                {
                    if (!first)
                    {
                        sb.Append('.');
                    }
                    first = false;
                    sb.Append(ns);
                }
                sb.AppendLine(" {");
            }

            // Loop through the full parent type hiearchy, starting with the outermost.
            foreach (var (i, typeScope) in typeScopes.Select((ts, i) => (i, ts)).Reverse())
            {
                sb.Append("partial ")
                    .Append(typeScope.Keyword) // e.g. class/struct/record
                    .Append(' ')
                    .Append(typeScope.Name) // e.g. Outer/Generic<T>
                    .Append(' ');

                if (i == 0 && interface_ is not null)
                {
                    sb.Append(" : ").Append(interface_);
                }

                sb.Append(typeScope.Constraints).AppendLine(" {");
            }

            sb.AppendLine();
            sb.Append(contents);
            sb.AppendLine();

            // We need to "close" each of the parent types, so write
            // the required number of '}'
            foreach (var typeScope in typeScopes)
            {
                sb.Append("} // ").AppendLine(typeScope.Name);
            }

            // Close the namespace, if we had one
            if (namespaces.Length > 0)
            {
                sb.AppendLine("} // namespace");
            }

            return sb.ToString();
        }
    }
}
