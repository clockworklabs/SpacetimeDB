namespace SpacetimeDB.Codegen;

using System;
using System.Collections.Generic;
using System.Collections.Immutable;
using System.Linq;
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
                        .Where(f =>
                            !f.Modifiers.Any(m =>
                                m.IsKind(SyntaxKind.StaticKeyword)
                                || m.IsKind(SyntaxKind.ConstKeyword)
                            )
                        )
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
                        fields = taggedEnumVariants.Value.Select(v => new VariableDeclaration
                        {
                            Name = v.Name,
                            TypeSymbol = v.Type,
                        });
                    }

                    if (type.TypeParameterList is not null)
                    {
                        throw new InvalidOperationException(
                            "Types with type parameters are not yet supported."
                        );
                    }

                    return new
                    {
                        Scope = new Scope(type),
                        ShortName = type.Identifier.Text,
                        FullName = SymbolToName(context.SemanticModel.GetDeclaredSymbol(type, ct)!),
                        IsTaggedEnum = taggedEnumVariants is not null,
                        Members = fields.ToArray(),
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

                    var bsatnDecls = type.Members.Select(m =>
                        (m.Name, TypeInfo: GetTypeInfo(m.TypeSymbol))
                    )
                        .ToList();

                    var fieldNames = bsatnDecls.Select(m => m.Name).ToArray();

                    if (type.IsTaggedEnum)
                    {
                        typeDesc +=
                            $@"
                            private {type.ShortName}() {{ }}

                            enum @enum: byte
                            {{
                                {string.Join(",\n", type.Members.Select(m => m.Name))}
                            }}
                        ";

                        bsatnDecls.Insert(
                            0,
                            (Name: "@enum", TypeInfo: "SpacetimeDB.BSATN.Enum<@enum>")
                        );

                        typeDesc += string.Join(
                            "\n",
                            type.Members.Select(m =>
                                $@"public sealed record {m.Name}({m.TypeSymbol} Value) : {type.ShortName};"
                            )
                        );

                        read =
                            $@"@enum.Read() switch {{
                                {string.Join(",\n", fieldNames.Select(name => $"@enum.{name} => new {name}({name}.Read(reader))"))}
                            }}";

                        write =
                            $@"switch (value) {{
                                {string.Join("\n", fieldNames.Select(name => $@"
                                    case {name}(var inner):
                                        @enum.Write(writer, @enum.{name});
                                        {name}.Write(writer, inner);
                                        break;
                                "))}
                            }}";
                    }
                    else
                    {
                        typeDesc +=
                            $@"
                            public void ReadFields(System.IO.BinaryReader reader) {{
                                {string.Join("\n", fieldNames.Select(name => $"{name} = BSATN.{name}.Read(reader);"))}
                            }}

                            public void WriteFields(System.IO.BinaryWriter writer) {{
                                {string.Join("\n", fieldNames.Select(name => $"BSATN.{name}.Write(writer, {name});"))}
                            }}
                        ";

                        read =
                            $"SpacetimeDB.BSATN.IStructuralReadWrite.Read<{type.ShortName}>(reader)";

                        write = "value.WriteFields(writer);";
                    }

                    typeDesc +=
                        $@"
                        public readonly struct BSATN : SpacetimeDB.BSATN.IReadWrite<{type.ShortName}>
                        {{
                            {string.Join("\n", bsatnDecls.Select(decl => $"internal static readonly {decl.TypeInfo} {decl.Name} = new();"))}

                            public {type.ShortName} Read(System.IO.BinaryReader reader) => {read};

                            public void Write(System.IO.BinaryWriter writer, {type.ShortName} value) {{
                                {write}
                            }}
                        }}
                    ";

                    return new KeyValuePair<string, string>(
                        type.FullName,
                        type.Scope.GenerateExtensions(
                            typeDesc,
                            type.IsTaggedEnum ? null : "SpacetimeDB.BSATN.IStructuralReadWrite"
                        )
                    );
                }
            )
            .RegisterSourceOutputs(context);
    }
}
