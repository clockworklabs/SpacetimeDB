namespace SpacetimeDB.Codegen;

using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using System;
using System.Collections.Generic;
using System.Collections.Immutable;
using System.Globalization;
using System.Linq;
using System.Text;
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
                if (!(node is EnumDeclarationSyntax enumType))
                    return true;

                // Ensure variants are contiguous as SATS enums don't support explicit tags.
                if (enumType.Members.Any(m => m.EqualsValue is not null))
                {
                    throw new InvalidOperationException(
                        "[SpacetimeDB.Type] enums cannot have explicit values."
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
        context.SyntaxProvider
            .ForAttributeWithMetadataName(
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
                        .Where(
                            t =>
                                t.OriginalDefinition.ToString()
                                == "SpacetimeDB.TaggedEnum<Variants>"
                        )
                        .Select(
                            t =>
                                (ImmutableArray<IFieldSymbol>?)
                                    ((INamedTypeSymbol)t.TypeArguments[0]).TupleElements
                        )
                        .FirstOrDefault();

                    var fields = type.Members
                        .OfType<FieldDeclarationSyntax>()
                        .Where(f => !f.Modifiers.Any(m => m.IsKind(SyntaxKind.StaticKeyword)))
                        .SelectMany(f =>
                        {
                            var typeSymbol = context.SemanticModel
                                .GetTypeInfo(f.Declaration.Type, ct)
                                .Type!;
                            // Seems like a bug in Roslyn - nullability annotation is not set on the top type.
                            // Set it manually for now. TODO: report upstream.
                            if (f.Declaration.Type is NullableTypeSyntax)
                            {
                                typeSymbol = typeSymbol.WithNullableAnnotation(
                                    NullableAnnotation.Annotated
                                );
                            }
                            return f.Declaration.Variables.Select(
                                v =>
                                    new VariableDeclaration
                                    {
                                        Name = v.Identifier.Text,
                                        TypeSymbol = typeSymbol,
                                    }
                            );
                        });

                    if (taggedEnumVariants is not null)
                    {
                        if (fields.Any())
                        {
                            throw new InvalidOperationException("Tagged enums cannot have fields.");
                        }
                        fields = taggedEnumVariants.Value
                            .Select(
                                v => new VariableDeclaration { Name = v.Name, TypeSymbol = v.Type, }
                            )
                            .ToArray();
                    }

                    return new
                    {
                        Scope = new Scope(type),
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

                    var fieldIO = type.Members.Select(
                        m =>
                            new
                            {
                                m.Name,
                                Read = $"{m.Name} = fieldTypeInfo.{m.Name}.Read(reader),",
                                Write = $"fieldTypeInfo.{m.Name}.Write(writer, value.{m.Name});"
                            }
                    );

                    if (type.IsTaggedEnum)
                    {
                        typeDesc +=
                            $@"
                            public enum TagKind: byte
                            {{
                                {string.Join(", ", type.Members.Select(m => m.Name))}
                            }}

                            public TagKind Tag {{ get; private set; }}
                            private object? boxedValue;
                        ";

                        typeDesc += string.Join(
                            "\n",
                            type.Members.Select(m =>
                            {
                                var name = m.Name;
                                var type = m.TypeSymbol.ToDisplayString();

                                return $@"
                                public bool Is{name} => Tag == TagKind.{name};

                                public {type} {name} {{
                                    get => Is{name} ? ({type})boxedValue! : throw new System.InvalidOperationException($""Expected {name} but got {{Tag}}"");
                                    set {{
                                        Tag = TagKind.{name};
                                        boxedValue = value;
                                    }}
                                }}
                                ";
                            })
                        );

                        read =
                            $@"(TagKind)reader.ReadByte() switch {{
                                {string.Join("\n", fieldIO.Select(m => $"TagKind.{m.Name} => new {type.GenericName} {{ {m.Read} }},"))}
                                var tag => throw new System.InvalidOperationException($""Unknown tag {{tag}}"")
                            }}";

                        write =
                            $@"writer.Write((byte)value.Tag);
                            switch (value.Tag) {{
                                {string.Join("\n", fieldIO.Select(m => $@"
                                    case TagKind.{m.Name}:
                                        {m.Write};
                                        break;
                                "))}
                                default:
                                    throw new System.InvalidOperationException($""Tagged enum is corrupted and has an unsupported tag {{value.Tag}}"");
                            }}
                        ";
                    }
                    else
                    {
                        read =
                            $@"new {type.GenericName} {{
                                {string.Join("\n", fieldIO.Select(m => m.Read))}
                            }}";

                        write = string.Join("\n", fieldIO.Select(m => m.Write));
                    }

                    typeDesc +=
                        $@"
private static SpacetimeDB.SATS.TypeInfo<{type.GenericName}>? satsTypeInfo;

public static SpacetimeDB.SATS.TypeInfo<{type.GenericName}> GetSatsTypeInfo({
    string.Join(", ", type.TypeParams.Select(p => $"SpacetimeDB.SATS.TypeInfo<{p}> {p}TypeInfo"))
}) {{
    if (satsTypeInfo is not null) {{
        return satsTypeInfo;
    }}
    var typeRef = SpacetimeDB.Module.FFI.AllocTypeRef();
    // Careful with the order: to prevent infinite recursion, we need to assign satsTypeInfo first,
    // and populate fieldTypeInfo and, correspondingly, read/write implementations, after that.
    System.Func<System.IO.BinaryReader, {type.GenericName}> read = (reader) => throw new System.InvalidOperationException(""Recursive type is not yet initialized"");
    System.Action<System.IO.BinaryWriter, {type.GenericName}> write = (writer, value) => throw new System.InvalidOperationException(""Recursive type is not yet initialized"");
    satsTypeInfo = new(
        typeRef,
        (reader) => read(reader),
        (writer, value) => write(writer, value)
    );
    var fieldTypeInfo = new {{
        {string.Join("\n", type.Members.Select(m => $"{m.Name} = {GetTypeInfo(m.TypeSymbol)},"))}
    }};
    SpacetimeDB.Module.FFI.SetTypeRef<{type.GenericName}>(
        typeRef,
        new SpacetimeDB.SATS.{typeKind}Type {{
            {string.Join("\n", type.Members.Select(m => $"{{ nameof({m.Name}), fieldTypeInfo.{m.Name}.AlgebraicType }},"))}
        }},
        {(
            fullyQualifiedMetadataName == "SpacetimeDB.TableAttribute"
            // anonymous (don't register type alias) if it's a table that will register its own name in a different way
            ? "true"
            : "false"
        )}
    );
    read = (reader) => {read};
    write = (writer, value) => {{
        {write}
    }};
    return satsTypeInfo;
}}
                    ";

                    return new KeyValuePair<string, string>(
                        type.FullName,
                        type.Scope.GenerateExtensions(typeDesc)
                    );
                }
            )
            .RegisterSourceOutputs(context);
    }
}
