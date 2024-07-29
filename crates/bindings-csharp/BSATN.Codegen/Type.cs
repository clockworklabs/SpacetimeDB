namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using System.Text;
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

    public string GenerateBSATNField(Accessibility visibility) =>
        $"{SyntaxFacts.GetText(visibility)} static readonly {TypeInfo} {Name} = new();";

    public static string GenerateDefs(IEnumerable<MemberDeclaration> members) =>
        $$"""
        new SpacetimeDB.BSATN.AggregateElement[] {
            {{string.Join(
                ",\n",
                members.Select(m => $"new(nameof({m.Name}), {m.Name}.GetAlgebraicType(registrar))")
            )}}
        }
        """;
}

abstract record TypeKind
{
    public record Product : TypeKind
    {
        public sealed override string ToString() => "Product";
    }

    public record Sum : TypeKind
    {
        public sealed override string ToString() => "Sum";
    }

    public record Table(bool IsScheduled) : Product;
}

record TypeDeclaration
{
    public readonly Scope Scope;
    public readonly string ShortName;
    public readonly string FullName;
    public readonly TypeKind Kind;
    public readonly EquatableArray<MemberDeclaration> Members;

    public TypeDeclaration(GeneratorAttributeSyntaxContext context)
    {
        var typeSyntax = (TypeDeclarationSyntax)context.TargetNode;
        var type = (INamedTypeSymbol)context.TargetSymbol;
        var attr = context.Attributes.Single();

        var fields = GetFields(typeSyntax, type);

        Kind =
            attr.AttributeClass?.Name == "TableAttribute"
                ? new TypeKind.Table(attr.NamedArguments.Any(a => a.Key == "Scheduled"))
                : new TypeKind.Product();

        // Check if type implements generic `SpacetimeDB.TaggedEnum<Variants>` and, if so, extract the `Variants` type.
        if (type.BaseType?.OriginalDefinition.ToString() == "SpacetimeDB.TaggedEnum<Variants>")
        {
            if (Kind is TypeKind.Table)
            {
                throw new InvalidOperationException("Tagged enums cannot be tables.");
            }

            Kind = new TypeKind.Sum();

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

        if (typeSyntax.TypeParameterList is not null)
        {
            throw new InvalidOperationException(
                "Types with type parameters are not yet supported."
            );
        }

        Scope = new(typeSyntax);
        ShortName = type.Name;
        FullName = SymbolToName(type);
        Members = new(fields.Select(v => new MemberDeclaration(v)).ToImmutableArray());
    }

    public KeyValuePair<string, string> GenerateOutput()
    {
        string read,
            write;

        var typeDesc = new StringBuilder();

        var bsatnDecls = Members.Cast<MemberDeclaration>();

        if (Kind is TypeKind.Table { IsScheduled: true })
        {
            // For scheduled tables, we append extra fields early in the pipeline,
            // both to the type itself and to the BSATN information, as if they
            // were part of the original declaration.

            typeDesc.Append(
                """
                public ulong ScheduledId;
                public SpacetimeDB.ScheduleAt ScheduledAt;
                """
            );
            bsatnDecls = bsatnDecls.Concat(
                [
                    new("ScheduledId", "ulong", "SpacetimeDB.BSATN.U64"),
                    new("ScheduledAt", "SpacetimeDB.ScheduleAt", "SpacetimeDB.ScheduleAt.BSATN"),
                ]
            );
        }

        var fieldNames = bsatnDecls.Select(m => m.Name);

        if (Kind is TypeKind.Sum)
        {
            typeDesc.Append(
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

            typeDesc.Append(
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
            typeDesc.Append(
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

        typeDesc.Append(
            $$"""
            public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<{{ShortName}}>
            {
                {{string.Join(
                    "\n",
                    bsatnDecls.Select(decl => decl.GenerateBSATNField(Accessibility.Internal))
                )}}

                public {{ShortName}} Read(System.IO.BinaryReader reader) => {{read}};

                public void Write(System.IO.BinaryWriter writer, {{ShortName}} value) {
                    {{write}}
                }

                public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(SpacetimeDB.BSATN.ITypeRegistrar registrar) =>
                    registrar.RegisterType<{{ShortName}}>(_ => new SpacetimeDB.BSATN.AlgebraicType.{{Kind}}(
                        {{MemberDeclaration.GenerateDefs(Members)}}
                    ));
            }
            """
        );

        return new(
            FullName,
            Scope.GenerateExtensions(
                typeDesc.ToString(),
                Kind is TypeKind.Product ? "SpacetimeDB.BSATN.IStructuralReadWrite" : null,
                // For scheduled tables we're adding extra fields and compiler will warn about undefined ordering.
                // We don't care about ordering as we generate BSATN ourselves and don't use those structs in FFI,
                // so we can safely suppress the warning by saying "yes, we're okay with an auto/arbitrary layout".
                Kind is TypeKind.Table { IsScheduled: true }
                    ? "[System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Auto)]"
                    : null
            )
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
                transform: (context, ct) => new TypeDeclaration(context)
            )
            .WithTrackingName("SpacetimeDB.Type.Parse")
            .Select((type, ct) => type.GenerateOutput())
            .WithTrackingName("SpacetimeDB.Type.GenerateExtensions")
            .RegisterSourceOutputs(context);
    }
}
