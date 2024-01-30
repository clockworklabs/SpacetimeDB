namespace SpacetimeDB.Codegen;

using System;
using System.Collections.Generic;
using System.Collections.Immutable;
using System.Globalization;
using System.Linq;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static Utils;

struct VariableDeclaration
{
    public string Name;
    public ITypeSymbol TypeSymbol;
}

[Generator]
public class Type : IIncrementalGenerator
{
    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        WithAttrAndPredicate(
            context,
            "SpacetimeDB.TypeAttribute",
            (node) =>
            {
                // structs and classes should be always processed
                if (node is not EnumDeclarationSyntax enumType)
                    return true;

                // Ensure variants are contiguous as SATS enums don't support explicit tags.
                if (enumType.Members.Any(m => m.EqualsValue is not null))
                {
                    throw new InvalidOperationException(
                        "[SpacetimeDB.Type] enums cannot have explicit values: "
                            + enumType.Identifier
                    );
                }

                // Ensure all enums fit in `byte` as that's what SATS uses for tags.
                if (enumType.Members.Count > 256)
                {
                    throw new InvalidOperationException(
                        "[SpacetimeDB.Type] enums cannot have more than 256 variants."
                    );
                }

                // Check that enums are compatible with SATS but otherwise skip from extra processing.
                return false;
            }
        );

        // Any table should be treated as a type without an explicit [Type] attribute.
        WithAttrAndPredicate(context, "SpacetimeDB.TableAttribute", (_node) => true);
    }

    public void WithAttrAndPredicate(
        IncrementalGeneratorInitializationContext context,
        string fullyQualifiedMetadataName,
        Func<SyntaxNode, bool> predicate
    )
    {
        context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName,
                predicate: (node, ct) => predicate(node),
                transform: (context, ct) =>
                {
                    var type = (TypeDeclarationSyntax)context.TargetNode;

                    // Check if type implements generic `SpacetimeDB.TaggedEnum<Variants>` and, if so, extract the `Variants` type.
                    var taggedEnumVariants = type.BaseList?.Types
                        .OfType<SimpleBaseTypeSyntax>()
                        .Select(t => context.SemanticModel.GetTypeInfo(t.Type, ct).Type)
                        .OfType<INamedTypeSymbol>()
                        .Where(t =>
                            t.OriginalDefinition.ToString() == "SpacetimeDB.TaggedEnum<Variants>"
                        )
                        .Select(t =>
                            (ImmutableArray<IFieldSymbol>?)
                                ((INamedTypeSymbol)t.TypeArguments[0]).TupleElements
                        )
                        .FirstOrDefault();

                    var fields = type.Members.OfType<FieldDeclarationSyntax>()
                        .Where(f => !f.Modifiers.Any(m => m.IsKind(SyntaxKind.StaticKeyword)))
                        .SelectMany(f =>
                        {
                            var typeSymbol = context
                                .SemanticModel.GetTypeInfo(f.Declaration.Type, ct)
                                .Type!;
                            // Seems like a bug in Roslyn - nullability annotation is not set on the top type.
                            // Set it manually for now. TODO: report upstream.
                            if (f.Declaration.Type is NullableTypeSyntax)
                            {
                                typeSymbol = typeSymbol.WithNullableAnnotation(
                                    NullableAnnotation.Annotated
                                );
                            }
                            return f.Declaration.Variables.Select(v => new VariableDeclaration
                            {
                                Name = v.Identifier.Text,
                                TypeSymbol = typeSymbol,
                            });
                        });

                    if (taggedEnumVariants is not null)
                    {
                        if (fields.Any())
                        {
                            throw new InvalidOperationException("Tagged enums cannot have fields.");
                        }
                        fields = taggedEnumVariants
                            .Value.Select(v => new VariableDeclaration
                            {
                                Name = v.Name,
                                TypeSymbol = v.Type,
                            })
                            .ToArray();
                    }

                    return new
                    {
                        Scope = new Scope(type),
                        ShortName = type.Identifier.Text,
                        FullName = SymbolToName(context.SemanticModel.GetDeclaredSymbol(type, ct)!),
                        GenericName = $"{type.Identifier}{type.TypeParameterList}",
                        IsTaggedEnum = taggedEnumVariants is not null,
                        TypeParams = type.TypeParameterList?.Parameters
                            .Select(p => p.Identifier.Text)
                            .ToArray() ?? new string[] { },
                        Members = fields,
                    };
                }
            )
            .Select(
                (type, ct) =>
                {
                    var typeKind = type.IsTaggedEnum ? "Sum" : "Product";

                    string read,
                        write;

                    var typeDesc = "";

                    var fieldIO = type.Members.Select(m =>
                    {
                        var typeInfo = GetTypeInfo(m.TypeSymbol);

                        return new { m.Name, TypeInfo = typeInfo, };
                    });

                    if (type.IsTaggedEnum)
                    {
                        typeDesc +=
                            $@"
                            private {type.ShortName}() {{ }}

                            enum __Tag: byte
                            {{
                                {string.Join(", ", type.Members.Select(m => m.Name))}
                            }}
                        ";

                        typeDesc += string.Join(
                            "\n",
                            type.Members.Select(m =>
                            {
                                var name = m.Name;
                                var fieldType = m.TypeSymbol.ToDisplayString();

                                return $@"public sealed record {name}({fieldType} Value) : {type.ShortName};";
                            })
                        );

                        read =
                            $@"(__Tag)reader.ReadByte() switch {{
                                {string.Join("\n", fieldIO.Select(m => $"__Tag.{m.Name} => new {m.Name}({m.TypeInfo}.Read(reader)),"))}
                                var tag => throw new System.InvalidOperationException($""Unsupported tag {{tag}}"")
                            }}";

                        write =
                            $@"switch (value) {{
                                {string.Join("\n", fieldIO.Select(m => $@"
                                    case {m.Name}(var inner):
                                        writer.Write((byte)__Tag.{m.Name});
                                        {m.TypeInfo}.Write(writer, inner);
                                        break;
                                "))}
                            }}";
                    }
                    else
                    {
                        read =
                            $@"new {type.GenericName} {{
                                {string.Join(",\n", fieldIO.Select(m => $"{m.Name} = {m.TypeInfo}.Read(reader)"))}
                            }}";

                        write = string.Join(
                            "\n",
                            fieldIO.Select(m => $"{m.TypeInfo}.Write(writer, value.{m.Name});")
                        );
                    }

                    typeDesc +=
                        $@"
                        public static {type.GenericName} Read(BinaryReader reader) => {read};

                        public static void Write(BinaryWriter writer, {type.GenericName} value) {{
                            {write}
                        }}
                    ";

                    return new KeyValuePair<string, string>(
                        type.FullName,
                        type.Scope.GenerateExtensions(
                            typeDesc,
                            $"SpacetimeDB.BSATN.IReadWrite<{type.GenericName}>"
                        )
                    );
                }
            )
            .RegisterSourceOutputs(context);
    }
}
