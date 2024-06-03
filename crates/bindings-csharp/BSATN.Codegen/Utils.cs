namespace SpacetimeDB;

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
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

    public class Scope(TypeDeclarationSyntax type)
    {
        private string nameSpace = GetNamespace(type);
        private ParentClass? parentClasses = GetParentClasses(type);

        // determine the namespace the class/enum/struct is declared in, if any
        static string GetNamespace(BaseTypeDeclarationSyntax syntax)
        {
            // If we don't have a namespace at all we'll return an empty string
            // This accounts for the "default namespace" case
            string nameSpace = string.Empty;

            // Get the containing syntax node for the type declaration
            // (could be a nested type, for example)
            SyntaxNode? potentialNamespaceParent = syntax.Parent;

            // Keep moving "out" of nested classes etc until we get to a namespace
            // or until we run out of parents
            while (
                potentialNamespaceParent != null
                && potentialNamespaceParent is not NamespaceDeclarationSyntax
                && potentialNamespaceParent is not FileScopedNamespaceDeclarationSyntax
            )
            {
                potentialNamespaceParent = potentialNamespaceParent.Parent;
            }

            // Build up the final namespace by looping until we no longer have a namespace declaration
            if (potentialNamespaceParent is BaseNamespaceDeclarationSyntax namespaceParent)
            {
                // We have a namespace. Use that as the type
                nameSpace = namespaceParent.Name.ToString();

                // Keep moving "out" of the namespace declarations until we
                // run out of nested namespace declarations
                while (true)
                {
                    if (namespaceParent.Parent is not NamespaceDeclarationSyntax parent)
                    {
                        break;
                    }

                    // Add the outer namespace as a prefix to the final namespace
                    nameSpace = $"{namespaceParent.Name}.{nameSpace}";
                    namespaceParent = parent;
                }
            }

            // return the final namespace
            return nameSpace;
        }

        public class ParentClass(
            string keyword,
            string name,
            string constraints,
            Scope.ParentClass? child
        )
        {
            public readonly ParentClass? Child = child;
            public readonly string Keyword = keyword;
            public readonly string Name = name;
            public readonly string Constraints = constraints;
        }

        static ParentClass? GetParentClasses(TypeDeclarationSyntax typeSyntax)
        {
            // Try and get the parent syntax. If it isn't a type like class/struct, this will be null
            TypeDeclarationSyntax? parentSyntax = typeSyntax;
            ParentClass? parentClassInfo = null;

            // Keep looping while we're in a supported nested type
            while (parentSyntax != null && IsAllowedKind(parentSyntax.Kind()))
            {
                // Record the parent type keyword (class/struct etc), name, and constraints
                parentClassInfo = new ParentClass(
                    keyword: parentSyntax.Keyword.ValueText,
                    name: parentSyntax.Identifier.ToString() + parentSyntax.TypeParameterList,
                    constraints: parentSyntax.ConstraintClauses.ToString(),
                    child: parentClassInfo
                ); // set the child link (null initially)

                // Move to the next outer type
                parentSyntax = (parentSyntax.Parent as TypeDeclarationSyntax);
            }

            // return a link to the outermost parent type
            return parentClassInfo;
        }

        // We can only be nested in class/struct/record
        static bool IsAllowedKind(SyntaxKind kind) =>
            kind == SyntaxKind.ClassDeclaration
            || kind == SyntaxKind.StructDeclaration
            || kind == SyntaxKind.RecordDeclaration;

        public string GenerateExtensions(string contents, string? interface_ = null)
        {
            var sb = new StringBuilder();

            // If we don't have a namespace, generate the code in the "default"
            // namespace, either global:: or a different <RootNamespace>
            var hasNamespace = !string.IsNullOrEmpty(nameSpace);
            if (hasNamespace)
            {
                // We could use a file-scoped namespace here which would be a little impler,
                // but that requires C# 10, which might not be available.
                // Depends what you want to support!
                sb.Append("namespace ")
                    .Append(nameSpace)
                    .AppendLine(
                        @"
        {"
                    );
            }

            // Loop through the full parent type hiearchy, starting with the outermost
            var parentsCount = 0;
            while (parentClasses is not null)
            {
                sb.Append("    partial ")
                    .Append(parentClasses.Keyword) // e.g. class/struct/record
                    .Append(' ')
                    .Append(parentClasses.Name) // e.g. Outer/Generic<T>
                    .Append(' ');

                if (parentClasses.Child is null && interface_ is not null)
                {
                    sb.Append(" : ").Append(interface_);
                }

                sb.Append(parentClasses.Constraints) // e.g. where T: new()
                    .AppendLine(
                        @"
            {"
                    );
                parentsCount++; // keep track of how many layers deep we are
                parentClasses = parentClasses.Child; // repeat with the next child
            }

            // Write the actual target generation code here. Not shown for brevity
            sb.AppendLine(contents);

            // We need to "close" each of the parent types, so write
            // the required number of '}'
            for (int i = 0; i < parentsCount; i++)
            {
                sb.AppendLine(@"    }");
            }

            // Close the namespace, if we had one
            if (hasNamespace)
            {
                sb.Append('}').AppendLine();
            }

            return sb.ToString();
        }
    }
}
