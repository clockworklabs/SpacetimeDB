namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static Utils;

public record MemberDeclaration(string Name, string Type, string TypeInfo)
{
    public MemberDeclaration(string name, ITypeSymbol type)
        : this(name, SymbolToName(type), GetTypeInfo(type)) { }

    public MemberDeclaration(IFieldSymbol field)
        : this(field.Name, field.Type) { }

    public static string GenerateBsatnFields(
        Accessibility visibility,
        IEnumerable<MemberDeclaration> members
    )
    {
        var visStr = SyntaxFacts.GetText(visibility);
        return string.Join(
            "\n",
            members.Select(m => $"{visStr} static readonly {m.TypeInfo} {m.Name} = new();")
        );
    }

    public static string GenerateDefs(IEnumerable<MemberDeclaration> members) =>
        string.Join(
            ",\n",
            members.Select(m => $"new(nameof({m.Name}), {m.Name}.GetAlgebraicType(registrar))")
        );
}

public enum TypeKind
{
    Product,
    Sum,
}

public abstract record BaseTypeDeclaration<M>
    where M : MemberDeclaration, IEquatable<M>
{
    public readonly Scope Scope;
    public readonly string ShortName;
    public readonly string FullName;
    public readonly TypeKind Kind;
    public EquatableArray<M> Members { get; init; }

    protected abstract M ConvertMember(IFieldSymbol field);

    public BaseTypeDeclaration(GeneratorAttributeSyntaxContext context)
    {
        var typeSyntax = (TypeDeclarationSyntax)context.TargetNode;
        var type = (INamedTypeSymbol)context.TargetSymbol;

        // Note: we could use naively use `type.GetMembers()` to get all fields of the type,
        // but some users add their own fields in extra partial declarations like this:
        //
        // ```csharp
        // [SpacetimeDB.Type]
        // partial class MyType
        // {
        //     public int TableField;
        // }
        //
        // partial class MyType
        // {
        //     public int ExtraField;
        // }
        // ```
        //
        // In this scenario, only fields declared inside the declaration with the `[SpacetimeDB.Type]` attribute
        // should be considered as BSATN fields, and others are expected to be ignored.
        //
        // To achieve this, we need to walk over the annotated type syntax node, collect the field names,
        // and look up the resolved field symbols only for those fields.
        var fields = typeSyntax
            .Members.OfType<FieldDeclarationSyntax>()
            .SelectMany(f => f.Declaration.Variables)
            .SelectMany(v => type.GetMembers(v.Identifier.Text))
            .OfType<IFieldSymbol>()
            .Where(f => !f.IsStatic);

        // Check if type implements generic `SpacetimeDB.TaggedEnum<Variants>` and, if so, extract the `Variants` type.
        if (type.BaseType?.OriginalDefinition.ToString() == "SpacetimeDB.TaggedEnum<Variants>")
        {
            Kind = TypeKind.Sum;

            if (
                type.BaseType.TypeArguments[0]
                is not INamedTypeSymbol { IsTupleType: true, TupleElements: var taggedEnumVariants }
            )
            {
                throw new InvalidOperationException("TaggedEnum must have a tuple type argument.");
            }

            if (fields.Any())
            {
                throw new InvalidOperationException("Tagged enums cannot have fields.");
            }

            fields = taggedEnumVariants;
        }
        else
        {
            Kind = TypeKind.Product;
        }

        if (typeSyntax.TypeParameterList is not null)
        {
            throw new InvalidOperationException(
                "Types with type parameters are not yet supported."
            );
        }

        Scope = new(typeSyntax);
        ShortName = type.Name;
        FullName = SymbolToName(type);
        Members = new(fields.Select(ConvertMember).ToImmutableArray());
    }

    public virtual Scope.Extensions ToExtensions()
    {
        string read,
            write;

        var extensions = new Scope.Extensions(Scope, FullName);

        var bsatnDecls = Members.Cast<MemberDeclaration>();
        var fieldNames = bsatnDecls.Select(m => m.Name);

        if (Kind is TypeKind.Sum)
        {
            extensions.Contents.Append(
                $$"""
                private {{ShortName}}() { }

                internal enum @enum: byte
                {
                    {{string.Join(",\n", fieldNames)}}
                }
                """
            );

            bsatnDecls = bsatnDecls.Prepend(
                new("__enumTag", "@enum", "SpacetimeDB.BSATN.Enum<@enum>")
            );

            extensions.Contents.Append(
                string.Join(
                    "\n",
                    Members.Select(m =>
                        // C# puts field names in the same namespace as records themselves, and will complain about clashes if they match.
                        // To avoid this, we append an underscore to the field name.
                        // In most cases the field name shouldn't matter anyway as you'll idiomatically use pattern matching to extract the value.
                        $"public sealed record {m.Name}({m.Type} {m.Name}_) : {ShortName};"
                    )
                )
            );

            read = $$"""
                __enumTag.Read(reader) switch {
                    {{string.Join(
                        "\n",
                        fieldNames.Select(name =>
                            $"@enum.{name} => new {name}({name}.Read(reader)),"
                        )
                    )}}
                    _ => throw new System.InvalidOperationException("Invalid tag value, this state should be unreachable.")
                }
                """;

            write = $$"""
                switch (value) {
                    {{string.Join(
                        "\n",
                        fieldNames.Select(name => $"""
                            case {name}(var inner):
                                __enumTag.Write(writer, @enum.{name});
                                {name}.Write(writer, inner);
                                break;
                            """)
                    )}}
                }
                """;
        }
        else
        {
            extensions.BaseTypes.Add("SpacetimeDB.BSATN.IStructuralReadWrite");

            extensions.Contents.Append(
                $$"""
                public void ReadFields(System.IO.BinaryReader reader) {
                    {{string.Join(
                        "\n",
                        fieldNames.Select(name => $"{name} = BSATN.{name}.Read(reader);")
                    )}}
                }

                public void WriteFields(System.IO.BinaryWriter writer) {
                    {{string.Join(
                        "\n",
                        fieldNames.Select(name => $"BSATN.{name}.Write(writer, {name});")
                    )}}
                }
                """
            );

            read = $"SpacetimeDB.BSATN.IStructuralReadWrite.Read<{ShortName}>(reader)";

            write = "value.WriteFields(writer);";
        }

        extensions.Contents.Append(
            $$"""
            public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<{{ShortName}}>
            {
                {{MemberDeclaration.GenerateBsatnFields(Accessibility.Internal, bsatnDecls)}}

                public {{ShortName}} Read(System.IO.BinaryReader reader) => {{read}};

                public void Write(System.IO.BinaryWriter writer, {{ShortName}} value) {
                    {{write}}
                }

                public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(SpacetimeDB.BSATN.ITypeRegistrar registrar) =>
                    registrar.RegisterType<{{ShortName}}>(_ => new SpacetimeDB.BSATN.AlgebraicType.{{Kind}}(new SpacetimeDB.BSATN.AggregateElement[] {
                        {{MemberDeclaration.GenerateDefs(Members)}}
                    }));
            }
            """
        );

        return extensions;
    }
}

record TypeDeclaration : BaseTypeDeclaration<MemberDeclaration>
{
    public TypeDeclaration(GeneratorAttributeSyntaxContext context)
        : base(context) { }

    protected override MemberDeclaration ConvertMember(IFieldSymbol field) => new(field);
}

[Generator]
public class Type : IIncrementalGenerator
{
    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        context
            .SyntaxProvider.ForAttributeWithMetadataName(
                "SpacetimeDB.TypeAttribute",
                predicate: (node, ct) =>
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
                },
                transform: (context, ct) => new TypeDeclaration(context)
            )
            .WithTrackingName("SpacetimeDB.Type.Parse")
            .Select((type, ct) => type.ToExtensions())
            .WithTrackingName("SpacetimeDB.Type.GenerateExtensions")
            .RegisterSourceOutputs(context);
    }
}
