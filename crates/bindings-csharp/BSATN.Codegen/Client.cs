namespace SpacetimeDB.Codegen;

// The client expansion of module declarations.
//
// Client bindings are module declarations without function bodies (proposal 0040): row types with
// [Table], and [Reducer], [Procedure], and [View] methods. In a client build, this generator
// expands them into the client API: `RemoteTables`, `RemoteReducers`, `RemoteProcedures`,
// `DbConnection`, and the types around them. The expansion is the code that `spacetime generate`
// used to write out. Module builds get their expansion from SpacetimeDB.Codegen instead, so this
// generator only expands declarations when the compilation references the client SDK. Without
// either, as in a library that only references SpacetimeDB.BSATN.Runtime, row types still get
// their BSATN implementation.
//
// The module-wide items are emitted once per namespace that contains declarations, which is the
// namespace that `spacetime generate --namespace` chooses.

using System.Collections.Immutable;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using SpacetimeDB.Internal;
using static Utils;

/// <summary>
/// Derives canonical names the way the host does when a module doesn't state them.
/// </summary>
static class CanonicalNames
{
    /// <summary>
    /// Converts a name to snake case, the default <c>CaseConversionPolicy</c>. This mirrors the
    /// <c>convert_case</c> crate with its default word boundaries, which the host uses.
    /// Generated bindings state the canonical names of tables, functions, and columns, so only
    /// parameter names and module source depend on this.
    /// </summary>
    public static string ToSnakeCase(string name)
    {
        var words = new List<string>();
        var word = new StringBuilder();
        void EndWord()
        {
            if (word.Length > 0)
            {
                words.Add(word.ToString().ToLowerInvariant());
                word.Clear();
            }
        }

        for (var i = 0; i < name.Length; i++)
        {
            var c = name[i];
            if (c is '_' or '-' or ' ')
            {
                EndWord();
                continue;
            }
            if (word.Length > 0)
            {
                var prev = name[i - 1];
                var next = i + 1 < name.Length ? name[i + 1] : '\0';
                var isBoundary =
                    // lowerUpper
                    (char.IsLower(prev) && char.IsUpper(c))
                    // letter1, 1letter
                    || (char.IsDigit(prev) != char.IsDigit(c))
                    // ACRONYMWord
                    || (char.IsUpper(prev) && char.IsUpper(c) && char.IsLower(next));
                if (isBoundary)
                {
                    EndWord();
                }
            }
            word.Append(c);
        }
        EndWord();

        return string.Join("_", words);
    }

    /// <summary>
    /// The canonical name of a column: the <c>[DataMember(Name = ...)]</c> that generated
    /// bindings carry, or else the field name in snake case.
    /// </summary>
    public static string ForField(IFieldSymbol field) =>
        field
            .GetAttributes()
            .Where(a =>
                a.AttributeClass?.ToString() == "System.Runtime.Serialization.DataMemberAttribute"
            )
            .SelectMany(a => a.NamedArguments)
            .Where(arg => arg.Key == "Name")
            .Select(arg => arg.Value.Value as string)
            .FirstOrDefault(name => name is not null) ?? ToSnakeCase(field.Name);
}

/// <summary>
/// Type information that the client expansion needs beyond <see cref="TypeUse"/>.
/// </summary>
static class ClientTypes
{
    public static readonly string BTreeAttrName = typeof(Index.BTreeAttribute).FullName.Replace(
        '+',
        '.'
    );

    public static bool IsOption(ITypeSymbol type) =>
        type.NullableAnnotation == NullableAnnotation.Annotated
        || type.OriginalDefinition.SpecialType == SpecialType.System_Nullable_T;

    /// <summary>
    /// The value that a generated parameterless constructor assigns to a field of this type,
    /// matching what <c>spacetime generate</c> wrote, or null if the field needs none.
    /// </summary>
    public static string? DefaultInit(ITypeSymbol type)
    {
        if (IsOption(type) || type.IsValueType)
        {
            return null;
        }
        if (type.SpecialType == SpecialType.System_String)
        {
            return "\"\"";
        }
        if (type is not INamedTypeSymbol named)
        {
            return "null!";
        }
        var original = named.OriginalDefinition.ToString();
        if (original == "System.Collections.Generic.List<T>")
        {
            return "new()";
        }
        if (original is "SpacetimeDB.Result<T, E>" or "SpacetimeDB.Result<T,E>")
        {
            return "default!";
        }
        // Tagged enums, including ScheduleAt, have no default.
        if (named.BaseType?.OriginalDefinition.ToString() == "SpacetimeDB.TaggedEnum<Variants>")
        {
            return "null!";
        }
        return "new()";
    }

    /// <summary>
    /// Whether a column of this type gets a unique index on the client when it is a view's
    /// primary key. This matches the types that <c>spacetime generate</c> filtered on.
    /// </summary>
    public static bool IsFilterable(ITypeSymbol type)
    {
        if (IsOption(type))
        {
            return false;
        }
        return type.SpecialType switch
        {
            SpecialType.System_Boolean
            or SpecialType.System_SByte
            or SpecialType.System_Byte
            or SpecialType.System_Int16
            or SpecialType.System_UInt16
            or SpecialType.System_Int32
            or SpecialType.System_UInt32
            or SpecialType.System_Int64
            or SpecialType.System_UInt64
            or SpecialType.System_String => true,
            _ => type.TypeKind == Microsoft.CodeAnalysis.TypeKind.Enum
                || type.ToString()
                    is "SpacetimeDB.I128"
                        or "SpacetimeDB.U128"
                        or "SpacetimeDB.I256"
                        or "SpacetimeDB.U256"
                        or "SpacetimeDB.Identity"
                        or "SpacetimeDB.ConnectionId",
        };
    }

    public static string NamespaceOf(ISymbol symbol)
    {
        var ns = symbol.ContainingNamespace;
        return ns is null || ns.IsGlobalNamespace ? "" : SymbolToName(ns);
    }
}

/// <summary>A column of a table or view, as the client sees it.</summary>
record ClientColumn(string Name, string Type, string CanonicalName)
{
    public string Identifier => EscapeIdentifier(Name);

    /// <summary>The column type without the <c>?</c> of an option, as the query builder takes it.</summary>
    public string ValueType => Type.EndsWith("?", StringComparison.Ordinal) ? Type[..^1] : Type;
}

/// <summary>A client-side index of a table or view.</summary>
record ClientIndex(string Name, bool IsUnique, EquatableArray<int> Columns);

/// <summary>Everything the client needs to expand one table or view.</summary>
record ClientTable(
    string Namespace,
    string Accessor,
    string CanonicalName,
    string RowFullName,
    bool IsEvent,
    EquatableArray<ClientColumn> Columns,
    EquatableArray<ClientIndex> Indexes,
    int? PrimaryKey
)
{
    public string Identifier => EscapeIdentifier(Accessor);

    /// <summary>
    /// The columns of the query builder's <c>IxCols</c>: indexed columns and the primary key,
    /// which counts even when the client has no index for it, as in <c>spacetime generate</c>.
    /// </summary>
    public IEnumerable<int> IndexedColumns =>
        Indexes
            .SelectMany(index => index.Columns)
            .Concat(PrimaryKey is { } pk ? [pk] : [])
            .Distinct()
            .OrderBy(i => i);
}

readonly record struct ClientColumnAttr(ColumnAttrs Mask, string? Table);

record ClientIndexAttr(string? Table, string Accessor, EquatableArray<int> Columns);

/// <summary>A field of a <c>[Table]</c> row type with its column attributes.</summary>
record ClientColumnDeclaration : MemberDeclaration
{
    public readonly string CanonicalName;
    public readonly EquatableArray<ClientColumnAttr> Attrs;
    public readonly EquatableArray<ClientIndexAttr> Indexes;

    public ClientColumnDeclaration(int index, IFieldSymbol field, DiagReporter diag)
        : base(field, diag)
    {
        CanonicalName = CanonicalNames.ForField(field);

        var attrs = field.GetAttributes();
        Attrs = new(
            attrs
                .Select(a =>
                    a.AttributeClass?.ToString() switch
                    {
                        "SpacetimeDB.PrimaryKeyAttribute" => new ClientColumnAttr(
                            ColumnAttrs.PrimaryKey,
                            a.ParseAs<PrimaryKeyAttribute>().Table
                        ),
                        "SpacetimeDB.UniqueAttribute" => new(
                            ColumnAttrs.Unique,
                            a.ParseAs<UniqueAttribute>().Table
                        ),
                        "SpacetimeDB.AutoIncAttribute" => new(
                            ColumnAttrs.AutoInc,
                            a.ParseAs<AutoIncAttribute>().Table
                        ),
                        _ => default,
                    }
                )
                .Where(a => a.Mask != ColumnAttrs.UnSet)
                .ToImmutableArray()
        );
        Indexes = new(
            attrs
                .Where(a => a.AttributeClass?.ToString() == ClientTypes.BTreeAttrName)
                .Select(a => a.ParseAs<Index.BTreeAttribute>())
                .Select(a => new ClientIndexAttr(
                    a.Table,
                    a.Accessor ?? field.Name,
                    new(ImmutableArray.Create(index))
                ))
                .ToImmutableArray()
        );
    }

    public ColumnAttrs GetAttrs(string tableAccessor) =>
        Attrs
            .Where(a => a.Table is null || a.Table == tableAccessor)
            .Aggregate(ColumnAttrs.UnSet, (mask, a) => mask | a.Mask);
}

/// <summary>A row type with one or more <c>[Table]</c> attributes.</summary>
record ClientTableDeclaration : BaseTypeDeclaration<ClientColumnDeclaration>
{
    public readonly bool HasTypeAttribute;
    public readonly EquatableArray<ClientTable> Tables;

    public ClientTableDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
        : base(context, diag)
    {
        var type = (INamedTypeSymbol)context.TargetSymbol;
        HasTypeAttribute = type.GetAttributes()
            .Any(a => a.AttributeClass?.ToString() == "SpacetimeDB.TypeAttribute");

        // A tagged enum is not a valid table; the module build reports it. A struct is one, but
        // the client cache only stores classes; the generator reports that in client builds.
        if (Kind is TypeKind.Sum || type.IsValueType)
        {
            Tables = new(ImmutableArray<ClientTable>.Empty);
            return;
        }

        var memberNames = Members.Select(m => m.Name).ToList();
        var typeIndexes = new List<ClientIndexAttr>();
        foreach (
            var data in type.GetAttributes()
                .Where(a => a.AttributeClass?.ToString() == ClientTypes.BTreeAttrName)
        )
        {
            var a = data.ParseAs<Index.BTreeAttribute>();
            var indexColumns = a
                .Columns.Select(name => memberNames.IndexOf(name))
                .ToImmutableArray();
            foreach (var name in a.Columns.Where((_, i) => indexColumns[i] < 0))
            {
                diag.Report(ErrorDescriptor.ClientUnknownColumn, (data, name, ShortName));
            }
            if (indexColumns.Length > 0 && indexColumns.All(i => i >= 0))
            {
                typeIndexes.Add(
                    new(a.Table, a.Accessor ?? string.Join("_", a.Columns), new(indexColumns))
                );
            }
        }

        var ns = ClientTypes.NamespaceOf(type);
        var columns = new EquatableArray<ClientColumn>(
            Members
                .Select(m => new ClientColumn(m.Name, m.Type.Name, m.CanonicalName))
                .ToImmutableArray()
        );

        Tables = new(
            context
                .Attributes.Select(a => a.ParseAs<TableAttribute>())
                .Select(attr =>
                {
                    var accessor = attr.Accessor ?? ShortName;
                    var attrs = Members.Select(m => m.GetAttrs(accessor)).ToArray();
                    var declared = typeIndexes
                        .Concat(Members.SelectMany(m => m.Indexes))
                        .Where(i => i.Table is null || i.Table == accessor);
                    var unique = Enumerable
                        .Range(0, Members.Length)
                        .Where(i => attrs[i].HasFlag(ColumnAttrs.Unique));
                    var primaryKey = Enumerable
                        .Range(0, Members.Length)
                        .Where(i => attrs[i].HasFlag(ColumnAttrs.PrimaryKey))
                        .Select(i => (int?)i)
                        .FirstOrDefault();

                    // Same rules as `spacetime generate` used: a declared index over one unique
                    // column is a unique index, and each unique column has a unique index named
                    // after it unless an index of that name already exists.
                    var names = new HashSet<string>();
                    var indexes = new List<ClientIndex>();
                    foreach (var index in declared)
                    {
                        var isUnique =
                            index.Columns.Length == 1 && unique.Contains(index.Columns[0]);
                        if (names.Add(index.Accessor) || !isUnique)
                        {
                            indexes.Add(new(index.Accessor, isUnique, index.Columns));
                        }
                    }
                    foreach (var i in unique)
                    {
                        if (names.Add(Members[i].Name))
                        {
                            indexes.Add(new(Members[i].Name, true, new(ImmutableArray.Create(i))));
                        }
                    }

                    return new ClientTable(
                        ns,
                        accessor,
                        attr.Name ?? CanonicalNames.ToSnakeCase(accessor),
                        FullName,
                        attr.Event,
                        columns,
                        new(indexes.ToImmutableArray()),
                        primaryKey
                    );
                })
                .ToImmutableArray()
        );
    }

    protected override ClientColumnDeclaration ConvertMember(
        int index,
        IFieldSymbol field,
        DiagReporter diag
    ) => new(index, field, diag);
}

/// <summary>A parameter of a reducer or procedure, and the field that stores it.</summary>
record ClientParam(string Name, TypeUse Type, string CanonicalName, string? DefaultInit)
{
    public string Identifier => EscapeIdentifier(Name);

    /// <summary>The field of the arguments class, which is the parameter name in Pascal case.</summary>
    public string FieldName => char.ToUpperInvariant(Name[0]) + Name[1..];

    public MemberDeclaration ToMember() => new(FieldName, Type);
}

/// <summary>
/// The parts of a reducer, procedure, or view declaration that the client expansion shares.
/// </summary>
record ClientFunction
{
    public readonly string Namespace;
    public readonly string Name;
    public readonly EquatableArray<ClientParam> Params;

    /// <summary>The containing type, for the implementing part of a bodiless declaration.</summary>
    public readonly Scope Scope;
    public readonly string FullName;

    /// <summary>The name of the generated source, which tells overloads apart.</summary>
    public readonly string HintName;

    /// <summary>
    /// Whether the declaration expands into the client API. If not, the generator has reported why.
    /// </summary>
    public readonly bool IsValid = true;

    /// <summary>The implementing part of a bodiless declaration, if it needs one.</summary>
    public readonly string? Implementation;

    public string Identifier => EscapeIdentifier(Name);

    public ClientFunction(
        GeneratorAttributeSyntaxContext context,
        DiagReporter diag,
        string kind,
        params string[] contexts
    )
    {
        var methodSyntax = (MethodDeclarationSyntax)context.TargetNode;
        var method = (IMethodSymbol)context.TargetSymbol;

        Namespace = ClientTypes.NamespaceOf(method);
        Name = method.Name;
        FullName = SymbolToName(method);
        Scope = new Scope(methodSyntax.Parent as MemberDeclarationSyntax);

        // The client API has one function of a kind per name, so only the first overload expands.
        var attr = context.Attributes[0].AttributeClass;
        var overload = method
            .ContainingType.GetMembers(method.Name)
            .OfType<IMethodSymbol>()
            .Where(m =>
                m.GetAttributes()
                    .Any(a => SymbolEqualityComparer.Default.Equals(a.AttributeClass, attr))
            )
            .ToList()
            .FindIndex(m =>
                SymbolEqualityComparer.Default.Equals(m, method.PartialDefinitionPart ?? method)
            );
        HintName = overload > 0 ? $"{FullName}.{overload}" : FullName;
        if (overload > 0)
        {
            diag.Report(ErrorDescriptor.ClientOverload, (method, kind));
            IsValid = false;
        }
        foreach (var p in method.Parameters.Where(p => p.RefKind != RefKind.None || p.IsParams))
        {
            diag.Report(ErrorDescriptor.ClientParamModifier, p);
            IsValid = false;
        }
        // As in modules, the first parameter is the context.
        var first = method.Parameters.FirstOrDefault()?.Type.ToString();
        if (!contexts.Any(c => first == $"SpacetimeDB.{c}"))
        {
            diag.Report(
                ErrorDescriptor.ClientContextParam,
                (methodSyntax, string.Join(" or ", contexts))
            );
            IsValid = false;
        }

        Params = new(
            method
                .Parameters.Skip(1)
                .Select(p => new ClientParam(
                    p.Name,
                    TypeUse.Parse(p, p.Type, diag),
                    CanonicalNames.ToSnakeCase(p.Name),
                    ClientTypes.DefaultInit(p.Type)
                ))
                .ToImmutableArray()
        );

        // Bindings declare functions without bodies, as `partial` methods.
        // Clients don't run them, so the implementing part throws.
        if (method.IsPartialDefinition && method.PartialImplementationPart is null)
        {
            var parameters = string.Join(
                ", ",
                method.Parameters.Select(p =>
                    (p.IsParams ? "params " : "")
                    + p.RefKind switch
                    {
                        RefKind.Ref => "ref ",
                        RefKind.Out => "out ",
                        RefKind.In => "in ",
                        _ => "",
                    }
                    + $"{SymbolToName(p.Type)} {EscapeIdentifier(p.Name)}"
                )
            );
            var returnType = method.ReturnsVoid ? "void" : SymbolToName(method.ReturnType);
            Implementation =
                $"{methodSyntax.Modifiers} {returnType} {Identifier}({parameters}) =>\n"
                + $"    throw new global::System.NotSupportedException(\"{Name} is declared for the client bindings and only runs in the module.\");";
        }
    }

    public string ParamList =>
        string.Join(", ", Params.Select(p => $"{p.Type.Name} {p.Identifier}"));

    public string ArgList => string.Join(", ", Params.Select(p => p.Identifier));

    public string ImplementationExtensions()
    {
        if (Implementation is null)
        {
            return "";
        }
        var extensions = new Scope.Extensions(Scope, FullName);
        extensions.Contents.Append(Implementation);
        // Generated bindings declare the functions in a nullable context.
        return "#nullable enable\n" + extensions;
    }
}

record ClientReducerDeclaration
{
    public readonly ClientFunction Function;
    public readonly string CanonicalName;
    public readonly ReducerKind Kind;

    public ClientReducerDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
    {
        Function = new(context, diag, "reducer", "ReducerContext");
        var attr = context.Attributes.Single().ParseAs<ReducerAttribute>();
        CanonicalName = attr.Name ?? CanonicalNames.ToSnakeCase(Function.Name);
        Kind = attr.Kind;
    }

    /// <summary>
    /// Whether the reducer expands into the client API. Like <c>spacetime generate</c>, the client
    /// API leaves out the <c>init</c> reducer.
    /// </summary>
    public bool HasClientApi => Function.IsValid && Kind != ReducerKind.Init;
}

record ClientProcedureDeclaration
{
    public readonly ClientFunction Function;
    public readonly string CanonicalName;
    public readonly TypeUse ReturnType;
    public readonly string? ReturnDefaultInit;

    public ClientProcedureDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
    {
        Function = new(context, diag, "procedure", "ProcedureContext");
        var method = (IMethodSymbol)context.TargetSymbol;
        var attr = context.Attributes.Single().ParseAs<ProcedureAttribute>();
        CanonicalName = attr.Name ?? CanonicalNames.ToSnakeCase(Function.Name);
        ReturnType = TypeUse.Parse(method, method.ReturnType, diag);
        ReturnDefaultInit = method.ReturnsVoid ? null : ClientTypes.DefaultInit(method.ReturnType);
    }
}

record ClientViewDeclaration
{
    public readonly ClientFunction Function;
    public readonly ClientTable? Table;

    public ClientViewDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
    {
        Function = new(context, diag, "view", "ViewContext", "AnonymousViewContext");
        var method = (IMethodSymbol)context.TargetSymbol;
        var attr = context.Attributes.Single().ParseAs<ViewAttribute>();
        var accessor = attr.Accessor ?? Function.Name;

        // A view returns `T?`, `List<T>`, `IEnumerable<T>`, or `IQuery<T>` of its row type `T`.
        var returnType = method.ReturnType;
        var rowType = returnType switch
        {
            INamedTypeSymbol { IsGenericType: true } generic
                when generic.OriginalDefinition.ToString()
                    is "System.Nullable<T>"
                        or "System.Collections.Generic.List<T>"
                        or "System.Collections.Generic.IEnumerable<T>"
                        or "SpacetimeDB.IQuery<TRow>" => generic.TypeArguments[0],
            _ => returnType.WithNullableAnnotation(NullableAnnotation.NotAnnotated),
        };
        if (
            rowType is not INamedTypeSymbol { TypeKind: Microsoft.CodeAnalysis.TypeKind.Class } row
            || !row.GetAttributes()
                .Any(a =>
                    a.AttributeClass?.ToString()
                        is "SpacetimeDB.TypeAttribute"
                            or "SpacetimeDB.TableAttribute"
                )
        )
        {
            diag.Report(
                ErrorDescriptor.ClientViewReturn,
                (MethodDeclarationSyntax)context.TargetNode
            );
            return;
        }

        var fields = SpacetimeDbFieldDiscovery.GetSpacetimeDbFields(row).ToArray();
        var columns = fields
            .Select(f => new ClientColumn(f.Name, SymbolToName(f.Type), CanonicalNames.ForField(f)))
            .ToImmutableArray();
        var primaryKey = attr.PrimaryKey is { } pk
            ? Array.FindIndex(fields, f => f.Name == pk)
            : -1;

        var indexes = ImmutableArray<ClientIndex>.Empty;
        if (primaryKey >= 0 && ClientTypes.IsFilterable(fields[primaryKey].Type))
        {
            indexes = ImmutableArray.Create(
                new ClientIndex(
                    fields[primaryKey].Name,
                    true,
                    new(ImmutableArray.Create(primaryKey))
                )
            );
        }

        Table = new(
            Function.Namespace,
            accessor,
            attr.Name ?? CanonicalNames.ToSnakeCase(accessor),
            SymbolToName(row),
            false,
            new(columns),
            new(indexes),
            primaryKey >= 0 ? primaryKey : null
        );
    }
}

/// <summary>
/// An arguments or return value class that the client expansion emits, with its BSATN implementation.
/// </summary>
record ClientGeneratedType : BaseTypeDeclaration<MemberDeclaration>
{
    public ClientGeneratedType(
        string @namespace,
        string container,
        string name,
        IEnumerable<MemberDeclaration> members
    )
        : base(
            new Scope(
                @namespace,
                new Scope.TypeScope("class", container, ""),
                new Scope.TypeScope("class", EscapeIdentifier(name), "")
            ),
            name,
            (@namespace == "" ? "" : @namespace + ".") + container + "." + EscapeIdentifier(name),
            members.ToImmutableArray()
        ) { }

    protected override MemberDeclaration ConvertMember(
        int index,
        IFieldSymbol field,
        DiagReporter diag
    ) => throw new InvalidOperationException("Generated types have no fields to convert.");
}

/// <summary>The C# text of the client expansion. Ported from the C# output of <c>spacetime generate</c>.</summary>
static class ClientCode
{
    public static string File(string @namespace, string contents)
    {
        var sb = new StringBuilder();
        sb.AppendLine("// <auto-generated />");
        sb.AppendLine("#nullable enable");
        sb.AppendLine();
        sb.AppendLine("using System;");
        sb.AppendLine("using SpacetimeDB;");
        sb.AppendLine("using SpacetimeDB.ClientApi;");
        sb.AppendLine("using System.Collections.Generic;");
        sb.AppendLine("using System.Runtime.Serialization;");
        sb.AppendLine();
        if (@namespace == "")
        {
            sb.Append(contents);
        }
        else
        {
            sb.AppendLine($"namespace {@namespace}");
            sb.AppendLine("{");
            sb.Append(contents);
            sb.AppendLine("}");
        }
        return sb.ToString();
    }

    public static string HintName(string name) =>
        string.Concat(
            name.Select(c => SyntaxFacts.IsIdentifierPartCharacter(c) || c == '.' ? c : '_')
        );

    public static string Table(ClientTable t)
    {
        var row = $"global::{t.RowFullName}";
        var handle = $"{t.Accessor}Handle";
        var baseClass = t.IsEvent ? "RemoteEventTableHandle" : "RemoteTableHandle";
        var cols = t.Columns;

        var indexes = new StringBuilder();
        foreach (var index in t.Indexes)
        {
            var indexColumns = index.Columns.Select(i => cols[i]).ToArray();
            var (keyType, getKey) =
                indexColumns.Length == 1
                    ? (indexColumns[0].Type, $"row.{indexColumns[0].Identifier}")
                    : (
                        $"({string.Join(", ", indexColumns.Select(c => $"{c.Type} {c.Identifier}"))})",
                        $"({string.Join(", ", indexColumns.Select(c => $"row.{c.Identifier}"))})"
                    );
            var indexClass = index.IsUnique ? $"{index.Name}UniqueIndex" : $"{index.Name}Index";
            var indexBase = index.IsUnique ? "UniqueIndexBase" : "BTreeIndexBase";
            indexes.Append(
                $$"""
                            public sealed class {{indexClass}} : {{indexBase}}<{{keyType}}>
                            {
                                protected override {{keyType}} GetKey({{row}} row) => {{getKey}};

                                public {{indexClass}}({{handle}} table) : base(table) { }
                            }

                            public readonly {{indexClass}} {{EscapeIdentifier(index.Name)}};

                
                """
            );
        }

        var indexInits = string.Join(
            "",
            t.Indexes.Select(index =>
                $"                {EscapeIdentifier(index.Name)} = new(this);\n"
            )
        );
        var primaryKey = t.PrimaryKey is { } pk
            ? $"\n            protected override object GetPrimaryKey({row} row) => row.{cols[pk].Identifier};\n"
            : "";

        var ixCols = t.IndexedColumns.Select(i => cols[i]).ToArray();

        return $$"""
                public sealed partial class RemoteTables
                {
                    public sealed class {{handle}} : {{baseClass}}<EventContext, {{row}}>
                    {
                        public override string RemoteTableName => "{{t.CanonicalName}}";

            {{indexes}}            internal {{handle}}(DbConnection conn) : base(conn)
                        {
            {{indexInits}}            }
            {{primaryKey}}        }

                    public readonly {{handle}} {{t.Identifier}};
                }

                public sealed class {{t.Accessor}}Cols
                {
            {{string.Join(
                "",
                cols.Select(c =>
                    $"        public global::SpacetimeDB.Col<{row}, {c.ValueType}> {c.Identifier} {{ get; }}\n"
                )
            )}}
                    public {{t.Accessor}}Cols(string tableName)
                    {
            {{string.Join(
                "",
                cols.Select(c =>
                    $"            {c.Identifier} = new global::SpacetimeDB.Col<{row}, {c.ValueType}>(tableName, \"{c.CanonicalName}\");\n"
                )
            )}}        }
                }

                public sealed class {{t.Accessor}}IxCols
                {
            {{string.Join(
                "",
                ixCols.Select(c =>
                    $"        public global::SpacetimeDB.IxCol<{row}, {c.ValueType}> {c.Identifier} {{ get; }}\n"
                )
            )}}
                    public {{t.Accessor}}IxCols(string tableName)
                    {
            {{string.Join(
                "",
                ixCols.Select(c =>
                    $"            {c.Identifier} = new global::SpacetimeDB.IxCol<{row}, {c.ValueType}>(tableName, \"{c.CanonicalName}\");\n"
                )
            )}}        }
                }

            """;
    }

    /// <summary>The fields and constructors of an arguments or return value class.</summary>
    static string ProductClass(
        string name,
        string? baseList,
        IEnumerable<(string Field, string Type, string CanonicalName, string? DefaultInit)> fields,
        string extra
    )
    {
        var fs = fields.ToArray();
        var sb = new StringBuilder();
        sb.Append(
            $$"""
                    [SpacetimeDB.Type]
                    [DataContract]
                    public sealed partial class {{name}}{{(
                baseList is null ? "" : $" : {baseList}"
            )}}
                    {
            
            """
        );
        foreach (var f in fs)
        {
            sb.Append($"            [DataMember(Name = \"{f.CanonicalName}\")]\n");
            sb.Append($"            public {f.Type} {EscapeIdentifier(f.Field)};\n");
        }
        if (fs.Length > 0)
        {
            sb.Append(
                $$"""

                            public {{name}}({{string.Join(
                    ", ",
                    fs.Select(f => $"{f.Type} {EscapeIdentifier(f.Field)}")
                )}})
                            {
                {{string.Join(
                    "",
                    fs.Select(f =>
                        $"                this.{EscapeIdentifier(f.Field)} = {EscapeIdentifier(f.Field)};\n"
                    )
                )}}            }

                            public {{name}}()
                            {
                {{string.Join(
                    "",
                    fs.Where(f => f.DefaultInit is not null)
                        .Select(f => $"                this.{EscapeIdentifier(f.Field)} = {f.DefaultInit};\n")
                )}}            }

                """
            );
        }
        if (extra != "")
        {
            sb.Append($"\n            {extra}\n");
        }
        sb.Append("        }\n");
        return sb.ToString();
    }

    static IEnumerable<(string, string, string, string?)> ArgFields(ClientFunction f) =>
        f.Params.Select(p => (p.FieldName, p.Type.Name, p.CanonicalName, p.DefaultInit));

    static string Bsatn(
        string @namespace,
        string container,
        string name,
        IEnumerable<MemberDeclaration> members
    ) => new ClientGeneratedType(@namespace, container, name, members).ToExtensions().ToString();

    public static string ReducerFile(ClientReducerDeclaration r)
    {
        var f = r.Function;
        var name = f.Name;
        var separator = f.Params.Length == 0 ? "" : ", ";
        var call =
            r.Kind == ReducerKind.UserDefined
                ? $$"""
                            public void {{f.Identifier}}({{f.ParamList}})
                            {
                                conn.InternalCallReducer(new Reducer.{{f.Identifier}}({{f.ArgList}}));
                            }

                    
                    """
                : "";
        var callbackArgs = string.Concat(
            f.Params.Select(p => $",\n                args.{EscapeIdentifier(p.FieldName)}")
        );

        var code = $$"""
                public sealed partial class RemoteReducers : RemoteBase
                {
                    public delegate void {{name}}Handler(ReducerEventContext ctx{{separator}}{{f.ParamList}});
                    public event {{name}}Handler? On{{name}};

            {{call}}        public bool Invoke{{name}}(ReducerEventContext ctx, Reducer.{{f.Identifier}} args)
                    {
                        if (On{{name}} == null)
                        {
                            if (InternalOnUnhandledReducerError != null)
                            {
                                switch(ctx.Event.Status)
                                {
                                    case Status.Failed(var reason): InternalOnUnhandledReducerError(ctx, new Exception(reason)); break;
                                    case Status.OutOfEnergy(var _): InternalOnUnhandledReducerError(ctx, new Exception("out of energy")); break;
                                }
                            }
                            return false;
                        }
                        On{{name}}(
                            ctx{{callbackArgs}}
                        );
                        return true;
                    }
                }

                public abstract partial class Reducer
                {
            {{ProductClass(
                f.Identifier,
                "Reducer, IReducerArgs",
                ArgFields(f),
                $"string IReducerArgs.ReducerName => \"{r.CanonicalName}\";"
            )}}    }

            """;

        return File(f.Namespace, code)
            + Bsatn(f.Namespace, "Reducer", name, f.Params.Select(p => p.ToMember()));
    }

    public static string ProcedureFile(ClientProcedureDeclaration p)
    {
        var f = p.Function;
        var name = f.Name;
        var separator = f.Params.Length == 0 ? "" : ", ";
        var returnType = p.ReturnType.Name;

        var code = $$"""
                public sealed partial class RemoteProcedures : RemoteBase
                {
                    public void {{f.Identifier}}({{f.ParamList}}{{separator}}ProcedureCallback<{{returnType}}> callback)
                    {
                        // Convert the clean callback to the wrapper callback
                        Internal{{name}}({{f.ArgList}}{{separator}}(ctx, result) => {
                        if (result.IsSuccess && result.Value != null)
                        {
                            callback(ctx, ProcedureCallbackResult<{{returnType}}>.Success(result.Value.Value));
                        }
                        else
                        {
                            callback(ctx, ProcedureCallbackResult<{{returnType}}>.Failure(result.Error!));
                        }
                        });
                    }

                    private void Internal{{name}}({{f.ParamList}}{{separator}}ProcedureCallback<Procedure.{{f.Identifier}}> callback)
                    {
                        conn.InternalCallProcedure(new Procedure.{{name}}Args({{f.ArgList}}), callback);
                    }

                }

                public abstract partial class Procedure
                {
            {{ProductClass(
                f.Identifier,
                null,
                [("Value", returnType, "Value", p.ReturnDefaultInit)],
                ""
            )}}{{ProductClass(
                $"{name}Args",
                "Procedure, IProcedureArgs",
                ArgFields(f),
                $"string IProcedureArgs.ProcedureName => \"{p.CanonicalName}\";"
            )}}
                }

            """;

        return File(f.Namespace, code)
            + Bsatn(f.Namespace, "Procedure", name, [new MemberDeclaration("Value", p.ReturnType)])
            + Bsatn(f.Namespace, "Procedure", $"{name}Args", f.Params.Select(p => p.ToMember()));
    }

    /// <summary>The items that depend on the whole module.</summary>
    public static string Module(
        IEnumerable<ClientTable> tables,
        IEnumerable<ClientReducerDeclaration> reducers
    )
    {
        var ts = tables.OrderBy(t => t.Accessor, StringComparer.Ordinal).ToArray();
        var rs = reducers.OrderBy(r => r.Function.Name, StringComparer.Ordinal).ToArray();

        return $$"""
                public sealed partial class RemoteReducers : RemoteBase
                {
                    internal RemoteReducers(DbConnection conn) : base(conn) { }
                    internal event Action<ReducerEventContext, Exception>? InternalOnUnhandledReducerError;
                }

                public sealed partial class RemoteProcedures : RemoteBase
                {
                    internal RemoteProcedures(DbConnection conn) : base(conn) { }
                }

                public sealed partial class RemoteTables : RemoteTablesBase
                {
                    public RemoteTables(DbConnection conn)
                    {
            {{string.Concat(ts.Select(t => $"            AddTable({t.Identifier} = new(conn));\n"))}}        }
                }

            {{ContextAndSubscriptionTypes}}
                public sealed class QueryBuilder
                {
                    public From From { get; } = new();

                    internal static string[] AllTablesSqlQueries() => new string[]
                    {
            {{string.Concat(ts.Select(t => $"            new QueryBuilder().From.{t.Identifier}().ToSql(),\n"))}}        }
                    ;
                }

                public sealed class From
                {
            {{string.Concat(
                ts.Select(t =>
                    $"        public global::SpacetimeDB.Table<global::{t.RowFullName}, {t.Accessor}Cols, {t.Accessor}IxCols> {t.Identifier}() => new(\"{t.CanonicalName}\", new {t.Accessor}Cols(\"{t.CanonicalName}\"), new {t.Accessor}IxCols(\"{t.CanonicalName}\"));\n"
                )
            )}}    }

                public sealed class TypedSubscriptionBuilder
                {
                    private readonly IDbConnection conn;
                    private Action<SubscriptionEventContext>? Applied;
                    private Action<ErrorContext, Exception>? Error;
                    private readonly List<string> querySqls = new();

                    internal TypedSubscriptionBuilder(IDbConnection conn, Action<SubscriptionEventContext>? applied, Action<ErrorContext, Exception>? error)
                    {
                        this.conn = conn;
                        Applied = applied;
                        Error = error;
                    }

                    public TypedSubscriptionBuilder OnApplied(Action<SubscriptionEventContext> callback)
                    {
                        Applied += callback;
                        return this;
                    }

                    public TypedSubscriptionBuilder OnError(Action<ErrorContext, Exception> callback)
                    {
                        Error += callback;
                        return this;
                    }

                    public TypedSubscriptionBuilder AddQuery<TRow>(Func<QueryBuilder, global::SpacetimeDB.IQuery<TRow>> build)
                    {
                        var qb = new QueryBuilder();
                        querySqls.Add(build(qb).ToSql());
                        return this;
                    }

                    public SubscriptionHandle Subscribe() => new(conn, Applied, Error, querySqls.ToArray());
                }

                public abstract partial class Reducer
                {
                    private Reducer() { }
                }

                public abstract partial class Procedure
                {
                    private Procedure() { }
                }

                public sealed class DbConnection : DbConnectionBase<DbConnection, RemoteTables, Reducer>
                {
                    public override RemoteTables Db { get; }
                    public readonly RemoteReducers Reducers;
                    public readonly RemoteProcedures Procedures;

                    public DbConnection()
                    {
                        Db = new(this);
                        Reducers = new(this);
                        Procedures = new(this);
                    }

                    protected override IEventContext ToEventContext(Event<Reducer> Event) =>
                    new EventContext(this, Event);

                    protected override IReducerEventContext ToReducerEventContext(ReducerEvent<Reducer> reducerEvent) =>
                    new ReducerEventContext(this, reducerEvent);

                    protected override ISubscriptionEventContext MakeSubscriptionEventContext() =>
                    new SubscriptionEventContext(this);

                    protected override IErrorContext ToErrorContext(Exception exception) =>
                    new ErrorContext(this, exception);

                    protected override IProcedureEventContext ToProcedureEventContext(ProcedureEvent procedureEvent) =>
                    new ProcedureEventContext(this, procedureEvent);

                    protected override bool Dispatch(IReducerEventContext context, Reducer reducer)
                    {
                        var eventContext = (ReducerEventContext)context;
                        return reducer switch {
            {{string.Concat(
                rs.Select(r =>
                    $"                Reducer.{r.Function.Identifier} args => Reducers.Invoke{r.Function.Name}(eventContext, args),\n"
                )
            )}}                _ => throw new ArgumentOutOfRangeException("Reducer", $"Unknown reducer {reducer}")
                        };
                    }

                    public SubscriptionBuilder SubscriptionBuilder() => new(this);
                    public event Action<ReducerEventContext, Exception> OnUnhandledReducerError
                    {
                        add => Reducers.InternalOnUnhandledReducerError += value;
                        remove => Reducers.InternalOnUnhandledReducerError -= value;
                    }
                }

            """;
    }

    // The event contexts and subscription types, unchanged from `spacetime generate`.
    const string ContextAndSubscriptionTypes = """
    public interface IRemoteDbContext : IDbContext<RemoteTables, RemoteReducers, SubscriptionBuilder, RemoteProcedures> {
        public event Action<ReducerEventContext, Exception>? OnUnhandledReducerError;
    }

    public sealed class EventContext : IEventContext, IRemoteDbContext
    {
        private readonly DbConnection conn;

        /// <summary>
        /// The event that caused this callback to run.
        /// </summary>
        public readonly Event<Reducer> Event;

        /// <summary>
        /// Access to tables in the client cache, which stores a read-only replica of the remote database state.
        ///
        /// The returned <c>DbView</c> will have a method to access each table defined by the module.
        /// </summary>
        public RemoteTables Db => conn.Db;
        /// <summary>
        /// Access to reducers defined by the module.
        ///
        /// The returned <c>RemoteReducers</c> will have a method to invoke each reducer defined by the module,
        /// plus methods for adding and removing callbacks on each of those reducers.
        /// </summary>
        public RemoteReducers Reducers => conn.Reducers;
        /// <summary>
        /// Access to procedures defined by the module.
        ///
        /// The returned <c>RemoteProcedures</c> will have a method to invoke each procedure defined by the module,
        /// with a callback for when the procedure completes and returns a value.
        /// </summary>
        public RemoteProcedures Procedures => conn.Procedures;
        /// <summary>
        /// Returns <c>true</c> if the connection is active, i.e. has not yet disconnected.
        /// </summary>
        public bool IsActive => conn.IsActive;
        /// <summary>
        /// Close the connection.
        ///
        /// Throws an error if the connection is already closed.
        /// </summary>
        public void Disconnect() {
            conn.Disconnect();
        }
        /// <summary>
        /// Start building a subscription.
        /// </summary>
        /// <returns>A builder-pattern constructor for subscribing to queries,
        /// causing matching rows to be replicated into the client cache.</returns>
        public SubscriptionBuilder SubscriptionBuilder() => conn.SubscriptionBuilder();
        /// <summary>
        /// Get the <c>Identity</c> of this connection.
        ///
        /// This method returns null if the connection was constructed anonymously
        /// and we have not yet received our newly-generated <c>Identity</c> from the host.
        /// </summary>
        public Identity? Identity => conn.Identity;
        /// <summary>
        /// Get this connection's <c>ConnectionId</c>.
        /// </summary>
        public ConnectionId ConnectionId => conn.ConnectionId;
        /// <summary>
        /// Register a callback to be called when a reducer with no handler returns an error.
        /// </summary>
        public event Action<ReducerEventContext, Exception>? OnUnhandledReducerError {
            add => Reducers.InternalOnUnhandledReducerError += value;
            remove => Reducers.InternalOnUnhandledReducerError -= value;
        }

        internal EventContext(DbConnection conn, Event<Reducer> Event)
        {
            this.conn = conn;
            this.Event = Event;
        }
    }

    public sealed class ReducerEventContext : IReducerEventContext, IRemoteDbContext
    {
        private readonly DbConnection conn;
        /// <summary>
        /// The reducer event that caused this callback to run.
        /// </summary>
        public readonly ReducerEvent<Reducer> Event;

        /// <summary>
        /// Access to tables in the client cache, which stores a read-only replica of the remote database state.
        ///
        /// The returned <c>DbView</c> will have a method to access each table defined by the module.
        /// </summary>
        public RemoteTables Db => conn.Db;
        /// <summary>
        /// Access to reducers defined by the module.
        ///
        /// The returned <c>RemoteReducers</c> will have a method to invoke each reducer defined by the module,
        /// plus methods for adding and removing callbacks on each of those reducers.
        /// </summary>
        public RemoteReducers Reducers => conn.Reducers;
        /// <summary>
        /// Access to procedures defined by the module.
        ///
        /// The returned <c>RemoteProcedures</c> will have a method to invoke each procedure defined by the module,
        /// with a callback for when the procedure completes and returns a value.
        /// </summary>
        public RemoteProcedures Procedures => conn.Procedures;
        /// <summary>
        /// Returns <c>true</c> if the connection is active, i.e. has not yet disconnected.
        /// </summary>
        public bool IsActive => conn.IsActive;
        /// <summary>
        /// Close the connection.
        ///
        /// Throws an error if the connection is already closed.
        /// </summary>
        public void Disconnect() {
            conn.Disconnect();
        }
        /// <summary>
        /// Start building a subscription.
        /// </summary>
        /// <returns>A builder-pattern constructor for subscribing to queries,
        /// causing matching rows to be replicated into the client cache.</returns>
        public SubscriptionBuilder SubscriptionBuilder() => conn.SubscriptionBuilder();
        /// <summary>
        /// Get the <c>Identity</c> of this connection.
        ///
        /// This method returns null if the connection was constructed anonymously
        /// and we have not yet received our newly-generated <c>Identity</c> from the host.
        /// </summary>
        public Identity? Identity => conn.Identity;
        /// <summary>
        /// Get this connection's <c>ConnectionId</c>.
        /// </summary>
        public ConnectionId ConnectionId => conn.ConnectionId;
        /// <summary>
        /// Register a callback to be called when a reducer with no handler returns an error.
        /// </summary>
        public event Action<ReducerEventContext, Exception>? OnUnhandledReducerError {
            add => Reducers.InternalOnUnhandledReducerError += value;
            remove => Reducers.InternalOnUnhandledReducerError -= value;
        }

        internal ReducerEventContext(DbConnection conn, ReducerEvent<Reducer> reducerEvent)
        {
            this.conn = conn;
            Event = reducerEvent;
        }
    }

    public sealed class ErrorContext : IErrorContext, IRemoteDbContext
    {
        private readonly DbConnection conn;
        /// <summary>
        /// The <c>Exception</c> that caused this error callback to be run.
        /// </summary>
        public readonly Exception Event;
        Exception IErrorContext.Event {
            get {
                return Event;
            }
        }

        /// <summary>
        /// Access to tables in the client cache, which stores a read-only replica of the remote database state.
        ///
        /// The returned <c>DbView</c> will have a method to access each table defined by the module.
        /// </summary>
        public RemoteTables Db => conn.Db;
        /// <summary>
        /// Access to reducers defined by the module.
        ///
        /// The returned <c>RemoteReducers</c> will have a method to invoke each reducer defined by the module,
        /// plus methods for adding and removing callbacks on each of those reducers.
        /// </summary>
        public RemoteReducers Reducers => conn.Reducers;
        /// <summary>
        /// Access to procedures defined by the module.
        ///
        /// The returned <c>RemoteProcedures</c> will have a method to invoke each procedure defined by the module,
        /// with a callback for when the procedure completes and returns a value.
        /// </summary>
        public RemoteProcedures Procedures => conn.Procedures;
        /// <summary>
        /// Returns <c>true</c> if the connection is active, i.e. has not yet disconnected.
        /// </summary>
        public bool IsActive => conn.IsActive;
        /// <summary>
        /// Close the connection.
        ///
        /// Throws an error if the connection is already closed.
        /// </summary>
        public void Disconnect() {
            conn.Disconnect();
        }
        /// <summary>
        /// Start building a subscription.
        /// </summary>
        /// <returns>A builder-pattern constructor for subscribing to queries,
        /// causing matching rows to be replicated into the client cache.</returns>
        public SubscriptionBuilder SubscriptionBuilder() => conn.SubscriptionBuilder();
        /// <summary>
        /// Get the <c>Identity</c> of this connection.
        ///
        /// This method returns null if the connection was constructed anonymously
        /// and we have not yet received our newly-generated <c>Identity</c> from the host.
        /// </summary>
        public Identity? Identity => conn.Identity;
        /// <summary>
        /// Get this connection's <c>ConnectionId</c>.
        /// </summary>
        public ConnectionId ConnectionId => conn.ConnectionId;
        /// <summary>
        /// Register a callback to be called when a reducer with no handler returns an error.
        /// </summary>
        public event Action<ReducerEventContext, Exception>? OnUnhandledReducerError {
            add => Reducers.InternalOnUnhandledReducerError += value;
            remove => Reducers.InternalOnUnhandledReducerError -= value;
        }

        internal ErrorContext(DbConnection conn, Exception error)
        {
            this.conn = conn;
            Event = error;
        }
    }

    public sealed class SubscriptionEventContext : ISubscriptionEventContext, IRemoteDbContext
    {
        private readonly DbConnection conn;

        /// <summary>
        /// Access to tables in the client cache, which stores a read-only replica of the remote database state.
        ///
        /// The returned <c>DbView</c> will have a method to access each table defined by the module.
        /// </summary>
        public RemoteTables Db => conn.Db;
        /// <summary>
        /// Access to reducers defined by the module.
        ///
        /// The returned <c>RemoteReducers</c> will have a method to invoke each reducer defined by the module,
        /// plus methods for adding and removing callbacks on each of those reducers.
        /// </summary>
        public RemoteReducers Reducers => conn.Reducers;
        /// <summary>
        /// Access to procedures defined by the module.
        ///
        /// The returned <c>RemoteProcedures</c> will have a method to invoke each procedure defined by the module,
        /// with a callback for when the procedure completes and returns a value.
        /// </summary>
        public RemoteProcedures Procedures => conn.Procedures;
        /// <summary>
        /// Returns <c>true</c> if the connection is active, i.e. has not yet disconnected.
        /// </summary>
        public bool IsActive => conn.IsActive;
        /// <summary>
        /// Close the connection.
        ///
        /// Throws an error if the connection is already closed.
        /// </summary>
        public void Disconnect() {
            conn.Disconnect();
        }
        /// <summary>
        /// Start building a subscription.
        /// </summary>
        /// <returns>A builder-pattern constructor for subscribing to queries,
        /// causing matching rows to be replicated into the client cache.</returns>
        public SubscriptionBuilder SubscriptionBuilder() => conn.SubscriptionBuilder();
        /// <summary>
        /// Get the <c>Identity</c> of this connection.
        ///
        /// This method returns null if the connection was constructed anonymously
        /// and we have not yet received our newly-generated <c>Identity</c> from the host.
        /// </summary>
        public Identity? Identity => conn.Identity;
        /// <summary>
        /// Get this connection's <c>ConnectionId</c>.
        /// </summary>
        public ConnectionId ConnectionId => conn.ConnectionId;
        /// <summary>
        /// Register a callback to be called when a reducer with no handler returns an error.
        /// </summary>
        public event Action<ReducerEventContext, Exception>? OnUnhandledReducerError {
            add => Reducers.InternalOnUnhandledReducerError += value;
            remove => Reducers.InternalOnUnhandledReducerError -= value;
        }

        internal SubscriptionEventContext(DbConnection conn)
        {
            this.conn = conn;
        }
    }

    public sealed class ProcedureEventContext : IProcedureEventContext, IRemoteDbContext
    {
        private readonly DbConnection conn;
        /// <summary>
        /// The procedure event that caused this callback to run.
        /// </summary>
        public readonly ProcedureEvent Event;

        /// <summary>
        /// Access to tables in the client cache, which stores a read-only replica of the remote database state.
        ///
        /// The returned <c>DbView</c> will have a method to access each table defined by the module.
        /// </summary>
        public RemoteTables Db => conn.Db;
        /// <summary>
        /// Access to reducers defined by the module.
        ///
        /// The returned <c>RemoteReducers</c> will have a method to invoke each reducer defined by the module,
        /// plus methods for adding and removing callbacks on each of those reducers.
        /// </summary>
        public RemoteReducers Reducers => conn.Reducers;
        /// <summary>
        /// Access to procedures defined by the module.
        ///
        /// The returned <c>RemoteProcedures</c> will have a method to invoke each procedure defined by the module,
        /// with a callback for when the procedure completes and returns a value.
        /// </summary>
        public RemoteProcedures Procedures => conn.Procedures;
        /// <summary>
        /// Returns <c>true</c> if the connection is active, i.e. has not yet disconnected.
        /// </summary>
        public bool IsActive => conn.IsActive;
        /// <summary>
        /// Close the connection.
        ///
        /// Throws an error if the connection is already closed.
        /// </summary>
        public void Disconnect() {
            conn.Disconnect();
        }
        /// <summary>
        /// Start building a subscription.
        /// </summary>
        /// <returns>A builder-pattern constructor for subscribing to queries,
        /// causing matching rows to be replicated into the client cache.</returns>
        public SubscriptionBuilder SubscriptionBuilder() => conn.SubscriptionBuilder();
        /// <summary>
        /// Get the <c>Identity</c> of this connection.
        ///
        /// This method returns null if the connection was constructed anonymously
        /// and we have not yet received our newly-generated <c>Identity</c> from the host.
        /// </summary>
        public Identity? Identity => conn.Identity;
        /// <summary>
        /// Get this connection's <c>ConnectionId</c>.
        /// </summary>
        public ConnectionId ConnectionId => conn.ConnectionId;
        /// <summary>
        /// Register a callback to be called when a reducer with no handler returns an error.
        /// </summary>
        public event Action<ReducerEventContext, Exception>? OnUnhandledReducerError {
            add => Reducers.InternalOnUnhandledReducerError += value;
            remove => Reducers.InternalOnUnhandledReducerError -= value;
        }

        internal ProcedureEventContext(DbConnection conn, ProcedureEvent Event)
        {
            this.conn = conn;
            this.Event = Event;
        }
    }

    /// <summary>
    /// Builder-pattern constructor for subscription queries.
    /// </summary>
    public sealed class SubscriptionBuilder
    {
        private readonly IDbConnection conn;

        private event Action<SubscriptionEventContext>? Applied;
        private event Action<ErrorContext, Exception>? Error;

        /// <summary>
        /// Private API, use <c>conn.SubscriptionBuilder()</c> instead.
        /// </summary>
        public SubscriptionBuilder(IDbConnection conn)
        {
            this.conn = conn;
        }

        /// <summary>
        /// Register a callback to run when the subscription is applied.
        /// </summary>
        public SubscriptionBuilder OnApplied(
            Action<SubscriptionEventContext> callback
        )
        {
            Applied += callback;
            return this;
        }

        /// <summary>
        /// Register a callback to run when the subscription fails.
        ///
        /// Note that this callback may run either when attempting to apply the subscription,
        /// in which case <c>Self::on_applied</c> will never run,
        /// or later during the subscription's lifetime if the module's interface changes,
        /// in which case <c>Self::on_applied</c> may have already run.
        /// </summary>
        public SubscriptionBuilder OnError(
            Action<ErrorContext, Exception> callback
        )
        {
            Error += callback;
            return this;
        }
    
        /// <summary>
        /// Add a typed query to this subscription.
        ///
        /// This is the entry point for building subscriptions without writing SQL by hand.
        /// Once a typed query is added, only typed queries may follow (SQL and typed queries cannot be mixed).
        /// </summary>
        public TypedSubscriptionBuilder AddQuery<TRow>(
            Func<QueryBuilder, global::SpacetimeDB.IQuery<TRow>> build
        )
        {
            var typed = new TypedSubscriptionBuilder(conn, Applied, Error);
            return typed.AddQuery(build);
        }

        /// <summary>
        /// Subscribe to the following SQL queries.
        ///
        /// This method returns immediately, with the data not yet added to the DbConnection.
        /// The provided callbacks will be invoked once the data is returned from the remote server.
        /// Data from all the provided queries will be returned at the same time.
        ///
        /// See the SpacetimeDB SQL docs for more information on SQL syntax:
        /// <a href="https://spacetimedb.com/docs/reference/sql">https://spacetimedb.com/docs/reference/sql</a>
        /// </summary>
        public SubscriptionHandle Subscribe(
            string[] querySqls
        ) => new(conn, Applied, Error, querySqls);

        /// <summary>
        /// Subscribe to all rows from all tables.
        ///
        /// This method is intended as a convenience
        /// for applications where client-side memory use and network bandwidth are not concerns.
        /// Applications where these resources are a constraint
        /// should register more precise queries via <c>Self.Subscribe</c>
        /// in order to replicate only the subset of data which the client needs to function.
        ///
        /// This method should not be combined with <c>Self.Subscribe</c> on the same <c>DbConnection</c>.
        /// A connection may either <c>Self.Subscribe</c> to particular queries,
        /// or <c>Self.SubscribeToAllTables</c>, but not both.
        /// Attempting to call <c>Self.Subscribe</c>
        /// on a <c>DbConnection</c> that has previously used <c>Self.SubscribeToAllTables</c>,
        /// or vice versa, may misbehave in any number of ways,
        /// including dropping subscriptions, corrupting the client cache, or panicking.
        /// </summary>
        public SubscriptionHandle SubscribeToAllTables() =>
            new(conn, Applied, Error, QueryBuilder.AllTablesSqlQueries());
    }

    public sealed class SubscriptionHandle : SubscriptionHandleBase<SubscriptionEventContext, ErrorContext> {
        /// <summary>
        /// Internal API. Construct <c>SubscriptionHandle</c>s using <c>conn.SubscriptionBuilder</c>.
        /// </summary>
        public SubscriptionHandle(
            IDbConnection conn,
            Action<SubscriptionEventContext>? onApplied,
            Action<ErrorContext, Exception>? onError,
            string[] querySqls
        ) : base(conn, onApplied, onError, querySqls)
        { }
    }
""";
}

[Generator]
public class Client : IIncrementalGenerator
{
    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        // Module builds expand the declarations with SpacetimeDB.Codegen.Module instead.
        // Bindings generated with `--namespace SpacetimeDB.Internal` declare a class of the same
        // name, so this checks the assembly too.
        var isModule = context
            .CompilationProvider.Select(
                (compilation, ct) =>
                    compilation
                        .GetTypeByMetadataName("SpacetimeDB.Internal.Module")
                        ?.ContainingAssembly.Name == "SpacetimeDB.Runtime"
            )
            .WithTrackingName("SpacetimeDB.Client.IsModule");
        // Only client builds, which reference the client SDK, get the client API.
        var isClient = context
            .CompilationProvider.Select(
                (compilation, ct) =>
                    compilation.GetTypeByMetadataName("SpacetimeDB.DbConnectionBase`3") is not null
            )
            .WithTrackingName("SpacetimeDB.Client.IsClient");

        // Row types are BSATN types in any build but a module's, so they are parsed without the
        // client SDK too.
        var rows = Parse(
            context,
            isModule.Select((module, ct) => !module),
            typeof(TableAttribute).FullName,
            (node, ct) => node is TypeDeclarationSyntax,
            (ctx, diag) => new ClientTableDeclaration(ctx, diag),
            "Table"
        );
        var reducers = Parse(
            context,
            isClient,
            typeof(ReducerAttribute).FullName,
            (node, ct) => node is MethodDeclarationSyntax,
            (ctx, diag) => new ClientReducerDeclaration(ctx, diag),
            "Reducer"
        );
        var procedures = Parse(
            context,
            isClient,
            typeof(ProcedureAttribute).FullName,
            (node, ct) => node is MethodDeclarationSyntax,
            (ctx, diag) => new ClientProcedureDeclaration(ctx, diag),
            "Procedure"
        );
        var views = Parse(
            context,
            isClient,
            typeof(ViewAttribute).FullName,
            (node, ct) => node is MethodDeclarationSyntax,
            (ctx, diag) => new ClientViewDeclaration(ctx, diag),
            "View"
        );

        // The client cache only stores classes, so struct rows have no client API.
        context.RegisterSourceOutput(
            context
                .SyntaxProvider.ForAttributeWithMetadataName(
                    typeof(TableAttribute).FullName,
                    (node, ct) => node is TypeDeclarationSyntax,
                    (ctx, ct) =>
                        ctx.TargetSymbol is ITypeSymbol { IsValueType: true }
                            ? ErrorDescriptor.ClientStructRow.ToDiag(
                                (TypeDeclarationSyntax)ctx.TargetNode
                            )
                            : null
                )
                .Combine(isClient)
                .Where(pair => pair.Left is not null && pair.Right),
            (ctx, pair) => ctx.ReportDiagnostic(pair.Left!)
        );

        context.RegisterSourceOutput(
            rows.Combine(isClient),
            (ctx, pair) =>
            {
                var (row, client) = pair;
                // A [Table] row type is a BSATN type, like a [Type] one.
                var bsatn = row.HasTypeAttribute ? "" : row.ToExtensions().ToString();
                if (client && row.Tables.Length > 0)
                {
                    AddSource(
                        ctx,
                        $"{row.FullName}.Table",
                        ClientCode.File(
                            row.Tables[0].Namespace,
                            string.Concat(row.Tables.Select(ClientCode.Table))
                        ) + bsatn
                    );
                }
                else if (bsatn != "")
                {
                    // Without the client API, the source looks like the Type generator's.
                    AddSource(
                        ctx,
                        $"{row.FullName}.Table",
                        $"// <auto-generated />\n#nullable enable\n\n{bsatn}"
                    );
                }
            }
        );

        context.RegisterSourceOutput(
            reducers,
            (ctx, reducer) =>
                AddSource(
                    ctx,
                    $"{reducer.Function.HintName}.Reducer",
                    (reducer.HasClientApi ? ClientCode.ReducerFile(reducer) : "")
                        + reducer.Function.ImplementationExtensions()
                )
        );

        context.RegisterSourceOutput(
            procedures,
            (ctx, procedure) =>
                AddSource(
                    ctx,
                    $"{procedure.Function.HintName}.Procedure",
                    (procedure.Function.IsValid ? ClientCode.ProcedureFile(procedure) : "")
                        + procedure.Function.ImplementationExtensions()
                )
        );

        context.RegisterSourceOutput(
            views,
            (ctx, view) =>
                AddSource(
                    ctx,
                    $"{view.Function.HintName}.View",
                    (
                        view.Function.IsValid && view.Table is { } t
                            ? ClientCode.File(t.Namespace, ClientCode.Table(t))
                            : ""
                    ) + view.Function.ImplementationExtensions()
                )
        );

        // The module-wide items, once per namespace.
        var allTables = rows.Combine(isClient)
            .Where(pair => pair.Right)
            .SelectMany((pair, ct) => pair.Left.Tables)
            .Collect()
            .Combine(
                views
                    .Where(view => view.Function.IsValid && view.Table is not null)
                    .Select((view, ct) => view.Table!)
                    .Collect()
            )
            .Select((pair, ct) => new EquatableArray<ClientTable>(pair.Left.AddRange(pair.Right)));
        var moduleInputs = allTables
            .Combine(
                reducers
                    .Where(r => r.HasClientApi)
                    .Collect()
                    .Select((rs, ct) => new EquatableArray<ClientReducerDeclaration>(rs))
            )
            .Combine(
                procedures
                    .Where(p => p.Function.IsValid)
                    .Select((p, ct) => p.Function.Namespace)
                    .Collect()
                    .Select((ns, ct) => new EquatableArray<string>(ns))
            )
            .Combine(
                views
                    .Where(v => v.Function.IsValid)
                    .Select((v, ct) => v.Function.Namespace)
                    .Collect()
                    .Select((ns, ct) => new EquatableArray<string>(ns))
            )
            .WithTrackingName("SpacetimeDB.Client.Module");

        context.RegisterSourceOutput(
            moduleInputs,
            (ctx, input) =>
            {
                var (((tables, reducers), procedureNamespaces), viewNamespaces) = input;
                var all = tables
                    .Select(t => t.Namespace)
                    .Concat(reducers.Select(r => r.Function.Namespace))
                    .Concat(procedureNamespaces)
                    .Concat(viewNamespaces)
                    .Distinct();
                foreach (var ns in all)
                {
                    AddSource(
                        ctx,
                        $"{ns}.SpacetimeDBClient",
                        ClientCode.File(
                            ns,
                            ClientCode.Module(
                                tables.Where(t => t.Namespace == ns),
                                reducers.Where(r => r.Function.Namespace == ns)
                            )
                        )
                    );
                }
            }
        );
    }

    private static void AddSource(SourceProductionContext ctx, string name, string source)
    {
        if (source != "")
        {
            ctx.AddSource($"{ClientCode.HintName(name)}.g.cs", source);
        }
    }

    private static IncrementalValuesProvider<T> Parse<T>(
        IncrementalGeneratorInitializationContext context,
        IncrementalValueProvider<bool> enabled,
        string attributeName,
        Func<SyntaxNode, CancellationToken, bool> predicate,
        Func<GeneratorAttributeSyntaxContext, DiagReporter, T> parse,
        string kind
    )
        where T : IEquatable<T>
    {
        var parsed = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                attributeName,
                predicate,
                (ctx, ct) => ctx.ParseWithDiags(diag => parse(ctx, diag))
            )
            .Combine(enabled)
            .Where(pair => pair.Right)
            .Select((pair, ct) => pair.Left)
            .WithTrackingName($"SpacetimeDB.Client.{kind}.Parse");

        context.RegisterSourceOutput(
            parsed.SelectMany((result, ct) => result.Diag),
            (ctx, diag) => ctx.ReportDiagnostic(diag)
        );

        return parsed.Select((result, ct) => result.Parsed!).Where(parsed => parsed is not null);
    }
}
