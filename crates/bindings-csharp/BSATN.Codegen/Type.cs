namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using SpacetimeDB.Codegen.Utils;

public record FieldDeclaration : MemberDeclaration
{
    public FieldDeclaration(IFieldSymbol field)
        : base(field.Name, field.Type) { }
}

public abstract record BaseTypeDeclaration<M> : SourceOutput
    where M : FieldDeclaration, IEquatable<M>
{
    public readonly bool IsTaggedEnum;
    public readonly EquatableArray<M> Members;

    protected abstract M ConvertMember(IFieldSymbol field);

    private BaseTypeDeclaration(TypeDeclarationSyntax typeSyntax, INamedTypeSymbol type)
        : base(new Scope(typeSyntax), type)
    {
        var fields = type.GetMembers().OfType<IFieldSymbol>().Where(f => !f.IsStatic);

        // Check if type implements generic `SpacetimeDB.TaggedEnum<Variants>` and, if so, extract the `Variants`
        if (type.BaseType?.OriginalDefinition.ToString() == "SpacetimeDB.TaggedEnum<Variants>")
        {
            IsTaggedEnum = true;

            if (
                type.BaseType.TypeArguments
                is not [
                    INamedTypeSymbol { IsTupleType: true, TupleElements: var taggedEnumVariants }
                ]
            )
            {
                Diagnostics.TaggedEnumInlineTuple.Report(
                    Diagnostics,
                    typeSyntax.BaseList!.Types[0]
                );
            }

            if (fields.FirstOrDefault() is { } field)
            {
                Diagnostics.TaggedEnumField.Report(Diagnostics, field);
            }

            fields = taggedEnumVariants;
        }

        if (typeSyntax.TypeParameterList is { } typeParams)
        {
            Diagnostics.TypeParams.Report(Diagnostics, typeParams);
        }

        Members = new(fields.Select(ConvertMember).ToImmutableArray());
    }

    public BaseTypeDeclaration(GeneratorAttributeSyntaxContext context)
        : this((TypeDeclarationSyntax)context.TargetNode, (INamedTypeSymbol)context.TargetSymbol)
    { }

    protected override string? BaseClassesOrInterfaces =>
        IsTaggedEnum ? null : "SpacetimeDB.BSATN.IStructuralReadWrite";

    public override string ToExtensions()
    {
        string typeKind,
            read,
            write;

        var typeDesc = "";

        var bsatnDecls = Members.Select(m => (m.Name, m.Type.BSATN));

        if (IsTaggedEnum)
        {
            typeKind = "Sum";

            typeDesc += $$"""
                private {{this}}() { }

                internal enum @enum: byte
                {
                    {{Members.Join(",\n", m => m.Name)}}
                }
                """;

            bsatnDecls = bsatnDecls.Prepend(
                (Name: "__enumTag", BSATN: "SpacetimeDB.BSATN.Enum<@enum>")
            );

            typeDesc += Members.Join(
                "\n",
                m =>
                    // C# puts field names in the same namespace as records themselves, and will complain about clashes if they match.
                    // To avoid this, we append an underscore to the field name.
                    // In most cases the field name shouldn't matter anyway as you'll idiomatically use pattern matching to extract the value.
                    $"public sealed record {m}({m.Type} {m}_) : {this};"
            );

            read = $$"""
                __enumTag.Read(reader) switch {
                    {{Members.Join("\n", m =>
                        $"@enum.{m} => new {m}({m}.Read(reader)),"
                    )}}
                    _ => throw new System.InvalidOperationException("Invalid tag value, this state should be unreachable.")
                }
                """;

            write = $$"""
                switch (value) {
                    {{Members.Join("\n", m => $"""
                        case {m}(var inner):
                            __enumTag.Write(writer, @enum.{m});
                            {m}.Write(writer, inner);
                            break;
                    """)}}
                }
                """;
        }
        else
        {
            typeKind = "Product";

            typeDesc += $$"""
                public void ReadFields(System.IO.BinaryReader reader) {
                    {{Members.Join("\n", m =>
                        $"{m} = BSATN.{m}.Read(reader);"
                    )}}
                }

                public void WriteFields(System.IO.BinaryWriter writer) {
                    {{Members.Join("\n",
                        m => $"BSATN.{m}.Write(writer, {m});"
                    )}}
                }
                """;

            read = $"SpacetimeDB.BSATN.IStructuralReadWrite.Read<{this}>(reader)";

            write = "value.WriteFields(writer);";
        }

        typeDesc += $$"""
            public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<{{this}}>
            {
                {{bsatnDecls.Join("\n", decl =>
                    $"internal static readonly {decl.BSATN} {decl.Name} = new();"
                )}}

                public {{this}} Read(System.IO.BinaryReader reader) => {{read}};

                public void Write(System.IO.BinaryWriter writer, {{this}} value) {
                    {{write}}
                }

                public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(SpacetimeDB.BSATN.ITypeRegistrar registrar) => registrar.RegisterType<{{this}}>(typeRef => new SpacetimeDB.BSATN.AlgebraicType.{{typeKind}}(new SpacetimeDB.BSATN.AggregateElement[] {
                    {{Members.Join(",\n", m =>
                        $"new(nameof({m}), {m}.GetAlgebraicType(registrar))"
                    )}}
                }));
            }
            """;

        return typeDesc;
    }
}

record TypeDeclaration : BaseTypeDeclaration<FieldDeclaration>
{
    public TypeDeclaration(GeneratorAttributeSyntaxContext context)
        : base(context) { }

    protected override FieldDeclaration ConvertMember(IFieldSymbol field) => new(field);
}

[Generator]
public class Type : IIncrementalGenerator
{
    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        // Handle enums separately: for those we don't emit any code
        // (because C# doesn't allow us to add methods to enums) and instead
        // just validate that they are compatible with SATS.
        context.RegisterSourceOutput(
            context.SyntaxProvider.ForAttributeWithMetadataName(
                "SpacetimeDB.TypeAttribute",
                predicate: (node, ct) => node is EnumDeclarationSyntax,
                transform: (context, ct) =>
                {
                    var enumType = (EnumDeclarationSyntax)context.TargetNode;
                    var diagnostics = new Diagnostics();

                    // Ensure variants are contiguous as SATS enums don't support explicit tags.
                    foreach (var m in enumType.Members)
                    {
                        if (m.EqualsValue is { } equalsValue)
                        {
                            Diagnostics.EnumWithExplicitValues.Report(
                                diagnostics,
                                (equalsValue, m, enumType)
                            );
                        }
                    }

                    // Ensure all enums fit in `byte` as that's what SATS uses for tags.
                    if (enumType.Members.Count > 256)
                    {
                        Diagnostics.EnumTooManyVariants.Report(diagnostics, enumType);
                    }

                    return diagnostics;
                }
            ),
            (context, diag) => diag.Emit(context)
        );

        context.HandleDerives(
            "SpacetimeDB.Type",
            context => new TypeDeclaration(context),
            node => node is not EnumDeclarationSyntax
        );
    }
}
