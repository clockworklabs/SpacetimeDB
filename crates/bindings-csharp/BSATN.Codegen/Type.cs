namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static Utils;

/// <summary>
/// The type of a member of one of the types we are generating code for.
///
/// Knows how to serialize and deserialize the member.
///
/// Also knows how to compare the member for equality and compute its hash code.
/// We can't just use Equals and GetHashCode for this, because they implement reference
/// equality for arrays and Lists.
///
/// (It would be nice to be able to dynamically build EqualityComparers at runtime
/// to do these operations, but this seems to require either (A) reflective calls
/// or (B) instantiating generics at runtime. These are (A) slow and (B) very slow
/// when compiling under IL2CPP. Instead, we just inline the needed loops to compute
/// the relevant values. This is very simple for IL2CPP to optimize.
/// That's good, since Equals and GetHashCode for BSATN types are used in hot parts
/// of the codebase.)
/// </summary>
/// <param name="Name">The name of the type</param>
/// <param name="BSATNName">The name of the BSATN struct for the type.</param>
public abstract record TypeUse(string Name, string BSATNName)
{
    /// <summary>
    /// Parse a type use for a member.
    /// </summary>
    /// <param name="member">The member name. Only used for reporting parsing failures.</param>
    /// <param name="typeSymbol">The type we are using. May not be the type of the member: this method is called recursively.</param>
    /// <param name="diag"></param>
    /// <returns></returns>
    public static TypeUse Parse(ISymbol member, ITypeSymbol typeSymbol, DiagReporter diag)
    {
        var type = SymbolToName(typeSymbol);
        string typeInfo;

        try
        {
            typeInfo = GetTypeInfo(typeSymbol);
        }
        catch (UnresolvedTypeException)
        {
            // If it's an unresolved type, this error will have been already highlighted by .NET itself, no need to add noise.
            // Just add some dummy type to avoid further errors.
            // Note that we just use `object` here because emitting the unresolved type's name again would produce more of said noise.
            typeInfo = "SpacetimeDB.BSATN.Unsupported<object>";
        }
        catch (Exception e)
        {
            diag.Report(ErrorDescriptor.UnsupportedType, (member, typeSymbol, e));
            // dummy BSATN implementation to produce fewer noisy errors
            typeInfo = $"SpacetimeDB.BSATN.Unsupported<{type}>";
        }

        return typeSymbol switch
        {
            ITypeParameterSymbol => new ReferenceUse(type, typeInfo),
            IArrayTypeSymbol { ElementType: var elementType } => new ArrayUse(
                type,
                typeInfo,
                Parse(member, elementType, diag)
            ),
            INamedTypeSymbol named => named.OriginalDefinition.ToString() switch
            {
                "System.Collections.Generic.List<T>" => new ListUse(
                    type,
                    typeInfo,
                    Parse(member, named.TypeArguments[0], diag)
                ),
                _ => named.IsValueType
                    ? (
                        named.TypeKind == Microsoft.CodeAnalysis.TypeKind.Enum
                            ? new EnumUse(type, typeInfo)
                            : new ValueUse(type, typeInfo)
                    )
                    : new ReferenceUse(type, typeInfo),
            },
            _ => throw new InvalidOperationException($"Unsupported type {type}"),
        };
    }

    /// <summary>
    /// Get a statement that declares outVar and assigns (inVar1 logically-equals inVar2) to it.
    /// logically-equals:
    /// - recursively compares lists and arrays by sequence equality.
    /// - is the same as .Equals( ) for everything else.
    ///
    /// This can't be an expression because some types need to use loops.
    /// </summary>
    /// <param name="inVar1">A variable of type `Type` that we want to hash.</param>
    /// <param name="inVar2">A variable of type `Type` that we want to hash.</param>
    /// <param name="outVar">The variable to declare and store the `Equals` bool in.</param>
    /// <param name="level">Iteration level counter. You don't need to set this.</param>
    /// <returns></returns>
    public abstract string EqualsStatement(
        string inVar1,
        string inVar2,
        string outVar,
        int level = 0
    );

    /// <summary>
    /// Get a statement that declares outVar and assigns the hash code of inVar to it.
    ///
    /// This can't be an expression because some types need to use loops.
    /// </summary>
    /// <param name="inVar">A variable of type `Type` that we want to hash.</param>
    /// <param name="outVar">The variable to declare and store the hash in.</param>
    /// <param name="level">Iteration level counter. You don't need to set this.</param>
    /// <returns></returns>
    public abstract string GetHashCodeStatement(string inVar, string outVar, int level = 0);
}

/// <summary>
/// A use of an enum type.
/// (This is a C# enum, not one of our tagged enums.)
/// </summary>
/// <param name="Type"></param>
/// <param name="TypeInfo"></param>
public record EnumUse(string Type, string TypeInfo) : TypeUse(Type, TypeInfo)
{
    // We just use `==` here, rather than `.Equals`, because
    // C# enums don't provide a `bool Equals(Self other)`, and
    // using `.Equals(object other)` allocates, which we want to avoid.
    //
    // We could instead generate custom .Equals for enums -- except that requires
    // partial enums, and I'm not sure such things exist.
    public override string EqualsStatement(
        string inVar1,
        string inVar2,
        string outVar,
        int level = 0
    ) => $"var {outVar} = {inVar1} == {inVar2};";

    public override string GetHashCodeStatement(string inVar, string outVar, int level = 0) =>
        $"var {outVar} = {inVar}.GetHashCode();";
}

/// <summary>
/// A use of a value type (that is not an enum).
/// </summary>
/// <param name="Type"></param>
/// <param name="TypeInfo"></param>
public record ValueUse(string Type, string TypeInfo) : TypeUse(Type, TypeInfo)
{
    public override string EqualsStatement(
        string inVar1,
        string inVar2,
        string outVar,
        int level = 0
    ) => $"var {outVar} = {inVar1}.Equals({inVar2});";

    public override string GetHashCodeStatement(string inVar, string outVar, int level = 0) =>
        $"var {outVar} = {inVar}.GetHashCode();";
}

/// <summary>
/// A use of a reference type.
/// </summary>
/// <param name="Type"></param>
/// <param name="TypeInfo"></param>
public record ReferenceUse(string Type, string TypeInfo) : TypeUse(Type, TypeInfo)
{
    public override string EqualsStatement(
        string inVar1,
        string inVar2,
        string outVar,
        int level = 0
    ) => $"var {outVar} = {inVar1} == null ? {inVar2} == null : {inVar1}.Equals({inVar2});";

    public override string GetHashCodeStatement(string inVar, string outVar, int level = 0) =>
        $"var {outVar} = {inVar} == null ? 0 : {inVar}.GetHashCode();";
}

/// <summary>
/// A use of an array type.
/// </summary>
/// <param name="Type"></param>
/// <param name="TypeInfo"></param>
/// <param name="ElementType"></param>
public record ArrayUse(string Type, string TypeInfo, TypeUse ElementType) : TypeUse(Type, TypeInfo)
{
    public override string EqualsStatement(
        string inVar1,
        string inVar2,
        string outVar,
        int level = 0
    )
    {
        var iterVar = $"___i{level}";
        var innerOutVar = $"___out{level + 1}";

        return $$"""
            var {{outVar}} = true;
            if ({{inVar1}} == null || {{inVar2}} == null) {
                {{outVar}} = {{inVar1}} == {{inVar2}};
            } else if ({{inVar1}}.Length != {{inVar2}}.Length) {
                {{outVar}} = false;
            } else {
                for (int {{iterVar}} = 0; {{iterVar}} < {{inVar1}}.Length; {{iterVar}}++) {
                    {{ElementType.EqualsStatement(
                $"{inVar1}[{iterVar}]",
                $"{inVar2}[{iterVar}]",
                innerOutVar,
                level + 1
            )}}
                    if (!{{innerOutVar}}) {
                        {{outVar}} = false;
                        break;
                    }
                }
            }
            """;
    }

    public override string GetHashCodeStatement(string inVar, string outVar, int level = 0)
    {
        var iterVar = $"___i{level}";
        var innerHashCode = $"___hc{level}";
        var innerOutVar = $"___out{level + 1}";

        return $$"""
            var {{outVar}} = 0;
            if ({{inVar}} != null) {
                var {{innerHashCode}} = new System.HashCode();
                for (int {{iterVar}} = 0; {{iterVar}} < {{inVar}}.Length; {{iterVar}}++) {
                    {{ElementType.GetHashCodeStatement(
                $"{inVar}[{iterVar}]",
                innerOutVar,
                level + 1
            )}}
                    {{innerHashCode}}.Add({{innerOutVar}});
                }
                {{outVar}} = {{innerHashCode}}.ToHashCode();
            }
            """;
    }
}

/// <summary>
/// A use of a list type.
/// </summary>
/// <param name="Type"></param>
/// <param name="TypeInfo"></param>
/// <param name="ElementType"></param>
public record ListUse(string Type, string TypeInfo, TypeUse ElementType) : TypeUse(Type, TypeInfo)
{
    public override string EqualsStatement(
        string inVar1,
        string inVar2,
        string outVar,
        int level = 0
    )
    {
        var iterVar = $"___i{level}";
        // needed to avoid warnings on list re-reference.
        var innerTmp1 = $"___tmpA{level}";
        var innerTmp2 = $"___tmpB{level}";
        var innerOutVar = $"___out{level + 1}";

        return $$"""
            var {{outVar}} = true;
            if ({{inVar1}} == null || {{inVar2}} == null) {
                {{outVar}} = {{inVar1}} == {{inVar2}};
            } else if ({{inVar1}}.Count != {{inVar2}}.Count) {
                {{outVar}} = false;
            } else {
                for (int {{iterVar}} = 0; {{iterVar}} < {{inVar1}}.Count; {{iterVar}}++) {
                    var {{innerTmp1}} = {{inVar1}}[{{iterVar}}];
                    var {{innerTmp2}} = {{inVar2}}[{{iterVar}}];
                    {{ElementType.EqualsStatement(innerTmp1, innerTmp2, innerOutVar, level + 1)}}
                    if (!{{innerOutVar}}) {
                        {{outVar}} = false;
                        break;
                    }
                }
            }
            """;
    }

    public override string GetHashCodeStatement(string inVar, string outVar, int level = 0)
    {
        var iterVar = $"___i{level}";
        var innerTmp = $"___tmp{level}";
        var innerHashCode = $"___hc{level}";
        var innerOutVar = $"___out{level + 1}";

        return $$"""
            var {{outVar}} = 0;
            if ({{inVar}} != null) {
                var {{innerHashCode}} = new System.HashCode();
                for (int {{iterVar}} = 0; {{iterVar}} < {{inVar}}.Count; {{iterVar}}++) {
                    var {{innerTmp}} = {{inVar}}[{{iterVar}}];
                    {{ElementType.GetHashCodeStatement(innerTmp, innerOutVar, level + 1)}}
                    {{innerHashCode}}.Add({{innerOutVar}});
                }
                {{outVar}} = {{innerHashCode}}.ToHashCode();
            }
            """;
    }
}

/// <summary>
/// A declaration of a member of a product or sum type.
/// </summary>
/// <param name="Name">The name of the member.</param>
/// <param name="Type">Type information relevant to the member.</param>
public record MemberDeclaration(
    string Name,
    // TODO: rename to `Type` once I've checked uses
    TypeUse Type
)
{
    public MemberDeclaration(ISymbol member, ITypeSymbol type, DiagReporter diag)
        : this(member.Name, TypeUse.Parse(member, type, diag)) { }

    public MemberDeclaration(IFieldSymbol field, DiagReporter diag)
        : this(field, field.Type, diag) { }

    public static string GenerateBsatnFields(
        Accessibility visibility,
        IEnumerable<MemberDeclaration> members
    )
    {
        var visStr = SyntaxFacts.GetText(visibility);
        return string.Join(
            "\n        ",
            members.Select(m => $"{visStr} static readonly {m.Type.BSATNName} {m.Name} = new();")
        );
    }

    public static string GenerateDefs(IEnumerable<MemberDeclaration> members) =>
        string.Join(
            ",\n                ",
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
    public readonly EquatableArray<M> Members;

    protected abstract M ConvertMember(int index, IFieldSymbol field, DiagReporter diag);

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
        Members = new(
            fields.Select((field, index) => ConvertMember(index, field, diag)).ToImmutableArray()
        );
    }

    public static string JoinOrValue(
        string join,
        IEnumerable<String> stringArray,
        string resultIfArrayEmpty
    )
    {
        if (stringArray.Any())
        {
            return string.Join(join, stringArray);
        }
        return resultIfArrayEmpty;
    }

    public Scope.Extensions ToExtensions()
    {
        string read,
            write,
            getHashCode;

        var extensions = new Scope.Extensions(Scope, FullName);

        var bsatnDecls = Members.Cast<MemberDeclaration>();
        var fieldNames = bsatnDecls.Select(m => m.Name);
        var fieldNamesAndIds = fieldNames.Select((name, i) => (name, i));

        extensions.BaseTypes.Add($"System.IEquatable<{ShortName}>");

        if (Kind is TypeKind.Sum)
        {
            extensions.Contents.Append(
                string.Join(
                    "\n",
                    Members.Select(m =>
                        // C# puts field names in the same namespace as records themselves, and will complain about clashes if they match.
                        // To avoid this, we append an underscore to the field name.
                        // In most cases the field name shouldn't matter anyway as you'll idiomatically use pattern matching to extract the value.
                        $$"""
                            public sealed record {{m.Name}}({{m.Type.Name}} {{m.Name}}_) : {{ShortName}}
                            {
                                public override string ToString() =>
                                    $"{{m.Name}}({ SpacetimeDB.BSATN.StringUtil.GenericToString({{m.Name}}_) })";
                            }
                        
                        """
                    )
                )
            );

            read = $$"""
                    return reader.ReadByte() switch {
                        {{string.Join(
                            "\n            ",
                            fieldNames.Select((name, i) =>
                                $"{i} => new {name}({name}.Read(reader)),"
                            )
                        )}}
                        _ => throw new System.InvalidOperationException("Invalid tag value, this state should be unreachable.")
                    };
            """;

            write = $$"""
            switch (value) {
            {{string.Join(
                "\n",
                fieldNames.Select((name, i) => $"""
                            case {name}(var inner):
                                writer.Write((byte){i});
                                {name}.Write(writer, inner);
                                break;
                """))}}
                        }
            """;

            getHashCode = $$"""
            switch (this) {
            {{string.Join(
                    "\n",
                    bsatnDecls
                    .Select(member =>
                    {
                        var hashName = $"___hash{member.Name}";

                        return $"""
                                case {member.Name}(var inner):
                                    {member.Type.GetHashCodeStatement("inner", hashName)}
                                    return {hashName};
                        """;
                    }))}}
                    default:
                        return 0;
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
                    fieldNames.Select(name => $"        {name} = BSATN.{name}.Read(reader);")
                )}}
                }

                public void WriteFields(System.IO.BinaryWriter writer) {
            {{string.Join(
                    "\n",
                    fieldNames.Select(name => $"        BSATN.{name}.Write(writer, {name});")
                )}}
                }

                object SpacetimeDB.BSATN.IStructuralReadWrite.GetSerializer() {
                    return new BSATN();
                }

            """
            );

            // escaping hell
            var start = "{{";
            var end = "}}";

            extensions.Contents.Append(
                $$"""
                public override string ToString() =>
                    $"{{ShortName}} {{start}} {{string.Join(
                        ", ",
                        fieldNames.Select(name => $$"""{{name}} = {SpacetimeDB.BSATN.StringUtil.GenericToString({{name}})}""")
                    )}} {{end}}";
            """
            );

            // Directly allocating the result object here (instead of calling e.g. IStructuralReadWrite.Read<T>, which does the same thing)
            // avoids generics; we've found that generics often result in reflective code being generated.
            // Using simple code here hopefully helps IL2CPP and Mono do this faster.
            read = $$"""
                    var ___result = new {{FullName}}();
                    ___result.ReadFields(reader);
                    return ___result;
                """;

            write = "value.WriteFields(writer);";

            var declHashName = (MemberDeclaration decl) => $"___hash{decl.Name}";

            getHashCode = $$"""
                {{string.Join("\n", bsatnDecls.Select(decl => decl.Type.GetHashCodeStatement(decl.Name, declHashName(decl))))}}
                return {{JoinOrValue(
                    " ^\n            ",
                    bsatnDecls.Select(declHashName),
                    "0" // if there are no members, the hash is 0.
                )}};
                """;
        }

        extensions.Contents.Append(
            $$"""

                public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<{{FullName}}>
                {
                    {{MemberDeclaration.GenerateBsatnFields(Accessibility.Internal, bsatnDecls)}}

                    public {{FullName}} Read(System.IO.BinaryReader reader) {
                        {{read}}
                    }

                    public void Write(System.IO.BinaryWriter writer, {{FullName}} value) {
                        {{write}}
                    }

                    public SpacetimeDB.BSATN.AlgebraicType.Ref GetAlgebraicType(SpacetimeDB.BSATN.ITypeRegistrar registrar) =>
                        registrar.RegisterType<{{FullName}}>(_ => new SpacetimeDB.BSATN.AlgebraicType.{{Kind}}(new SpacetimeDB.BSATN.AggregateElement[] {
                            {{MemberDeclaration.GenerateDefs(Members)}}
                        }));

                    SpacetimeDB.BSATN.AlgebraicType SpacetimeDB.BSATN.IReadWrite<{{FullName}}>.GetAlgebraicType(SpacetimeDB.BSATN.ITypeRegistrar registrar) =>
                        GetAlgebraicType(registrar);
                }
                
                public override int GetHashCode()
                {
                    {{getHashCode}}
                }
            
            """
        );

        if (!Scope.IsRecord)
        {
            // If we are a reference type, various equality methods need to take nullable references.
            // If we are a value type, everything is pleasantly by-value.
            var fullNameMaybeRef = $"{FullName}{(Scope.IsStruct ? "" : "?")}";
            var declEqualsName = (MemberDeclaration decl) => $"___eq{decl.Name}";

            extensions.Contents.Append(
                $$"""

            #nullable enable
                public bool Equals({{fullNameMaybeRef}} that)
                {
                    {{(Scope.IsStruct ? "" : "if (((object?)that) == null) { return false; }\n        ")}}
                    {{string.Join("\n", bsatnDecls.Select(decl => decl.Type.EqualsStatement($"this.{decl.Name}", $"that.{decl.Name}", declEqualsName(decl))))}}
                    return {{JoinOrValue(
                        " &&\n        ",
                        bsatnDecls.Select(declEqualsName),
                        "true" // if there are no elements, the structs are equal :)
                    )}};
                }

                public override bool Equals(object? that) {
                    if (that == null) {
                        return false;
                    }
                    var that_ = that as {{FullName}}{{(Scope.IsStruct ? "?" : "")}};
                    if (((object?)that_) == null) {
                        return false;
                    }
                    return Equals(that_);
                }

                public static bool operator == ({{fullNameMaybeRef}} this_, {{fullNameMaybeRef}} that) {
                    if (((object?)this_) == null || ((object?)that) == null) {
                        return object.Equals(this_, that);
                    }
                    return this_.Equals(that);
                }

                public static bool operator != ({{fullNameMaybeRef}} this_, {{fullNameMaybeRef}} that) {
                    if (((object?)this_) == null || ((object?)that) == null) {
                        return !object.Equals(this_, that);
                    }
                    return !this_.Equals(that);
                }
            #nullable restore
            """
            );
        }

        return extensions;
    }
}

record TypeDeclaration : BaseTypeDeclaration<MemberDeclaration>
{
    public TypeDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
        : base(context, diag) { }

    protected override MemberDeclaration ConvertMember(
        int index,
        IFieldSymbol field,
        DiagReporter diag
    ) => new(field, diag);
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
