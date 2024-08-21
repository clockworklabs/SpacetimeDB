namespace SpacetimeDB.Codegen;

using System.Collections;
using System.Collections.Generic;
using System.Collections.Immutable;
using System.Linq;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static System.Collections.StructuralComparisons;

public static class Utils
{
    static readonly StringBuilder sb = new();

    public static StringBuilder StringBuilder()
    {
        sb.Clear();
        return sb;
    }

    public static readonly SymbolDisplayFormat fmt =
        new(
            globalNamespaceStyle: SymbolDisplayGlobalNamespaceStyle.Included,
            typeQualificationStyle: SymbolDisplayTypeQualificationStyle.NameAndContainingTypesAndNamespaces,
            memberOptions: SymbolDisplayMemberOptions.IncludeContainingType,
            genericsOptions: SymbolDisplayGenericsOptions.IncludeTypeParameters
        );

    public static bool Always(SyntaxNode _sn, CancellationToken _ct) => true;

    public static string Bool(bool x) => x ? "true" : "false";

    public static ITypeSymbol Generic(ITypeSymbol sym, int idx)
    {
        if (sym is INamedTypeSymbol named)
        {
            return named.TypeArguments[idx];
        }
        else
        {
            throw new InvalidOperationException(
                $"Failed to get generic type argument {idx} for type {sym.ToDisplayString()}"
            );
        }
    }

    public static void CollectType(HashSet<ITypeSymbol> syms, ITypeSymbol sym)
    {
        if (syms.Contains(sym))
        {
            return;
        }

        syms.Add(sym);

        foreach (var f in sym.GetMembers().OfType<IFieldSymbol>())
        {
            CollectType(syms, f.Type);
        }

        if (sym.BaseType?.OriginalDefinition.ToString() == "SpacetimeDB.TaggedEnum<Variants>")
        {
            foreach (var f in GetSumElements(sym))
            {
                CollectType(syms, f.Type);
            }
        }

        switch (sym)
        {
            case IArrayTypeSymbol array:
                CollectType(syms, array.ElementType);
                break;
            case INamedTypeSymbol named:
                foreach (var t in named.TypeArguments)
                {
                    CollectType(syms, t);
                }
                break;
        }
    }

    public class AnalyzedType
    {
        public required ITypeSymbol sym;
        public required int idx;
        public required string fqn;
        public required string alg;
        public required string var;
        public required string? prim;
        public required TypeKind kind;
        public required bool isOpt;
        public required bool isInt;
        public required bool isEq;
    }

    public enum TypeKind
    {
        Prim,
        Enum,
        Builtin,
        Option,
        Array,
        List,
        Map,
        Sum,
        Product,
    }

    public static Dictionary<ISymbol, AnalyzedType> AnalyzeTypes(HashSet<ITypeSymbol> syms)
    {
        if (syms.Count == 0)
        {
            throw new Exception("No types found in module.");
        }

        var refOffset = 0; // Used to align indexes for type references and type aliases.
        var types = syms.Select(
                (sym, idx) =>
                {
                    var od = sym.OriginalDefinition.ToString();

                    var prim = sym.SpecialType switch
                    {
                        SpecialType.System_String => "String",
                        SpecialType.System_Boolean => "Bool",
                        SpecialType.System_SByte => "I8",
                        SpecialType.System_Byte => "U8",
                        SpecialType.System_Int16 => "I16",
                        SpecialType.System_UInt16 => "U16",
                        SpecialType.System_Int32 => "I32",
                        SpecialType.System_UInt32 => "U32",
                        SpecialType.System_Int64 => "I64",
                        SpecialType.System_UInt64 => "U64",
                        SpecialType.System_Single => "F32",
                        SpecialType.System_Double => "F64",
                        SpecialType.None => od switch
                        {
                            "System.Int128" => "I128",
                            "System.UInt128" => "U128",
                            "SpacetimeDB.I128" => "I128Stdb",
                            "SpacetimeDB.U128" => "U128Stdb",
                            "SpacetimeDB.I256" => "I256",
                            "SpacetimeDB.U256" => "U256",
                            _ => null,
                        },
                        _ => null,
                    };

                    var builtin = od is "SpacetimeDB.Address" or "SpacetimeDB.Identity";
                    if (builtin)
                    {
                        prim = sym.Name;
                    }

                    var isInt = sym.SpecialType switch
                    {
                        SpecialType.System_SByte
                        or SpecialType.System_Byte
                        or SpecialType.System_Int16
                        or SpecialType.System_UInt16
                        or SpecialType.System_Int32
                        or SpecialType.System_UInt32
                        or SpecialType.System_Int64
                        or SpecialType.System_UInt64 => true,
                        SpecialType.None => od
                            is "System.Int128"
                                or "System.UInt128"
                                or "SpacetimeDB.I128"
                                or "SpacetimeDB.U128"
                                or "SpacetimeDB.I256"
                                or "SpacetimeDB.U256",
                        _ => false,
                    };

                    var isOpt = sym.NullableAnnotation == NullableAnnotation.Annotated;

                    var isEq =
                        (
                            isInt
                            || sym.SpecialType switch
                            {
                                SpecialType.System_Boolean or SpecialType.System_String => true,
                                SpecialType.None => builtin,
                                _ => false,
                            }
                        ) && !isOpt;

                    var kind =
                        isOpt && sym.IsValueType ? TypeKind.Option
                        : builtin ? TypeKind.Builtin
                        : prim != null ? TypeKind.Prim
                        : od switch
                        {
                            "System.Collections.Generic.List<T>" => TypeKind.List,
                            "System.Collections.Generic.Dictionary<TKey, TValue>" => TypeKind.Map,
                            _ => sym.BaseType?.OriginalDefinition.ToString() switch
                            {
                                "System.Enum" => TypeKind.Enum,
                                "SpacetimeDB.TaggedEnum<Variants>" => TypeKind.Sum,
                                _ => sym is IArrayTypeSymbol ? TypeKind.Array : TypeKind.Product,
                            },
                        };

                    var prevRefOffset = refOffset;
                    if (!(kind is TypeKind.Enum or TypeKind.Sum or TypeKind.Product))
                        refOffset++;

                    return new AnalyzedType()
                    {
                        sym = sym,
                        idx = idx - prevRefOffset,
                        fqn = sym.ToDisplayString(fmt),
                        alg = "",
                        var = "",
                        prim = prim,
                        kind = kind,
                        isOpt = isOpt && !sym.IsValueType,
                        isInt = isInt,
                        isEq = isEq,
                    };
                }
            )
            .ToDictionary(x => x.sym, SymbolEqualityComparer.Default);

        foreach (var t in types.Values)
        {
            if (t.prim != null && !t.isOpt)
            {
                t.var = t.prim;
            }
            else
            {
                var sb = StringBuilder();
                EmitVar(types!, t, sb);
                t.var = sb.ToString();
            }

            t.alg = t.isOpt
                ? t.var
                : t.kind switch
                {
                    TypeKind.Prim or TypeKind.Builtin =>
                        $"global::SpacetimeDB.BSATN.AlgebraicTypes.{t.prim}",
                    TypeKind.Enum or TypeKind.Sum or TypeKind.Product => $"{t.var}Ref",
                    _ => t.var,
                };
        }

        return types!;
    }

    public static ImmutableArray<IFieldSymbol> GetSumElements(ITypeSymbol sym)
    {
        if (
            sym.BaseType!.TypeArguments[0]
            is not INamedTypeSymbol { IsTupleType: true, TupleElements: var elems }
        )
        {
            throw new Exception("TaggedUnion must have a tuple type as its type argument.");
        }

        if (sym.GetMembers().OfType<IFieldSymbol>().Any())
        {
            throw new Exception("TaggedUnion cannot have fields.");
        }

        return elems;
    }

    public static string ResolveBSATN(
        IReadOnlyDictionary<ISymbol, AnalyzedType> types,
        ITypeSymbol sym
    )
    {
        var sb = StringBuilder();
        ResolveBSATN(types, sym, sb);
        return sb.ToString();
    }

    static void ResolveBSATN(
        IReadOnlyDictionary<ISymbol, AnalyzedType> types,
        ITypeSymbol sym,
        StringBuilder sb
    )
    {
        var t = types[sym];
        if (t.isOpt)
        {
            sb.Append("global::SpacetimeDB.BSATN.RefOption<");
            sb.Append(t.fqn);
            sb.Append(", ");
        }

        sb.Append(
            t.kind switch
            {
                TypeKind.Prim => $"global::SpacetimeDB.BSATN.{t.prim}",
                TypeKind.Builtin => $"global::SpacetimeDB.{t.sym.Name}.BSATN",
                TypeKind.Enum => $"global::SpacetimeDB.BSATN.Enum<{t.fqn}>",
                TypeKind.Option => "global::SpacetimeDB.BSATN.ValueOption",
                TypeKind.Array
                    when sym is IArrayTypeSymbol { ElementType: var elem }
                        && elem.SpecialType == SpecialType.System_Byte =>
                    "global::SpacetimeDB.BSATN.ByteArray",
                TypeKind.Array => "global::SpacetimeDB.BSATN.Array",
                TypeKind.List => "global::SpacetimeDB.BSATN.List",
                TypeKind.Map => "global::SpacetimeDB.BSATN.Dictionary",
                TypeKind.Sum or TypeKind.Product => $"{t.fqn}.BSATN",
                _ => throw new InvalidDataException(
                    $"Failed to resolve BSATN type for {types[sym].fqn}"
                ),
            }
        );

        switch (sym)
        {
            case IArrayTypeSymbol { ElementType: var elem }
                when elem.SpecialType != SpecialType.System_Byte:
                sb.Append('<');
                sb.Append(types[elem].fqn);
                sb.Append(", ");
                ResolveBSATN(types, elem, sb);
                sb.Append('>');
                break;
            case INamedTypeSymbol named when !named.TypeArguments.IsEmpty:
                sb.Append('<');
                bool first = true;
                foreach (var a in named.TypeArguments)
                {
                    if (first)
                        first = false;
                    else
                        sb.Append(", ");
                    sb.Append(types[a].fqn);
                }
                foreach (var a in named.TypeArguments)
                {
                    sb.Append(", ");
                    ResolveBSATN(types, a, sb);
                }
                sb.Append('>');
                break;
        }

        if (t.isOpt)
        {
            sb.Append('>');
        }
    }

    static void EmitVar(
        IReadOnlyDictionary<ISymbol, AnalyzedType> types,
        AnalyzedType t,
        StringBuilder sb
    )
    {
        if (t.isOpt)
        {
            sb.Append("RefOpt__");
        }

        switch (t.kind)
        {
            case TypeKind.Prim:
                sb.Append(t.prim);
                break;
            case TypeKind.Builtin:
            case TypeKind.Enum:
                sb.Append(t.sym.Name);
                break;
            case TypeKind.Option:
                sb.Append("ValOpt_");
                break;
            case TypeKind.Array:
                sb.Append("Arr_");
                break;
            case TypeKind.List:
                sb.Append("List_");
                break;
            case TypeKind.Map:
                sb.Append("Map_");
                break;
            case TypeKind.Sum:
            case TypeKind.Product:
                sb.Append(t.sym.Name);
                break;
        }
        ;

        switch (t.sym)
        {
            case IArrayTypeSymbol { ElementType: var elem }:
                sb.Append('_');
                EmitVar(types, types[elem], sb);
                break;
            case INamedTypeSymbol named when !named.TypeArguments.IsEmpty:
                foreach (var a in named.TypeArguments)
                {
                    sb.Append('_');
                    EmitVar(types, types[a], sb);
                }
                break;
        }
    }

    // Even `ImmutableArray<T>` is not deeply equatable, which makes it a common
    // pain point for source generators as they must use only cacheable types.
    // As a result, everyone builds their own `EquatableArray<T>` type.
    public readonly record struct EquatableArray<T>(ImmutableArray<T> Array) : IEnumerable<T>
        where T : IEquatable<T>
    {
        public int Length => Array.Length;
        public T this[int index] => Array[index];

        public bool Equals(EquatableArray<T> other) => Array.SequenceEqual(other.Array);

        public override int GetHashCode() => StructuralEqualityComparer.GetHashCode(Array);

        public IEnumerator<T> GetEnumerator() => ((IEnumerable<T>)Array).GetEnumerator();

        IEnumerator IEnumerable.GetEnumerator() => ((IEnumerable)Array).GetEnumerator();
    }

    private static readonly SymbolDisplayFormat SymbolFormat = SymbolDisplayFormat
        .FullyQualifiedFormat.WithGlobalNamespaceStyle(SymbolDisplayGlobalNamespaceStyle.Omitted)
        .AddMemberOptions(SymbolDisplayMemberOptions.IncludeContainingType)
        .AddMiscellaneousOptions(
            SymbolDisplayMiscellaneousOptions.IncludeNullableReferenceTypeModifier
        );

    public static string SymbolToName(ISymbol symbol)
    {
        return symbol.ToDisplayString(SymbolFormat);
    }

    public static void RegisterSourceOutputs(
        this IncrementalValuesProvider<KeyValuePair<string, string>> methods,
        IncrementalGeneratorInitializationContext context
    )
    {
        context.RegisterSourceOutput(
            methods,
            (context, method) =>
            {
                context.AddSource(
                    $"{string.Join("_", method.Key.Split(Path.GetInvalidFileNameChars()))}.cs",
                    $"""
                    // <auto-generated />
                    #nullable enable

                    {method.Value}
                    """
                );
            }
        );
    }

    public static string MakeRwTypeParam(string typeParam) => typeParam + "RW";

    public static string GetTypeInfo(ITypeSymbol type)
    {
        // We need to distinguish handle nullable reference types specially:
        // compiler expands something like `int?` to `System.Nullable<int>` with the nullable annotation set to `Annotated`
        // while something like `string?` is expanded to `string` with the nullable annotation set to `Annotated`...
        // Beautiful design requires beautiful hacks.
        if (
            type.NullableAnnotation == NullableAnnotation.Annotated
            && type.OriginalDefinition.SpecialType != SpecialType.System_Nullable_T
        )
        {
            // If we're here, then this is a nullable reference type like `string?` and the original definition is `string`.
            type = type.WithNullableAnnotation(NullableAnnotation.None);
            return $"SpacetimeDB.BSATN.RefOption<{type}, {GetTypeInfo(type)}>";
        }
        return type switch
        {
            ITypeParameterSymbol typeParameter => MakeRwTypeParam(typeParameter.Name),
            INamedTypeSymbol namedType => type.SpecialType switch
            {
                SpecialType.System_Boolean => "SpacetimeDB.BSATN.Bool",
                SpecialType.System_SByte => "SpacetimeDB.BSATN.I8",
                SpecialType.System_Byte => "SpacetimeDB.BSATN.U8",
                SpecialType.System_Int16 => "SpacetimeDB.BSATN.I16",
                SpecialType.System_UInt16 => "SpacetimeDB.BSATN.U16",
                SpecialType.System_Int32 => "SpacetimeDB.BSATN.I32",
                SpecialType.System_UInt32 => "SpacetimeDB.BSATN.U32",
                SpecialType.System_Int64 => "SpacetimeDB.BSATN.I64",
                SpecialType.System_UInt64 => "SpacetimeDB.BSATN.U64",
                SpecialType.System_Single => "SpacetimeDB.BSATN.F32",
                SpecialType.System_Double => "SpacetimeDB.BSATN.F64",
                SpecialType.System_String => "SpacetimeDB.BSATN.String",
                SpecialType.None => GetTypeInfoForNamedType(namedType),
                _ => throw new InvalidOperationException(
                    $"Unsupported special type {type} ({type.SpecialType})"
                ),
            },
            IArrayTypeSymbol { ElementType: var elementType } => elementType.SpecialType
            == SpecialType.System_Byte
                ? "SpacetimeDB.BSATN.ByteArray"
                : $"SpacetimeDB.BSATN.Array<{elementType}, {GetTypeInfo(elementType)}>",
            _ => throw new InvalidOperationException($"Unsupported type {type}"),
        };

        static string GetTypeInfoForNamedType(INamedTypeSymbol type)
        {
            if (type.TypeKind == Microsoft.CodeAnalysis.TypeKind.Error)
            {
                throw new InvalidOperationException($"Could not resolve type {type}");
            }
            if (type.TypeKind == Microsoft.CodeAnalysis.TypeKind.Enum)
            {
                if (
                    !type.GetAttributes()
                        .Any(a => a.AttributeClass?.ToString() == "SpacetimeDB.TypeAttribute")
                )
                {
                    throw new InvalidOperationException(
                        $"Enum {type} does not have a [SpacetimeDB.Type] attribute"
                    );
                }
                return $"SpacetimeDB.BSATN.Enum<{SymbolToName(type)}>";
            }
            var result = type.OriginalDefinition.ToString() switch
            {
                // {U/I}{128/256} are not treated by C# as regular primitives, so we need to match them by type name.
                "System.Int128" => "SpacetimeDB.BSATN.I128",
                "System.UInt128" => "SpacetimeDB.BSATN.U128",
                "SpacetimeDB.I128" => "SpacetimeDB.BSATN.I128Stdb",
                "SpacetimeDB.U128" => "SpacetimeDB.BSATN.U128Stdb",
                "SpacetimeDB.I256" => "SpacetimeDB.BSATN.I256",
                "SpacetimeDB.U256" => "SpacetimeDB.BSATN.U256",
                "System.Collections.Generic.List<T>" => $"SpacetimeDB.BSATN.List",
                "System.Collections.Generic.Dictionary<TKey, TValue>" =>
                    $"SpacetimeDB.BSATN.Dictionary",
                // If we're here, then this is nullable *value* type like `int?`.
                "System.Nullable<T>" => $"SpacetimeDB.BSATN.ValueOption",
                var name when name.StartsWith("System.") => throw new InvalidOperationException(
                    $"Unsupported system type {name}"
                ),
                _ => $"{SymbolToName(type)}.BSATN",
            };
            if (type.IsGenericType)
            {
                result =
                    $"{result}<{string.Join(", ", type.TypeArguments.Select(SymbolToName).Concat(type.TypeArguments.Select(GetTypeInfo)))}>";
            }

            return result;
        }
    }

    public static IEnumerable<IFieldSymbol> GetFields(
        TypeDeclarationSyntax typeSyntax,
        INamedTypeSymbol type
    )
    {
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
        return typeSyntax
            .Members.OfType<FieldDeclarationSyntax>()
            .SelectMany(f => f.Declaration.Variables)
            .SelectMany(v => type.GetMembers(v.Identifier.Text))
            .OfType<IFieldSymbol>()
            .Where(f => !f.IsStatic);
    }

    // Borrowed & modified code for generating in-place extensions for partial structs/classes/etc. Source:
    // https://andrewlock.net/creating-a-source-generator-part-5-finding-a-type-declarations-namespace-and-type-hierarchy/

    public readonly record struct Scope
    {
        // Reversed list of typescopes, from innermost to outermost.
        private readonly EquatableArray<TypeScope> typeScopes;

        // Reversed list of namespaces, from innermost to outermost.
        private readonly EquatableArray<string> namespaces;

        public Scope(MemberDeclarationSyntax? node)
        {
            var typeScopes_ = ImmutableArray.CreateBuilder<TypeScope>();
            // Keep looping while we're in a supported nested type
            while (node is TypeDeclarationSyntax type)
            {
                // Record the parent type keyword (class/struct etc), name, and constraints
                typeScopes_.Add(
                    new TypeScope(
                        Keyword: type.Keyword.ValueText,
                        Name: type.Identifier.ToString() + type.TypeParameterList,
                        Constraints: type.ConstraintClauses.ToString()
                    )
                ); // set the child link (null initially)

                // Move to the next outer type
                node = type.Parent as MemberDeclarationSyntax;
            }
            typeScopes = new(typeScopes_.ToImmutable());

            // We've now reached the outermost type, so we can determine the namespace
            var namespaces_ = ImmutableArray.CreateBuilder<string>();
            while (node is BaseNamespaceDeclarationSyntax ns)
            {
                namespaces_.Add(ns.Name.ToString());
                node = node.Parent as MemberDeclarationSyntax;
            }
            namespaces = new(namespaces_.ToImmutable());
        }

        public readonly record struct TypeScope(string Keyword, string Name, string Constraints);

        public string GenerateExtensions(
            string contents,
            string? interface_ = null,
            string? extraAttrs = null
        )
        {
            var sb = new StringBuilder();

            // Join all namespaces into a single namespace statement, starting with the outermost.
            if (namespaces.Length > 0)
            {
                sb.Append("namespace ");
                var first = true;
                foreach (var ns in namespaces.Reverse())
                {
                    if (!first)
                    {
                        sb.Append('.');
                    }
                    first = false;
                    sb.Append(ns);
                }
                sb.AppendLine(" {");
            }

            // Loop through the full parent type hiearchy, starting with the outermost.
            foreach (var (i, typeScope) in typeScopes.Select((ts, i) => (i, ts)).Reverse())
            {
                if (i == 0 && extraAttrs is not null)
                {
                    sb.AppendLine(extraAttrs);
                }

                sb.Append("partial ")
                    .Append(typeScope.Keyword) // e.g. class/struct/record
                    .Append(' ')
                    .Append(typeScope.Name) // e.g. Outer/Generic<T>
                    .Append(' ');

                if (i == 0 && interface_ is not null)
                {
                    sb.Append(" : ").Append(interface_);
                }

                sb.Append(typeScope.Constraints).AppendLine(" {");
            }

            sb.AppendLine();
            sb.Append(contents);
            sb.AppendLine();

            // We need to "close" each of the parent types, so write
            // the required number of '}'
            foreach (var typeScope in typeScopes)
            {
                sb.Append("} // ").AppendLine(typeScope.Name);
            }

            // Close the namespace, if we had one
            if (namespaces.Length > 0)
            {
                sb.AppendLine("} // namespace");
            }

            return sb.ToString();
        }
    }
}
