namespace SpacetimeDB.Codegen;

using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static Utils;

internal static class ClientSchemaMembers
{
    private static readonly SymbolDisplayFormat TypeFormat =
        SymbolDisplayFormat.FullyQualifiedFormat.AddMiscellaneousOptions(
            SymbolDisplayMiscellaneousOptions.IncludeNullableReferenceTypeModifier
        );

    internal static string? ForType(
        TypeDeclarationSyntax syntax,
        INamedTypeSymbol type,
        DiagReporter diag
    )
    {
        // [Type] already triggers BSATN generation. In a client schema [Table] alone
        // also describes a serializable row, just as it does in a server module.
        var hasTypeAttribute = type.GetAttributes()
            .Any(a => a.AttributeClass?.ToDisplayString() == "SpacetimeDB.TypeAttribute");
        var extensions = hasTypeAttribute
            ? new Scope.Extensions(new Scope(syntax), SymbolToName(type))
            : new TypeDeclaration(syntax, type, diag).ToExtensions();
        if (!hasTypeAttribute)
        {
            // Old generated row classes exposed [Type] to consumers inspecting their serializers.
            extensions.ExtraAttrs.Add("[global::SpacetimeDB.Type]");
        }

        if (type.TypeKind == Microsoft.CodeAnalysis.TypeKind.Class && !type.IsRecord)
        {
            var fields = SpacetimeDbFieldDiscovery
                .GetFieldsDeclaredInAnnotatedPartial(syntax, type)
                .ToArray();
            if (fields.Length != 0)
            {
                extensions.Contents.AppendLine();
                AddConstructors(extensions, type, fields);
            }
        }

        return extensions.Contents.Length > 0 ? extensions.ToString() : null;
    }

    private static void AddConstructors(
        Scope.Extensions extensions,
        INamedTypeSymbol type,
        IFieldSymbol[] fields
    )
    {
        var constructors = type.InstanceConstructors.Where(c => !c.IsImplicitlyDeclared);
        var name = EscapeIdentifier(type.Name);
        if (
            !constructors.Any(c =>
                c.Parameters.Length == fields.Length
                && c.Parameters.Select(p => p.Type)
                    .SequenceEqual(fields.Select(f => f.Type), SymbolEqualityComparer.Default)
            )
        )
        {
            var parameters = fields.Select(f =>
                $"{f.Type.ToDisplayString(TypeFormat)} {EscapeIdentifier(f.Name)}"
            );
            extensions.Contents.AppendLine($"public {name}({string.Join(", ", parameters)}) {{");
            foreach (var field in fields)
            {
                var identifier = EscapeIdentifier(field.Name);
                extensions.Contents.AppendLine($"    this.{identifier} = {identifier};");
            }
            extensions.Contents.AppendLine("}");
        }
        if (!constructors.Any(c => c.Parameters.Length == 0))
        {
            extensions.Contents.AppendLine($"public {name}() {{");
            foreach (var field in fields)
            {
                if (
                    !field.DeclaringSyntaxReferences.Any(r =>
                        r.GetSyntax() is VariableDeclaratorSyntax { Initializer: not null }
                    ) && DefaultValue(field.Type) is { } value
                )
                {
                    extensions.Contents.AppendLine(
                        $"    this.{EscapeIdentifier(field.Name)} = {value};"
                    );
                }
            }
            extensions.Contents.AppendLine("}");
        }
    }

    internal static string? DefaultValue(ITypeSymbol type)
    {
        if (type.IsValueType || type.NullableAnnotation == NullableAnnotation.Annotated)
        {
            return null;
        }
        if (type.SpecialType == SpecialType.System_String)
        {
            return "string.Empty";
        }
        if (type is IArrayTypeSymbol array)
        {
            return $"global::System.Array.Empty<{array.ElementType.ToDisplayString(TypeFormat)}>()";
        }
        if (type.BaseType?.OriginalDefinition.ToString() == "SpacetimeDB.TaggedEnum<Variants>")
        {
            return "null!";
        }
        return "new()";
    }

    internal static string? ForMethod(MethodDeclarationSyntax syntax, IMethodSymbol method)
    {
        if (
            syntax.Body != null
            || syntax.ExpressionBody != null
            || !syntax.Modifiers.Any(SyntaxKind.PartialKeyword)
            || method.PartialImplementationPart != null
        )
        {
            return null;
        }

        var extensions = new Scope.Extensions(
            new Scope((TypeDeclarationSyntax)syntax.Parent!),
            SymbolToName(method)
        );
        var parameters = method.Parameters.Select(p =>
            $"{p.Type.ToDisplayString(TypeFormat)} {EscapeIdentifier(p.Name)}"
        );
        extensions.Contents.AppendLine(
            $"{syntax.Modifiers} {method.ReturnType.ToDisplayString(TypeFormat)} "
                + $"{EscapeIdentifier(method.Name)}({string.Join(", ", parameters)}) => "
                + "throw new global::System.NotSupportedException(\"This method describes a server function. Use the connection to call it.\");"
        );
        return extensions.ToString();
    }
}
