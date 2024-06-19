namespace SpacetimeDB.Codegen.Utils;

using System.Collections;
using System.Collections.Immutable;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using static System.Collections.StructuralComparisons;

// Even `ImmutableArray<T>` is not deeply equatable, which makes it a common
// pain point for source generators as they must use only cacheable types.
// As a result, everyone builds their own `EquatableArray<T>` type.
public record EquatableCollection<T, C> : IReadOnlyList<T>
    where T : IEquatable<T>
    where C : IReadOnlyList<T>, new()
{
    protected readonly C Collection;

    public EquatableCollection() => Collection = new();

    public EquatableCollection(C arr) => Collection = arr;

    public int Count => Collection.Count;
    public T this[int index] => Collection[index];

    public virtual bool Equals(EquatableCollection<T, C> other) =>
        Collection.SequenceEqual(other.Collection);

    public override int GetHashCode() => StructuralEqualityComparer.GetHashCode(Collection);

    public IEnumerator<T> GetEnumerator() => Collection.GetEnumerator();

    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
}

public record EquatableArray<T> : EquatableCollection<T, ImmutableArray<T>>
    where T : IEquatable<T>
{
    public EquatableArray(ImmutableArray<T> arr)
        : base(arr) { }
}

public record NamedItem(string Name)
{
    public sealed override string ToString() => Name;
}

public record MemberDeclaration : NamedItem
{
    public readonly ResolvedType Type;

    public MemberDeclaration(string name, ITypeSymbol typeSymbol)
        : base(name)
    {
        Type = new(typeSymbol);
    }
}

public abstract record SourceOutput : NamedItem
{
    public readonly Scope Scope;
    public readonly Diagnostics Diagnostics = new();

    public SourceOutput(Scope scope, ISymbol symbol)
        : base(symbol.Name)
    {
        Scope = scope;
    }

    protected virtual string? BaseClassesOrInterfaces => null;

    public abstract string ToExtensions();

    public void Emit(SourceProductionContext context)
    {
        Diagnostics.Emit(context);

        var result = Scope.GenerateExtensions(ToExtensions(), BaseClassesOrInterfaces);
        context.AddSource(result.Key, result.Value);
    }
}

public readonly record struct ResolvedType
{
    public readonly string FullName;
    public readonly string BSATN;

    public ResolvedType(ITypeSymbol type)
    {
        FullName = type.GetFullName();

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
            BSATN = $"SpacetimeDB.BSATN.RefOption{ExpandGenerics([type])}";
            return;
        }
        BSATN = type switch
        {
            ITypeParameterSymbol typeParameter => MakeRwTypeParam(typeParameter.Name),
            INamedTypeSymbol namedType
                => type.SpecialType switch
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
                    _
                        => throw new InvalidOperationException(
                            $"Unsupported special type {type} ({type.SpecialType})"
                        )
                },
            IArrayTypeSymbol { ElementType: var elementType } => GetTypeInfoForArrayOf(elementType),
            _ => throw new InvalidOperationException($"Unsupported type {type}")
        };
    }

    public override string ToString() => FullName;

    private static string MakeRwTypeParam(string typeParam) => typeParam + "RW";

    private static string ExpandGenerics(IEnumerable<ITypeSymbol> typeArgs)
    {
        if (!typeArgs.Any())
        {
            return "";
        }
        var typeInfos = typeArgs.Select(arg => new ResolvedType(arg)).ToList();
        return $"<{Enumerable.Concat(typeInfos.Select(t => t.FullName), typeInfos.Select(t => t.BSATN)).Join(", ")}>";
    }

    private static string GetTypeInfoForArrayOf(ITypeSymbol elementType)
    {
        if (elementType.SpecialType == SpecialType.System_Byte)
        {
            return "SpacetimeDB.BSATN.ByteArray";
        }
        return $"SpacetimeDB.BSATN.Array{ExpandGenerics([elementType])}";
    }

    private static string GetTypeInfoForNamedType(INamedTypeSymbol type)
    {
        if (type.TypeKind == TypeKind.Error)
        {
            throw new InvalidOperationException($"Could not resolve type {type}");
        }
        if (type.TypeKind == TypeKind.Enum)
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
            return $"SpacetimeDB.BSATN.Enum<{type.GetFullName()}>";
        }
        var nonGenericType = type.OriginalDefinition.ToString() switch
        {
            // (U)Int128 are not treated by C# as regular primitives, so we need to match them by type name.
            "System.Int128" => "SpacetimeDB.BSATN.I128",
            "System.UInt128" => "SpacetimeDB.BSATN.U128",
            "System.Collections.Generic.List<T>" => $"SpacetimeDB.BSATN.List",
            "System.Collections.Generic.Dictionary<TKey, TValue>"
                => $"SpacetimeDB.BSATN.Dictionary",
            // If we're here, then this is nullable *value* type like `int?`.
            "System.Nullable<T>" => $"SpacetimeDB.BSATN.ValueOption",
            var name when name.StartsWith("System.")
                => throw new InvalidOperationException($"Unsupported system type {name}"),
            _ => $"{type.GetFullName()}.BSATN"
        };
        return $"{nonGenericType}{ExpandGenerics(type.TypeArguments)}";
    }
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

    private static readonly char[] InvalidFilenameChars = Path.GetInvalidFileNameChars();

    public KeyValuePair<string, string> GenerateExtensions(
        string contents,
        string? interface_ = null
    )
    {
        var contentsBuilder = new StringBuilder();
        contentsBuilder.AppendLine("#nullable enable");
        contentsBuilder.AppendLine();

        var filenameBuilder = new StringBuilder();

        var firstPart = true;

        // Join all namespaces into a single namespace statement, starting with the outermost.
        if (namespaces.Count > 0)
        {
            contentsBuilder.Append("namespace ");
            foreach (var ns in namespaces.Reverse())
            {
                if (!firstPart)
                {
                    contentsBuilder.Append('.');
                    filenameBuilder.Append('.');
                }
                firstPart = false;
                contentsBuilder.Append(ns);
                filenameBuilder.Append(ns);
            }
            contentsBuilder.AppendLine(" {");
        }

        // Loop through the full parent type hiearchy, starting with the outermost.
        foreach (var (i, typeScope) in typeScopes.Select((ts, i) => (i, ts)).Reverse())
        {
            contentsBuilder
                .Append("partial ")
                .Append(typeScope.Keyword) // e.g. class/struct/record
                .Append(' ')
                .Append(typeScope.Name) // e.g. Outer/Generic<T>
                .Append(' ');

            if (i == 0 && interface_ is not null)
            {
                contentsBuilder.Append(" : ").Append(interface_);
            }

            contentsBuilder.Append(typeScope.Constraints).AppendLine(" {");

            if (!firstPart)
            {
                filenameBuilder.Append('.');
            }
            firstPart = false;
            filenameBuilder.Append(typeScope.Name);
        }

        contentsBuilder.AppendLine();
        contentsBuilder.Append(contents);
        contentsBuilder.AppendLine();

        // We need to "close" each of the parent types, so write
        // the required number of '}'
        foreach (var typeScope in typeScopes)
        {
            contentsBuilder.Append("} // ").AppendLine(typeScope.Name);
        }

        // Close the namespace, if we had one
        if (namespaces.Count > 0)
        {
            contentsBuilder.AppendLine("} // namespace");
        }
        contents = contentsBuilder.ToString();

        // Finish the hint filename.
        filenameBuilder.Append(".g.cs");
        var filename = filenameBuilder.ToString().Split(InvalidFilenameChars).Join("_");

        return new(filename, contents);
    }
}

public static class Utils
{
    private static readonly SymbolDisplayFormat SymbolFormat = SymbolDisplayFormat
        .FullyQualifiedFormat.WithGlobalNamespaceStyle(SymbolDisplayGlobalNamespaceStyle.Omitted)
        .AddMemberOptions(SymbolDisplayMemberOptions.IncludeContainingType)
        .AddMiscellaneousOptions(
            SymbolDisplayMiscellaneousOptions.IncludeNullableReferenceTypeModifier
        );

    public static string GetFullName(this ISymbol symbol) => symbol.ToDisplayString(SymbolFormat);

    public static IncrementalValuesProvider<TOutput> HandleDerives<TOutput>(
        this IncrementalGeneratorInitializationContext context,
        string fullAttrName,
        Func<GeneratorAttributeSyntaxContext, TOutput> createOutput,
        Func<SyntaxNode, bool>? predicate = null
    )
        where TOutput : SourceOutput
    {
        var parsedOutputs = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: $"{fullAttrName}Attribute",
                predicate: (node, ct) => predicate?.Invoke(node) ?? true,
                transform: (context, ct) => createOutput(context)
            )
            .WithTrackingName($"{fullAttrName}.Parse");

        context.RegisterSourceOutput(parsedOutputs, (context, output) => output.Emit(context));

        return parsedOutputs;
    }

    public static string Join(this IEnumerable<string> source, string separator) =>
        string.Join(separator, source);

    public static string Join<T>(
        this IEnumerable<T> source,
        string separator,
        Func<T, string> convert
    ) => source.Select(convert).Join(separator);
}
