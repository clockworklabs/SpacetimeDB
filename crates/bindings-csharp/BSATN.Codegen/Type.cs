namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static Utils;

public record MemberDeclaration(string Name, string Type, string TypeInfo)
{
    public MemberDeclaration(ISymbol member, ITypeSymbol type, DiagReporter diag)
        : this(member.Name, SymbolToName(type), "")
    {
        try
        {
            TypeInfo = GetTypeInfo(type);
        }
        catch (Exception e)
        {
            diag.Report(ErrorDescriptor.UnsupportedType, (member, type, e));
            // dummy type; can't instantiate an interface, but at least it will produce fewer noisy errors
            TypeInfo = $"SpacetimeDB.BSATN.IReadWrite<{Type}>";
        }
    }

    public MemberDeclaration(IFieldSymbol field, DiagReporter diag)
        : this(field, field.Type, diag) { }

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

    protected abstract M ConvertMember(IFieldSymbol field, DiagReporter diag);

    public BaseTypeDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
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
            .Select(v => type.GetMembers(v.Identifier.Text).OfType<IFieldSymbol>().Single())
            .Where(f => !f.IsStatic);

        // Check if type implements generic `SpacetimeDB.TaggedEnum<Variants>` and, if so, extract the `Variants` type.
        if (type.BaseType?.OriginalDefinition.ToString() == "SpacetimeDB.TaggedEnum<Variants>")
        {
            Kind = TypeKind.Sum;

            if (
                type.BaseType.TypeArguments.FirstOrDefault()
                is not INamedTypeSymbol { IsTupleType: true, TupleElements: var taggedEnumVariants }
            )
            {
                diag.Report(ErrorDescriptor.TaggedEnumInlineTuple, type.BaseType);
            }

            if (fields.FirstOrDefault() is { } field)
            {
                diag.Report(ErrorDescriptor.TaggedEnumField, field);
            }

            fields = taggedEnumVariants;
        }
        else
        {
            Kind = TypeKind.Product;
        }

        if (typeSyntax.TypeParameterList is { } typeParams)
        {
            diag.Report(ErrorDescriptor.TypeParams, typeParams);
        }

        Scope = new(typeSyntax);
        ShortName = type.Name;
        FullName = SymbolToName(type);
        Members = new(fields.Select(field => ConvertMember(field, diag)).ToImmutableArray());
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

            read = $"SpacetimeDB.BSATN.IStructuralReadWrite.Read<{FullName}>(reader)";

            write = "value.WriteFields(writer);";
        }

        extensions.Contents.Append(
            $$"""
            public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<{{FullName}}>
            {
                {{MemberDeclaration.GenerateBsatnFields(Accessibility.Internal, bsatnDecls)}}

                public {{FullName}} Read(System.IO.BinaryReader reader) => {{read}};

                public void Write(System.IO.BinaryWriter writer, {{FullName}} value) {
                    {{write}}
                }

                public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(SpacetimeDB.BSATN.ITypeRegistrar registrar) =>
                    registrar.RegisterType<{{FullName}}>(_ => new SpacetimeDB.BSATN.AlgebraicType.{{Kind}}(new SpacetimeDB.BSATN.AggregateElement[] {
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
    public TypeDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
        : base(context, diag) { }

    protected override MemberDeclaration ConvertMember(IFieldSymbol field, DiagReporter diag) =>
        new(field, diag);
}

[Generator]
public class Type : IIncrementalGenerator
{
    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        context
            .SyntaxProvider.ForAttributeWithMetadataName(
                "SpacetimeDB.TypeAttribute",
                // for enums, we just do diagnostics, nothing else
                predicate: (node, ct) => node is EnumDeclarationSyntax,
                transform: (context, ct) =>
                    context.ParseWithDiags(diag =>
                    {
                        var enumType = (EnumDeclarationSyntax)context.TargetNode;

                        // Ensure variants are contiguous as SATS enums don't support explicit tags.
                        foreach (var variant in enumType.Members)
                        {
                            if (variant.EqualsValue is { } equalsValue)
                            {
                                diag.Report(
                                    ErrorDescriptor.EnumWithExplicitValues,
                                    (equalsValue, variant, enumType)
                                );
                            }
                        }

                        // Ensure all enums fit in `byte` as that's what SATS uses for tags.
                        if (enumType.Members.Count > 256)
                        {
                            diag.Report(ErrorDescriptor.EnumTooManyVariants, enumType);
                        }

                        // Unused empty type.
                        return default(ValueTuple);
                    })
            )
            .ReportDiagnostics(context)
            .WithTrackingName("SpacetimeDB.Type.ParseEnum");

        context
            .SyntaxProvider.ForAttributeWithMetadataName(
                "SpacetimeDB.TypeAttribute",
                // parse anything except enums here (they're handled above)
                predicate: (node, ct) => node is not EnumDeclarationSyntax,
                transform: (context, ct) =>
                    context.ParseWithDiags(diag => new TypeDeclaration(context, diag))
            )
            .ReportDiagnostics(context)
            .WithTrackingName("SpacetimeDB.Type.Parse")
            .Select((type, ct) => type.ToExtensions())
            .WithTrackingName("SpacetimeDB.Type.GenerateExtensions")
            .RegisterSourceOutputs(context);
    }
}
