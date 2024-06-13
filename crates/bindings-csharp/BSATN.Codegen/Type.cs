namespace SpacetimeDB.Codegen;

using System;
using System.Collections.Generic;
using System.Collections.Immutable;
using System.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static Utils;

readonly record struct MemberDeclaration
{
    public readonly string Name;
    public readonly string Type;
    public readonly string TypeInfo;

    public MemberDeclaration(string name, ITypeSymbol typeSymbol)
    {
        Name = name;
        Type = SymbolToName(typeSymbol);
        TypeInfo = GetTypeInfo(typeSymbol);
    }
}

record TypeDeclaration
{
    public readonly Scope Scope;
    public readonly string ShortName;
    public readonly string FullName;
    public readonly bool IsTaggedEnum;
    public readonly EquatableArray<MemberDeclaration> Members;

    public TypeDeclaration(
        TypeDeclarationSyntax typeSyntax,
        INamedTypeSymbol type,
        bool isTaggedEnum,
        IEnumerable<IFieldSymbol> members
    )
    {
        Scope = new(typeSyntax);
        ShortName = type.Name;
        FullName = SymbolToName(type);
        IsTaggedEnum = isTaggedEnum;
        Members = new(
            members.Select(v => new MemberDeclaration(v.Name, v.Type)).ToImmutableArray()
        );
    }
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

    public static void WithAttrAndPredicate(
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
                    var typeSyntax = (TypeDeclarationSyntax)context.TargetNode;
                    var type = context.SemanticModel.GetDeclaredSymbol(typeSyntax, ct)!;
                    var fields = GetFields(type);
                    var isTaggedEnum = false;

                    // Check if type implements generic `SpacetimeDB.TaggedEnum<Variants>` and, if so, extract the `Variants` type.
                    if (
                        type.BaseType?.OriginalDefinition.ToString()
                        == "SpacetimeDB.TaggedEnum<Variants>"
                    )
                    {
                        isTaggedEnum = true;

                        if (
                            type.BaseType.TypeArguments[0]
                            is not INamedTypeSymbol
                            {
                                IsTupleType: true,
                                TupleElements: var taggedEnumVariants
                            }
                        )
                        {
                            throw new InvalidOperationException(
                                "TaggedEnum must have a tuple type argument."
                            );
                        }

                        if (fields.Any())
                        {
                            throw new InvalidOperationException("Tagged enums cannot have fields.");
                        }

                        fields = taggedEnumVariants;
                    }

                    if (typeSyntax.TypeParameterList is not null)
                    {
                        throw new InvalidOperationException(
                            "Types with type parameters are not yet supported."
                        );
                    }

                    return new TypeDeclaration(typeSyntax, type, isTaggedEnum, fields);
                }
            )
            .WithTrackingName("SpacetimeDB.Type.Parse")
            .Select(
                (type, ct) =>
                {
                    string typeKind,
                        read,
                        write;

                    var typeDesc = "";

                    var bsatnDecls = type.Members.Select(m => (m.Name, m.TypeInfo));
                    var fieldNames = type.Members.Select(m => m.Name);

                    if (type.IsTaggedEnum)
                    {
                        typeKind = "Sum";

                        typeDesc +=
                            $@"
                            private {type.ShortName}() {{ }}

                            internal enum @enum: byte
                            {{
                                {string.Join(",\n", fieldNames)}
                            }}
                        ";

                        bsatnDecls = bsatnDecls.Prepend(
                            (Name: "__enumTag", TypeInfo: "SpacetimeDB.BSATN.Enum<@enum>")
                        );

                        typeDesc += string.Join(
                            "\n",
                            type.Members.Select(m =>
                                // C# puts field names in the same namespace as records themselves, and will complain about clashes if they match.
                                // To avoid this, we append an underscore to the field name.
                                // In most cases the field name shouldn't matter anyway as you'll idiomatically use pattern matching to extract the value.
                                $@"public sealed record {m.Name}({m.Type} {m.Name}_) : {type.ShortName};"
                            )
                        );

                        read =
                            $@"__enumTag.Read(reader) switch {{
                                {string.Join("\n", fieldNames.Select(name => $"@enum.{name} => new {name}({name}.Read(reader)),"))}
                                _ => throw new System.InvalidOperationException(""Invalid tag value, this state should be unreachable."")
                            }}";

                        write =
                            $@"switch (value) {{
                                {string.Join("\n", fieldNames.Select(name => $@"
                                    case {name}(var inner):
                                        __enumTag.Write(writer, @enum.{name});
                                        {name}.Write(writer, inner);
                                        break;
                                "))}
                            }}";
                    }
                    else
                    {
                        typeKind = "Product";

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
                        public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<{type.ShortName}>
                        {{
                            {string.Join("\n", bsatnDecls.Select(decl => $"internal static readonly {decl.TypeInfo} {decl.Name} = new();"))}

                            public {type.ShortName} Read(System.IO.BinaryReader reader) => {read};

                            public void Write(System.IO.BinaryWriter writer, {type.ShortName} value) {{
                                {write}
                            }}

                            public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(SpacetimeDB.BSATN.ITypeRegistrar registrar) => registrar.RegisterType<{type.ShortName}>(typeRef => new SpacetimeDB.BSATN.AlgebraicType.{typeKind}(new SpacetimeDB.BSATN.AggregateElement[] {{
                                {string.Join(",\n", fieldNames.Select(name => $"new(nameof({name}), {name}.GetAlgebraicType(registrar))"))}
                            }}));
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
            .WithTrackingName("SpacetimeDB.Type.GenerateExtensions")
            .RegisterSourceOutputs(context);
    }
}
