namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using System.Linq;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using SpacetimeDB.Internal;
using static Utils;

/// <summary>
/// Represents column attributes parsed from field attributes in table classes.
/// Used to track metadata like primary keys, unique constraints, and default values.
/// </summary>
/// <param name="Mask">Bitmask representing the column attributes (PrimaryKey, Unique, etc.)</param>
/// <param name="Table">Optional table name if the attribute is table-specific</param>
/// <param name="Value">Optional value for attributes like Default that carry additional data</param>
readonly record struct ColumnAttr(ColumnAttrs Mask, string? Table = null, string? Value = null)
{
    // Maps attribute type names to their corresponding attribute types
    private static readonly ImmutableDictionary<string, System.Type> AttrTypes = ImmutableArray
        .Create(
            typeof(AutoIncAttribute),
            typeof(PrimaryKeyAttribute),
            typeof(UniqueAttribute),
            typeof(DefaultAttribute)
        )
        .ToImmutableDictionary(t => t.FullName!);

    /// <summary>
    /// Parses a Roslyn AttributeData into a ColumnAttr instance.
    /// </summary>
    /// <param name="attrData">The attribute data to parse</param>
    /// <returns>A ColumnAttr instance representing the parsed attribute, or default if the attribute type is not recognized</returns>
    public static ColumnAttr Parse(AttributeData attrData)
    {
        if (
            attrData.AttributeClass is not { } attrClass
            || !AttrTypes.TryGetValue(attrClass.ToString(), out var attrType)
        )
        {
            return default;
        }

        // Special handling for DefaultAttribute as it contains an additional value
        if (attrClass.ToString() == typeof(DefaultAttribute).FullName)
        {
            var defaultAttr = attrData.ParseAs<DefaultAttribute>(attrType);
            return new(defaultAttr.Mask, defaultAttr.Table, defaultAttr.Value);
        }

        // Handle standard column attributes (PrimaryKey, Unique, AutoInc)
        var attr = attrData.ParseAs<ColumnAttribute>(attrType);
        return new(attr.Mask, attr.Table);
    }
}

/// <summary>
/// Represents a reference to a column in a table, combining its index and name.
/// Used to maintain references to columns for indexing and querying purposes.
/// </summary>
/// <param name="Index">The zero-based index of the column in the table</param>
/// <param name="Name">The name of the column as defined in the source code</param>
record ColumnRef(int Index, string Name);

/// <summary>
/// Represents the declaration of a column in a table.
/// Contains metadata and attributes for the column, including its type, constraints, and indexes.
/// </summary>
record ColumnDeclaration : MemberDeclaration
{
    public readonly EquatableArray<ColumnAttr> Attrs;
    public readonly EquatableArray<TableIndex> Indexes;
    public readonly bool IsEquatable;
    public readonly string FullTableName;
    public readonly int ColumnIndex;
    public readonly string? ColumnDefaultValue;

    // A helper to combine multiple column attributes into a single mask.
    // Note: it doesn't check the table names, this is left up to the caller.
    private static ColumnAttrs CombineColumnAttrs(IEnumerable<ColumnAttr> attrs) =>
        attrs.Aggregate(ColumnAttrs.UnSet, (mask, attr) => mask | attr.Mask);

    public ColumnDeclaration(string tableName, int index, IFieldSymbol field, DiagReporter diag)
        : base(field, diag)
    {
        FullTableName = tableName;
        ColumnIndex = index;

        Attrs = new(
            field
                .GetAttributes()
                .Select(ColumnAttr.Parse)
                .Where(a => a.Mask != ColumnAttrs.UnSet)
                .GroupBy(
                    a => a.Table,
                    (key, group) => new ColumnAttr(CombineColumnAttrs(group), key)
                )
                .ToImmutableArray()
        );

        Indexes = new(
            field
                .GetAttributes()
                .Where(TableIndex.CanParse)
                .Select(a => new TableIndex(new ColumnRef(index, field.Name), a, diag))
                .ToImmutableArray()
        );

        ColumnDefaultValue = field
            .GetAttributes()
            .Select(ColumnAttr.Parse)
            .Where(a => a.Mask == ColumnAttrs.Default)
            .Select(a => a.Value)
            .ToList()
            .FirstOrDefault();

        var type = field.Type;

        var isInteger = type.SpecialType switch
        {
            SpecialType.System_Byte
            or SpecialType.System_SByte
            or SpecialType.System_Int16
            or SpecialType.System_UInt16
            or SpecialType.System_Int32
            or SpecialType.System_UInt32
            or SpecialType.System_Int64
            or SpecialType.System_UInt64 => true,
            SpecialType.None => type.ToString()
                is "System.Int128"
                    or "System.UInt128"
                    or "SpacetimeDB.I128"
                    or "SpacetimeDB.U128"
                    or "SpacetimeDB.I256"
                    or "SpacetimeDB.U256",
            _ => false,
        };

        var attrs = CombineColumnAttrs(Attrs);

        if (attrs.HasFlag(ColumnAttrs.AutoInc) && !isInteger)
        {
            diag.Report(ErrorDescriptor.AutoIncNotInteger, field);
        }

        // Check whether this is a sum type without a payload.
        var isAllUnitEnum = false;
        if (type.TypeKind == Microsoft.CodeAnalysis.TypeKind.Enum)
        {
            isAllUnitEnum = true;
        }
        else if (type.BaseType?.OriginalDefinition.ToString() == "SpacetimeDB.TaggedEnum<Variants>")
        {
            if (
                type.BaseType.TypeArguments.FirstOrDefault() is INamedTypeSymbol
                {
                    IsTupleType: true,
                    TupleElements: var taggedEnumVariants
                }
            )
            {
                isAllUnitEnum = taggedEnumVariants.All(
                    (field) => field.Type.ToString() == "SpacetimeDB.Unit"
                );
            }
        }

        IsEquatable =
            (
                isInteger
                || isAllUnitEnum
                || type.SpecialType switch
                {
                    SpecialType.System_String or SpecialType.System_Boolean => true,
                    SpecialType.None => type.ToString()
                        is "SpacetimeDB.ConnectionId"
                            or "SpacetimeDB.Identity"
                            or "SpacetimeDB.Uuid",
                    _ => false,
                }
            )
            && type.NullableAnnotation != NullableAnnotation.Annotated;

        if (attrs.HasFlag(ColumnAttrs.Unique) && !IsEquatable)
        {
            diag.Report(ErrorDescriptor.UniqueNotEquatable, field);
        }

        if (
            attrs.HasFlag(ColumnAttrs.Default)
            && (
                attrs.HasFlag(ColumnAttrs.AutoInc)
                || attrs.HasFlag(ColumnAttrs.PrimaryKey)
                || attrs.HasFlag(ColumnAttrs.Unique)
            )
        )
        {
            diag.Report(ErrorDescriptor.IncompatibleDefaultAttributesCombination, field);
        }
    }

    public ColumnAttrs GetAttrs(TableAccessor tableAccessor) =>
        CombineColumnAttrs(Attrs.Where(x => x.Table == null || x.Table == tableAccessor.Name));

    // For the `TableDesc` constructor.
    public string GenerateColumnDef() =>
        $"new (nameof({Name}), BSATN.{Name}{TypeUse.BsatnFieldSuffix}.GetAlgebraicType(registrar))";
}

record Scheduled(string ReducerName, int ScheduledAtColumn);

record TableAccessor
{
    public readonly string Name;
    public readonly bool IsPublic;
    public readonly Scheduled? Scheduled;

    public TableAccessor(TableDeclaration table, AttributeData data, DiagReporter diag)
    {
        var attr = data.ParseAs<TableAttribute>();

        Name = attr.Name ?? table.ShortName;
        IsPublic = attr.Public;
        if (
            attr.Scheduled is { } reducer
            && table.GetColumnIndex(data, attr.ScheduledAt, diag) is { } scheduledAtIndex
        )
        {
            try
            {
                Scheduled = new(reducer, scheduledAtIndex);
                if (
                    table.GetPrimaryKey(this) is not { } pk
                    || table.Members[pk].Type.Name != "ulong"
                )
                {
                    throw new InvalidOperationException(
                        $"{Name} is a scheduled table but doesn't have a primary key of type `ulong`."
                    );
                }
                if (
                    table.Members[Scheduled.ScheduledAtColumn].Type.Name != "SpacetimeDB.ScheduleAt"
                )
                {
                    throw new InvalidOperationException(
                        $"{Name}.{attr.ScheduledAt} is marked with `ScheduledAt`, but doesn't have the expected type `SpacetimeDB.ScheduleAt`."
                    );
                }
            }
            catch (Exception e)
            {
                diag.Report(ErrorDescriptor.InvalidScheduledDeclaration, (data, e.Message));
            }
        }
    }
}

enum TableIndexType
{
    BTree,
}

/// <summary>
/// Represents an index on a database table accessor, used to optimize queries.
/// Supports B-tree indexing (and potentially other types in the future).
/// </summary>
record TableIndex
{
    public readonly EquatableArray<ColumnRef> Columns;
    public readonly string? Table;
    public readonly string AccessorName;
    public readonly TableIndexType Type;

    // See: bindings_sys::index_id_from_name for documentation of this format.
    // Guaranteed not to contain quotes, so does not need to be escaped when embedded in a string.
    private readonly string StandardNameSuffix;

    /// <summary>
    /// Primary constructor that initializes all fields.
    /// Other constructors delegate to this one to avoid code duplication.
    /// </summary>
    /// <param name="accessorName">Name to use when accessing this index. If null, will be generated from column names.</param>
    /// <param name="columns">The columns that make up this index.</param>
    /// <param name="tableName">The name of the table this index belongs to, if any.</param>
    /// <param name="type">The type of index (currently only B-tree is supported).</param>
    private TableIndex(
        string? accessorName,
        ImmutableArray<ColumnRef> columns,
        string? tableName,
        TableIndexType type
    )
    {
        Columns = new(columns);
        Table = tableName;
        var columnNames = string.Join("_", columns.Select(c => c.Name));
        AccessorName = accessorName ?? columnNames;
        Type = type;
        StandardNameSuffix = $"_{columnNames}_idx_{Type.ToString().ToLower()}";
    }

    /// <summary>
    /// Creates a B-tree index on a single column with auto-generated name.
    /// </summary>
    /// <param name="col">The column to index.</param>
    public TableIndex(ColumnRef col)
        : this(
            null,
            ImmutableArray.Create(col),
            null,
            TableIndexType.BTree // this might become hash in the future
        ) { }

    /// <summary>
    /// Creates an index with the given attribute and columns.
    /// Used internally by other constructors that parse attributes.
    /// </summary>
    private TableIndex(Index.BTreeAttribute attr, ImmutableArray<ColumnRef> columns)
        : this(attr.Name, columns, attr.Table, TableIndexType.BTree) { }

    /// <summary>
    /// Creates an index from a table declaration and attribute data.
    /// Validates the index configuration and reports any errors through the diag reporter.
    /// </summary>
    private TableIndex(
        TableDeclaration table,
        Index.BTreeAttribute attr,
        AttributeData data,
        DiagReporter diag
    )
        : this(
            attr,
            attr.Columns.Select(name => new ColumnRef(
                    table.GetColumnIndex(data, name, diag) ?? -1,
                    name
                ))
                .Where(c => c.Index != -1)
                .ToImmutableArray()
        )
    {
        if (attr.Columns.Length == 0)
        {
            diag.Report(ErrorDescriptor.EmptyIndexColumns, data);
        }
    }

    /// <summary>
    /// Creates an index by parsing attribute data from a table declaration.
    /// </summary>
    public TableIndex(TableDeclaration table, AttributeData data, DiagReporter diag)
        : this(table, data.ParseAs<Index.BTreeAttribute>(), data, diag) { }

    /// <summary>
    /// Creates an index for a single column with attribute data.
    /// Validates that no additional columns were specified in the attribute.
    /// </summary>
    private TableIndex(
        ColumnRef column,
        Index.BTreeAttribute attr,
        AttributeData data,
        DiagReporter diag
    )
        : this(attr, ImmutableArray.Create(column))
    {
        if (attr.Columns.Length != 0)
        {
            diag.Report(ErrorDescriptor.UnexpectedIndexColumns, data);
        }
    }

    /// <summary>
    /// Creates an index for a single column by parsing attribute data.
    /// </summary>
    public TableIndex(ColumnRef col, AttributeData data, DiagReporter diag)
        : this(col, data.ParseAs<Index.BTreeAttribute>(), data, diag) { }

    // `FullName` and Roslyn have different ways of representing nested types in full names -
    // one uses a `Parent+Child` syntax, the other uses `Parent.Child`.
    // Manually fixup one to the other.
    private static readonly string BTreeAttrName = typeof(Index.BTreeAttribute).FullName.Replace(
        '+',
        '.'
    );

    public static bool CanParse(AttributeData data) =>
        data.AttributeClass?.ToString() == BTreeAttrName;

    public string GenerateIndexDef() =>
        $$"""
            new(
                Name: null,
                AccessorName: "{{AccessorName}}",
                Algorithm: new SpacetimeDB.Internal.RawIndexAlgorithm.{{Type}}([{{string.Join(
                    ", ",
                    Columns.Select(c => c.Index)
                )}}])
            )
            """;

    public string StandardIndexName(TableAccessor tableAccessor) =>
        tableAccessor.Name + StandardNameSuffix;
}

/// <summary>
/// Represents a table declaration in a module.
/// Handles table metadata, accessors, indexes, and column declarations for code generation.
/// </summary>
record TableDeclaration : BaseTypeDeclaration<ColumnDeclaration>
{
    public readonly Accessibility Visibility;
    public readonly EquatableArray<TableAccessor> TableAccessors;
    public readonly EquatableArray<TableIndex> Indexes;

    private readonly bool isRowStruct;

    public int? GetColumnIndex(AttributeData attrContext, string name, DiagReporter diag)
    {
        var index = Members
            .Select((col, i) => (col, i))
            .FirstOrDefault(pair => pair.col.Name == name);
        if (index.col is null)
        {
            diag.Report(ErrorDescriptor.UnknownColumn, (attrContext, name, ShortName));
            return null;
        }
        return index.i;
    }

    public TableDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
        : base(context, diag)
    {
        var typeSyntax = (TypeDeclarationSyntax)context.TargetNode;

        isRowStruct = ((INamedTypeSymbol)context.TargetSymbol).IsValueType;

        if (Kind is TypeKind.Sum)
        {
            diag.Report(ErrorDescriptor.TableTaggedEnum, typeSyntax);
        }

        var container = context.TargetSymbol;
        Visibility = container.DeclaredAccessibility;
        while (container != null)
        {
            switch (container.DeclaredAccessibility)
            {
                case Accessibility.ProtectedAndInternal:
                case Accessibility.NotApplicable:
                case Accessibility.Internal:
                case Accessibility.Public:
                    if (Visibility < container.DeclaredAccessibility)
                    {
                        Visibility = container.DeclaredAccessibility;
                    }
                    break;
                default:
                    diag.Report(ErrorDescriptor.InvalidTableVisibility, typeSyntax);
                    throw new Exception(
                        "Table row type visibility must be public or internal, including containing types."
                    );
            }

            container = container.ContainingType;
        }

        TableAccessors = new(
            context.Attributes.Select(a => new TableAccessor(this, a, diag)).ToImmutableArray()
        );
        Indexes = new(
            context
                .TargetSymbol.GetAttributes()
                .Where(TableIndex.CanParse)
                .Select(a => new TableIndex(this, a, diag))
                .ToImmutableArray()
        );
    }

    protected override ColumnDeclaration ConvertMember(
        int index,
        IFieldSymbol field,
        DiagReporter diag
    ) => new(FullName, index, field, diag);

    public IEnumerable<string> GenerateTableAccessorFilters(TableAccessor tableAccessor)
    {
        var vis = SyntaxFacts.GetText(Visibility);
        var globalName = $"global::{FullName}";

        var uniqueIndexBase = isRowStruct ? "UniqueIndex" : "RefUniqueIndex";

        foreach (var ct in GetConstraints(tableAccessor, ColumnAttrs.Unique))
        {
            var f = ct.Col;
            if (!f.IsEquatable)
            {
                // Skip - we already emitted diagnostic for this during parsing, and generated code would
                // only produce a lot of noisy typechecking errors.
                continue;
            }
            var standardIndexName = ct.ToIndex().StandardIndexName(tableAccessor);
            yield return $$"""
                {{vis}} sealed class {{f.Name}}UniqueIndex : {{uniqueIndexBase}}<{{tableAccessor.Name}}, {{globalName}}, {{f.Type.Name}}, {{f.Type.BSATNName}}> {
                    internal {{f.Name}}UniqueIndex() : base("{{standardIndexName}}") {}
                    // Important: don't move this to the base class.
                    // C# generics don't play well with nullable types and can't accept both struct-type-based and class-type-based
                    // `globalName` in one generic definition, leading to buggy `Row?` expansion for either one or another.
                    public {{globalName}}? Find({{f.Type.Name}} key) => FindSingle(key);
                    public {{globalName}} Update({{globalName}} row) => DoUpdate(row);
                }
                {{vis}} {{f.Name}}UniqueIndex {{f.Name}} => new();
                """;
        }

        foreach (var index in GetIndexes(tableAccessor))
        {
            var name = index.AccessorName;

            // Skip bad declarations. Empty name means no columns, which we have already reported with a meaningful error.
            // Emitting this will result in further compilation errors due to missing property name.
            if (name == "")
            {
                continue;
            }

            var members = index.Columns.Select(c => Members[c.Index]).ToArray();
            var standardIndexName = index.StandardIndexName(tableAccessor);

            yield return $$"""
                    {{vis}} sealed class {{name}}Index() : SpacetimeDB.Internal.IndexBase<{{globalName}}>("{{standardIndexName}}") {
                """;

            for (var n = 0; n < members.Length; n++)
            {
                var types = string.Join(
                    ", ",
                    members.Take(n + 1).Select(m => $"{m.Type.Name}, {m.Type.BSATNName}")
                );
                var scalars = members.Take(n).Select(m => $"{m.Type.Name} {m.Name}");
                var lastScalar = $"{members[n].Type.Name} {members[n].Name}";
                var lastBounds =
                    $"global::SpacetimeDB.Bound<{members[n].Type.Name}> {members[n].Name}";
                var argsScalar = string.Join(", ", scalars.Append(lastScalar));
                var argsBounds = string.Join(", ", scalars.Append(lastBounds));
                string argName;
                if (n > 0)
                {
                    argName = "f";
                    argsScalar = $"({argsScalar}) f";
                    argsBounds = $"({argsBounds}) f";
                }
                else
                {
                    argName = members[0].Name;
                }

                yield return $$"""
                        public IEnumerable<{{globalName}}> Filter({{argsScalar}}) =>
                            DoFilter(new SpacetimeDB.Internal.BTreeIndexBounds<{{types}}>({{argName}}));

                        public ulong Delete({{argsScalar}}) =>
                            DoDelete(new SpacetimeDB.Internal.BTreeIndexBounds<{{types}}>({{argName}}));

                        public IEnumerable<{{globalName}}> Filter({{argsBounds}}) =>
                            DoFilter(new SpacetimeDB.Internal.BTreeIndexBounds<{{types}}>({{argName}}));

                        public ulong Delete({{argsBounds}}) =>
                            DoDelete(new SpacetimeDB.Internal.BTreeIndexBounds<{{types}}>({{argName}}));
                    
                    """;
            }

            yield return $"}}\n {vis} {name}Index {name} => new();\n";
        }
    }

    private IEnumerable<string> GenerateReadOnlyAccessorFilters(TableAccessor tableAccessor)
    {
        var vis = SyntaxFacts.GetText(Visibility);
        var globalName = $"global::{FullName}";

        var uniqueIndexBase = isRowStruct
            ? "global::SpacetimeDB.Internal.ReadOnlyUniqueIndex"
            : "global::SpacetimeDB.Internal.ReadOnlyRefUniqueIndex";

        foreach (var ct in GetConstraints(tableAccessor, ColumnAttrs.Unique))
        {
            var f = ct.Col;
            if (!f.IsEquatable)
            {
                continue;
            }

            var standardIndexName = ct.ToIndex().StandardIndexName(tableAccessor);

            yield return $$$"""
                public sealed class {{{f.Name}}}Index
                    : {{{uniqueIndexBase}}}<
                          global::SpacetimeDB.Internal.ViewHandles.{{{tableAccessor.Name}}}ReadOnly,
                          {{{globalName}}},
                          {{{f.Type.Name}}},
                          {{{f.Type.BSATNName}}}>
                {
                    internal {{{f.Name}}}Index() : base("{{{standardIndexName}}}") { }

                    public {{{globalName}}}? Find({{{f.Type.Name}}} key) => FindSingle(key);
                }

                public {{{f.Name}}}Index {{{f.Name}}} => new();
                """;
        }

        foreach (var index in GetIndexes(tableAccessor))
        {
            if (string.IsNullOrEmpty(index.AccessorName))
            {
                continue;
            }

            var members = index.Columns.Select(c => Members[c.Index]).ToArray();
            var standardIndexName = index.StandardIndexName(tableAccessor);
            var name = index.AccessorName;

            var blocks = new List<string>
            {
                $$$"""
                    public sealed class {{{name}}}Index
                    : global::SpacetimeDB.Internal.ReadOnlyIndexBase<{{{globalName}}}>
                    {
                    internal {{{name}}}Index() : base("{{{standardIndexName}}}") {}
                    """,
            };

            for (var n = 0; n < members.Length; n++)
            {
                var declaringMembers = members.Take(n + 1).ToArray();
                var types = string.Join(
                    ", ",
                    declaringMembers.Select(m => $"{m.Type.Name}, {m.Type.BSATNName}")
                );
                var scalarArgs = string.Join(
                    ", ",
                    declaringMembers.Select(m => $"{m.Type.Name} {m.Name}")
                );
                var boundsArgs = string.Join(
                    ", ",
                    declaringMembers
                        .Take(n)
                        .Select(m => $"{m.Type.Name} {m.Name}")
                        .Append(
                            $"global::SpacetimeDB.Bound<{declaringMembers[^1].Type.Name}> {declaringMembers[^1].Name}"
                        )
                );

                var ctorArg = n == 0 ? declaringMembers[0].Name : "f";

                if (n > 0)
                {
                    scalarArgs = $"({scalarArgs}) f";
                    boundsArgs = $"({boundsArgs}) f";
                }

                blocks.Add(
                    $$$"""
                    public IEnumerable<{{{globalName}}}> Filter({{{scalarArgs}}}) =>
                        DoFilter(new global::SpacetimeDB.Internal.BTreeIndexBounds<{{{types}}}>({{{ctorArg}}}));

                    public IEnumerable<{{{globalName}}}> Filter({{{boundsArgs}}}) =>
                        DoFilter(new global::SpacetimeDB.Internal.BTreeIndexBounds<{{{types}}}>({{{ctorArg}}}));
                    """
                );
            }

            blocks.Add($"}}\n{vis} {name}Index {name} => new();");
            yield return string.Join("\n", blocks);
        }
    }

    /// <summary>
    /// Represents a generated accessor for a table, providing different access patterns
    /// and visibility levels for the underlying table data.
    /// </summary>
    /// <param name="tableAccessorName">Name of the generated accessor type</param>
    /// <param name="tableName">Fully qualified name of the table type</param>
    /// <param name="tableAccessor">C# source code for the accessor implementation</param>
    /// <param name="getter">C# property getter for accessing the accessor</param>
    public record struct GeneratedTableAccessor(
        string tableAccessorName,
        string tableName,
        string tableAccessor,
        string getter
    );

    /// <summary>
    /// Generates accessor implementations for all table accessors defined in this table declaration.
    /// Each accessor represents a different way to access or filter the table's data.
    /// </summary>
    /// <returns>Collection of Accessor records containing generated code for each accessor</returns>
    public IEnumerable<GeneratedTableAccessor> GenerateTableAccessors()
    {
        // Don't try to generate accessors if this table is a sum type.
        // We already emitted a diagnostic, and attempting to generate accessors will only result in more noisy errors.
        if (Kind is TypeKind.Sum)
        {
            yield break;
        }
        foreach (var v in TableAccessors)
        {
            var autoIncFields = Members.Where(m => m.GetAttrs(v).HasFlag(ColumnAttrs.AutoInc));

            var globalName = $"global::{FullName}";
            var iTable = $"global::SpacetimeDB.Internal.ITableView<{v.Name}, {globalName}>";
            yield return new(
                v.Name,
                globalName,
                $$$"""
            {{{SyntaxFacts.GetText(Visibility)}}} readonly struct {{{v.Name}}} : {{{iTable}}} {
                public static {{{globalName}}} ReadGenFields(System.IO.BinaryReader reader, {{{globalName}}} row) {
                    {{{string.Join(
                        "\n",
                        autoIncFields.Select(m =>
                            $$"""
                            if (row.{{m.Name}} == default)
                            {
                                row.{{m.Name}} = {{globalName}}.BSATN.{{m.Name}}{{TypeUse.BsatnFieldSuffix}}.Read(reader);
                            }
                            """
                        )
                    )}}}
                    return row;
                }

                public static SpacetimeDB.Internal.RawTableDefV9 MakeTableDesc(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                    Name: nameof({{{v.Name}}}),
                    ProductTypeRef: (uint) new {{{globalName}}}.BSATN().GetAlgebraicType(registrar).Ref_,
                    PrimaryKey: [{{{GetPrimaryKey(v)?.ToString() ?? ""}}}],
                    Indexes: [
                        {{{string.Join(
                            ",\n",
                            GetConstraints(v, ColumnAttrs.Unique)
                            .Select(c => c.ToIndex())
                            .Concat(GetIndexes(v))
                            .Select(b => b.GenerateIndexDef())
                        )}}}
                    ],
                    Constraints: {{{GenConstraintList(v, ColumnAttrs.Unique, $"{iTable}.MakeUniqueConstraint")}}},
                    Sequences: {{{GenConstraintList(v, ColumnAttrs.AutoInc, $"{iTable}.MakeSequence")}}},
                    Schedule: {{{(
                        v.Scheduled is { } scheduled
                        ? $"{iTable}.MakeSchedule(\"{scheduled.ReducerName}\", {scheduled.ScheduledAtColumn})"
                        : "null"
                    )}}},
                    TableType: SpacetimeDB.Internal.TableType.User,
                    TableAccess: SpacetimeDB.Internal.TableAccess.{{{(v.IsPublic ? "Public" : "Private")}}}
                );

                public ulong Count => {{{iTable}}}.DoCount();
                public IEnumerable<{{{globalName}}}> Iter() => {{{iTable}}}.DoIter();
                public {{{globalName}}} Insert({{{globalName}}} row) => {{{iTable}}}.DoInsert(row);
                public bool Delete({{{globalName}}} row) => {{{iTable}}}.DoDelete(row);

                {{{string.Join("\n", GenerateTableAccessorFilters(v))}}}
            }
            """,
                $"{SyntaxFacts.GetText(Visibility)} global::SpacetimeDB.Internal.TableHandles.{v.Name} {v.Name} => new();"
            );
        }
    }

    public record struct GeneratedReadOnlyAccessor(
        string tableAccessorName,
        string tableName,
        string readOnlyAccessor,
        string readOnlyGetter
    );

    public IEnumerable<GeneratedReadOnlyAccessor> GenerateReadOnlyAccessors()
    {
        if (Kind is TypeKind.Sum)
        {
            yield break;
        }

        foreach (var accessor in TableAccessors)
        {
            var globalName = $"global::{FullName}";

            var readOnlyIndexDecls = string.Join("\n", GenerateReadOnlyAccessorFilters(accessor));
            var visibility = SyntaxFacts.GetText(Visibility);
            yield return new(
                accessor.Name,
                globalName,
                $$$"""
                {{{visibility}}} sealed class {{{accessor.Name}}}ReadOnly
                    : global::SpacetimeDB.Internal.ReadOnlyTableView<{{{globalName}}}>
                {
                    internal {{{accessor.Name}}}ReadOnly() : base("{{{accessor.Name}}}") { }

                    public ulong Count => DoCount();

                    {{{readOnlyIndexDecls}}}
                }
                """,
                $"{visibility} global::SpacetimeDB.Internal.ViewHandles.{accessor.Name}ReadOnly {accessor.Name} => new();"
            );
        }
    }

    /// <summary>
    /// Represents a default value for a table field, used during table creation.
    /// </summary>
    /// <param name="tableName">Name of the table containing the field</param>
    /// <param name="columnId">Index of the column in the table</param>
    /// <param name="value">String representation of the default value</param>
    /// <param name="BSATNTypeName">BSATN Type name of the default value</param>
    public record struct FieldDefaultValue(
        string tableName,
        string columnId,
        string value,
        string BSATNTypeName
    );

    /// <summary>
    /// Generates default values for table fields with the [Default] attribute.
    /// These values are used when creating new rows without explicit values for the corresponding fields.
    /// </summary>
    /// <returns>Collection of default values for fields that specify them</returns>
    public IEnumerable<FieldDefaultValue> GenerateDefaultValues()
    {
        if (Kind is TypeKind.Sum)
        {
            yield break;
        }

        foreach (var tableAccessor in TableAccessors)
        {
            var members = string.Join(", ", Members.Select(m => m.Name));
            var fieldsWithDefaultValues = Members.Where(m =>
                m.GetAttrs(tableAccessor).HasFlag(ColumnAttrs.Default)
            );
            var defaultValueAttributes = string.Join(
                ", ",
                Members
                    .Where(m => m.GetAttrs(tableAccessor).HasFlag(ColumnAttrs.Default))
                    .Select(m => m.Attrs.FirstOrDefault(a => a.Mask == ColumnAttrs.Default))
            );

            var withDefaultValues =
                fieldsWithDefaultValues as ColumnDeclaration[] ?? fieldsWithDefaultValues.ToArray();
            foreach (var fieldsWithDefaultValue in withDefaultValues)
            {
                if (
                    fieldsWithDefaultValue.ColumnDefaultValue != null
                    && fieldsWithDefaultValue.Type.BSATNName != ""
                )
                {
                    // For enums, we'll need to wrap the default value in the enum type.
                    if (fieldsWithDefaultValue.Type.BSATNName.StartsWith("SpacetimeDB.BSATN.Enum"))
                    {
                        yield return new FieldDefaultValue(
                            tableAccessor.Name,
                            fieldsWithDefaultValue.ColumnIndex.ToString(),
                            $"({fieldsWithDefaultValue.Type.Name}){fieldsWithDefaultValue.ColumnDefaultValue}",
                            fieldsWithDefaultValue.Type.BSATNName
                        );
                    }
                    else
                    {
                        yield return new FieldDefaultValue(
                            tableAccessor.Name,
                            fieldsWithDefaultValue.ColumnIndex.ToString(),
                            fieldsWithDefaultValue.ColumnDefaultValue,
                            fieldsWithDefaultValue.Type.BSATNName
                        );
                    }
                }
            }
        }
    }

    public record Constraint(ColumnDeclaration Col, int Pos, ColumnAttrs Attr)
    {
        public TableIndex ToIndex() => new(new ColumnRef(Pos, Col.Name));
    }

    public IEnumerable<Constraint> GetConstraints(
        TableAccessor tableAccessor,
        ColumnAttrs filterByAttr = ~ColumnAttrs.UnSet
    ) =>
        Members
            // Important: the position must be stored here, before filtering.
            .Select((col, pos) => new Constraint(col, pos, col.GetAttrs(tableAccessor)))
            .Where(c => c.Attr.HasFlag(filterByAttr));

    public IEnumerable<TableIndex> GetIndexes(TableAccessor tableAccessor) =>
        Indexes
            .Concat(Members.SelectMany(m => m.Indexes))
            .Where(i => i.Table == null || i.Table == tableAccessor.Name);

    // Reimplementation of V8 -> V9 constraint conversion in Rust.
    // See https://github.com/clockworklabs/SpacetimeDB/blob/13a800e9f88cbe885b98eab9e45b0fcfd3ab7014/crates/schema/src/def/validate/v8.rs#L74-L78
    // and https://github.com/clockworklabs/SpacetimeDB/blob/13a800e9f88cbe885b98eab9e45b0fcfd3ab7014/crates/lib/src/db/raw_def/v8.rs#L460-L510
    private string GenConstraintList(
        TableAccessor tableAccessor,
        ColumnAttrs filterByAttr,
        string makeConstraintFn
    ) =>
        $$"""
        [
            {{string.Join(
                ",\n",
                GetConstraints(tableAccessor, filterByAttr)
                    .Select(pair => $"{makeConstraintFn}({pair.Pos})")
            )}}
        ]
        """;

    internal int? GetPrimaryKey(TableAccessor tableAccessor) =>
        GetConstraints(tableAccessor, ColumnAttrs.PrimaryKey)
            .Select(c => (int?)c.Pos)
            .SingleOrDefault();
}

/// <summary>
/// Represents a view method declaration in a module.
/// </summary>
record ViewDeclaration
{
    public readonly string Name;
    public readonly string FullName;
    public readonly bool IsAnonymous;
    public readonly bool IsPublic;
    public readonly TypeUse ReturnType;
    public readonly EquatableArray<MemberDeclaration> Parameters;
    public readonly Scope Scope;

    public ViewDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
    {
        var methodSyntax = (MethodDeclarationSyntax)context.TargetNode;
        var method = (IMethodSymbol)context.TargetSymbol;
        var attr = context.Attributes.Single().ParseAs<ViewAttribute>();
        var hasContextParam = method.Parameters.Length > 0;
        var firstParamType = hasContextParam ? method.Parameters[0].Type : null;
        var isAnonymousContext = firstParamType?.Name == "AnonymousViewContext";
        var hasArguments = method.Parameters.Length > 1;

        if (string.IsNullOrEmpty(attr.Name))
        {
            diag.Report(ErrorDescriptor.ViewMustHaveName, methodSyntax);
        }
        // TODO: Remove once Views support Private: Views must be Public currently
        if (!attr.Public)
        {
            diag.Report(ErrorDescriptor.ViewMustBePublic, methodSyntax);
        }
        if (hasArguments)
        {
            diag.Report(ErrorDescriptor.ViewArgsUnsupported, methodSyntax);
        }

        Name = attr.Name ?? method.Name;
        FullName = SymbolToName(method);
        IsPublic = attr.Public;
        IsAnonymous = isAnonymousContext;
        ReturnType = TypeUse.Parse(method, method.ReturnType, diag);
        Scope = new Scope(methodSyntax.Parent as MemberDeclarationSyntax);

        if (method.Parameters.Length == 0)
        {
            diag.Report(ErrorDescriptor.ViewContextParam, methodSyntax);
        }
        else if (
            method.Parameters[0].Type
            is not INamedTypeSymbol { Name: "ViewContext" or "AnonymousViewContext" }
        )
        {
            diag.Report(ErrorDescriptor.ViewContextParam, methodSyntax);
        }

        // Validate return type: must be Option<T> or Vec<T>
        if (
            !ReturnType.BSATNName.Contains("SpacetimeDB.BSATN.ValueOption")
            && !ReturnType.BSATNName.Contains("SpacetimeDB.BSATN.RefOption")
            && !ReturnType.BSATNName.Contains("SpacetimeDB.BSATN.List")
        )
        {
            diag.Report(ErrorDescriptor.ViewInvalidReturn, methodSyntax);
        }

        Parameters = new(
            method
                .Parameters.Skip(1)
                .Select(p => new MemberDeclaration(p, p.Type, diag))
                .ToImmutableArray()
        );
    }

    public string GenerateViewDef(uint Index) =>
        $$$"""
            new global::SpacetimeDB.Internal.RawViewDefV9(
                Name: "{{{Name}}}",
                Index: {{{Index}}},
                IsPublic: {{{IsPublic.ToString().ToLower()}}},
                IsAnonymous: {{{IsAnonymous.ToString().ToLower()}}},
                Params: [{{{MemberDeclaration.GenerateDefs(Parameters)}}}],
                ReturnType: new {{{ReturnType.BSATNName}}}().GetAlgebraicType(registrar)
            );
            """;

    /// <summary>
    /// Generates the class responsible for evaluating a view.
    /// If this is an anonymous view, the index corresponds to the position of this dispatcher in the `viewDispatchers` list of `RegisterView`.
    /// Otherwise it corresponds to the position of this dispatcher in the `anonymousViewDispatchers` list of `RegisterAnonymousView`.
    /// </summary>
    public string GenerateDispatcherClass(uint index)
    {
        var paramReads = string.Join(
            "\n                        ",
            Parameters.Select(p =>
                $"var {p.Name} = {p.Name}{TypeUse.BsatnFieldSuffix}.Read(reader);"
            )
        );

        var makeViewDefMethod = IsAnonymous ? "MakeAnonymousViewDef" : "MakeViewDef";

        var interfaceName = IsAnonymous
            ? "global::SpacetimeDB.Internal.IAnonymousView"
            : "global::SpacetimeDB.Internal.IView";
        var interfaceContext = IsAnonymous
            ? "global::SpacetimeDB.Internal.IAnonymousViewContext"
            : "global::SpacetimeDB.Internal.IViewContext";
        var concreteContext = IsAnonymous
            ? "SpacetimeDB.AnonymousViewContext"
            : "SpacetimeDB.ViewContext";

        var isOption =
            ReturnType.BSATNName.Contains("SpacetimeDB.BSATN.ValueOption")
            || ReturnType.BSATNName.Contains("SpacetimeDB.BSATN.RefOption");
        var writeOutput = isOption
            ? $$$"""
                    var listSerializer = {{{ReturnType.BSATNName}}}.GetListSerializer();
                    var listValue = ModuleRegistration.ToListOrEmpty(returnValue);
                    using var output = new System.IO.MemoryStream();
                    using var writer = new System.IO.BinaryWriter(output);
                    listSerializer.Write(writer, listValue);
                    return output.ToArray();
                """
            : $$$"""
                    {{{ReturnType.BSATNName}}} returnRW = new();
                    using var output = new System.IO.MemoryStream();
                    using var writer = new System.IO.BinaryWriter(output);
                    returnRW.Write(writer, returnValue);
                    return output.ToArray();            
                """;

        var invocationArgs =
            Parameters.Length == 0 ? "" : ", " + string.Join(", ", Parameters.Select(p => p.Name));
        return $$$"""
            sealed class {{{Name}}}ViewDispatcher : {{{interfaceName}}} {
                {{{MemberDeclaration.GenerateBsatnFields(Accessibility.Private, Parameters)}}}
                
                public SpacetimeDB.Internal.RawViewDefV9 {{{makeViewDefMethod}}}(SpacetimeDB.BSATN.ITypeRegistrar registrar)
                    => {{{GenerateViewDef(index)}}}

                public byte[] Invoke(
                    System.IO.BinaryReader reader,
                    {{{interfaceContext}}} ctx
                ) {
                    try {
                        {{{paramReads}}}
                        var returnValue = {{{FullName}}}(({{{concreteContext}}})ctx{{{invocationArgs}}});
                        {{{writeOutput}}}
                    } catch (System.Exception e) {
                        global::SpacetimeDB.Log.Error("Error in view '{{{Name}}}': " + e);
                        throw;
                    }
                }
            }
            """;
    }
}

/// <summary>
/// Represents a reducer method declaration in a module.
/// </summary>
record ReducerDeclaration
{
    public readonly string Name;
    public readonly ReducerKind Kind;
    public readonly string FullName;
    public readonly EquatableArray<MemberDeclaration> Args;
    public readonly Scope Scope;
    private readonly bool HasWrongSignature;

    public ReducerDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
    {
        var methodSyntax = (MethodDeclarationSyntax)context.TargetNode;
        var method = (IMethodSymbol)context.TargetSymbol;
        var attr = context.Attributes.Single().ParseAs<ReducerAttribute>();

        if (!method.ReturnsVoid)
        {
            diag.Report(ErrorDescriptor.ReducerReturnType, methodSyntax);
        }

        if (
            method.Parameters.FirstOrDefault()?.Type
            is not INamedTypeSymbol { Name: "ReducerContext" }
        )
        {
            diag.Report(ErrorDescriptor.ReducerContextParam, methodSyntax);
            HasWrongSignature = true;
        }

        Name = method.Name;
        if (Name.Length >= 2)
        {
            var prefix = Name[..2];
            if (prefix is "__" or "on" or "On")
            {
                diag.Report(ErrorDescriptor.ReducerReservedPrefix, (methodSyntax, prefix));
            }
        }

        Kind = attr.Kind;
        FullName = SymbolToName(method);
        Args = new(
            method
                .Parameters.Skip(1)
                .Select(p => new MemberDeclaration(p, p.Type, diag))
                .ToImmutableArray()
        );
        Scope = new Scope(methodSyntax.Parent as MemberDeclarationSyntax);
    }

    public string GenerateClass()
    {
        var invocation = HasWrongSignature
            ? "throw new System.InvalidOperationException()"
            : $"{FullName}({string.Join(
                ", ",
                Args.Select(a => $"{a.Name}{TypeUse.BsatnFieldSuffix}.Read(reader)").Prepend("(SpacetimeDB.ReducerContext)ctx")
            )})";

        return $$"""
             class {{Name}}: SpacetimeDB.Internal.IReducer {
                 {{MemberDeclaration.GenerateBsatnFields(Accessibility.Private, Args)}}

                 public SpacetimeDB.Internal.RawReducerDefV9 MakeReducerDef(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                     nameof({{Name}}),
                     [{{MemberDeclaration.GenerateDefs(Args)}}],
                     {{Kind switch
        {
            ReducerKind.Init => "SpacetimeDB.Internal.Lifecycle.Init",
            ReducerKind.ClientConnected => "SpacetimeDB.Internal.Lifecycle.OnConnect",
            ReducerKind.ClientDisconnected => "SpacetimeDB.Internal.Lifecycle.OnDisconnect",
            _ => "null"
        }}}
                 );

                 public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx) {
                     {{invocation}};
                 }
             }
             """;
    }

    public Scope.Extensions GenerateSchedule()
    {
        var extensions = new Scope.Extensions(Scope, FullName);

        // Mark the API as unstable. We use name `STDB_UNSTABLE` because:
        // 1. It's a close equivalent of the `unstable` Cargo feature in Rust.
        // 2. Our diagnostic IDs use either BSATN or STDB prefix depending on the package.
        // 3. We don't expect to mark individual experimental features with numeric IDs, so we don't use the standard 1234 suffix.
        extensions.Contents.Append(
            $$"""
            [System.Diagnostics.CodeAnalysis.Experimental("STDB_UNSTABLE")]
            public static void VolatileNonatomicScheduleImmediate{{Name}}({{string.Join(
                ", ",
                Args.Select(a => $"{a.Type.Name} {a.Name}")
            )}}) {
                using var stream = new MemoryStream();
                using var writer = new BinaryWriter(stream);
                {{string.Join(
                    "\n",
                    Args.Select(a => $"new {a.Type.ToBSATNString()}().Write(writer, {a.Name});")
                )}}
                SpacetimeDB.Internal.IReducer.VolatileNonatomicScheduleImmediate(nameof({{Name}}), stream);
            }
            """
        );

        return extensions;
    }
}

/// <summary>
/// Represents a procedure method declaration in a module.
/// </summary>
record ProcedureDeclaration
{
    public readonly string Name;
    public readonly string FullName;
    public readonly EquatableArray<MemberDeclaration> Args;
    public readonly Scope Scope;
    private readonly bool HasWrongSignature;
    public readonly TypeUse ReturnType;
    private readonly IMethodSymbol _methodSymbol;
    private readonly ITypeSymbol _returnTypeSymbol;
    private readonly DiagReporter _diag;

    public ProcedureDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
    {
        var methodSyntax = (MethodDeclarationSyntax)context.TargetNode;
        var method = (IMethodSymbol)context.TargetSymbol;
        var attr = context.Attributes.Single().ParseAs<ProcedureAttribute>();

        _methodSymbol = method;
        _returnTypeSymbol = method.ReturnType;
        _diag = diag;

        if (
            method.Parameters.FirstOrDefault()?.Type
            is not INamedTypeSymbol { Name: "ProcedureContext" }
        )
        {
            diag.Report(ErrorDescriptor.ProcedureContextParam, methodSyntax);
            HasWrongSignature = true;
        }

        Name = method.Name;
        if (Name.Length >= 2)
        {
            var prefix = Name[..2];
            if (prefix is "__" or "on" or "On")
            {
                diag.Report(ErrorDescriptor.ProcedureReservedPrefix, (methodSyntax, prefix));
            }
        }

        ReturnType = TypeUse.Parse(method, method.ReturnType, diag);

        FullName = SymbolToName(method);
        Args = new(
            method
                .Parameters.Skip(1)
                .Select(p => new MemberDeclaration(p, p.Type, diag))
                .ToImmutableArray()
        );
        Scope = new Scope(methodSyntax.Parent as MemberDeclarationSyntax);
    }

    public string GenerateClass()
    {
        var invocationArgs =
            Args.Length == 0 ? "" : ", " + string.Join(", ", Args.Select(a => a.Name));
        var invocation = $"{FullName}((SpacetimeDB.ProcedureContext)ctx{invocationArgs})";

        var hasTxOutcome = TryGetTxOutcomeType(out var txOutcomePayload);
        var hasTxResult = TryGetTxResultTypes(out var txResultPayload, out _);
        var hasTxWrapper = hasTxOutcome || hasTxResult;
        var txPayload = hasTxOutcome ? txOutcomePayload : txResultPayload;
        var txPayloadIsUnit = hasTxWrapper && txPayload.BSATNName == "SpacetimeDB.BSATN.Unit";

        string[] bodyLines;

        if (HasWrongSignature)
        {
            bodyLines = new[]
            {
                "throw new System.InvalidOperationException(\"Invalid procedure signature.\");",
            };
        }
        else if (hasTxWrapper)
        {
            var successLines = txPayloadIsUnit
                ? new[] { "return System.Array.Empty<byte>();" }
                : new[]
                {
                    "using var output = new MemoryStream();",
                    "using var writer = new BinaryWriter(output);",
                    "__txReturnRW.Write(writer, outcome.Value!);",
                    "return output.ToArray();",
                };

            bodyLines = new[]
            {
                $"var outcome = {invocation};",
                "if (!outcome.IsSuccess)",
                "{",
                "    throw outcome.Error ?? new System.InvalidOperationException(\"Transaction failed.\");",
                "}",
            }
                .Concat(successLines)
                .ToArray();
        }
        else if (ReturnType.Name == "SpacetimeDB.Unit")
        {
            bodyLines = new[] { $"{invocation};", "return System.Array.Empty<byte>();" };
        }
        else
        {
            var serializer = $"new {ReturnType.ToBSATNString()}()";
            bodyLines = new[]
            {
                $"var result = {invocation};",
                "using var output = new MemoryStream();",
                "using var writer = new BinaryWriter(output);",
                $"{serializer}.Write(writer, result);",
                "return output.ToArray();",
            };
        }

        var invokeBody = string.Join("\n", bodyLines.Select(line => $"                    {line}"));
        var paramReads =
            Args.Length == 0
                ? string.Empty
                : string.Join(
                    "\n",
                    Args.Select(a =>
                        $"                    var {a.Name} = {a.Name}{TypeUse.BsatnFieldSuffix}.Read(reader);"
                    )
                ) + "\n";

        var returnTypeExpr = hasTxWrapper
            ? (
                txPayloadIsUnit
                    ? "SpacetimeDB.BSATN.AlgebraicType.Unit"
                    : $"new {txPayload.ToBSATNString2()}().GetAlgebraicType(registrar)"
            )
            : (
                ReturnType.Name == "SpacetimeDB.Unit"
                    ? "SpacetimeDB.BSATN.AlgebraicType.Unit"
                    : $"new {ReturnType.ToBSATNString2()}().GetAlgebraicType(registrar)"
            );

        var classFields = MemberDeclaration.GenerateBsatnFields(Accessibility.Private, Args);
        if (hasTxWrapper && !txPayloadIsUnit)
        {
            classFields +=
                $"\n        private {txPayload.BSATNName} __txReturnRW = new {txPayload.BSATNName}();";
        }

        return $$$"""
            class {{{Name}}} : SpacetimeDB.Internal.IProcedure {
                {{{classFields}}}

                public SpacetimeDB.Internal.RawProcedureDefV9 MakeProcedureDef(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new(
                    nameof({{{Name}}}),
                    [{{{MemberDeclaration.GenerateDefs(Args)}}}],
                    {{{returnTypeExpr}}}
                );

                public byte[] Invoke(BinaryReader reader, SpacetimeDB.Internal.IProcedureContext ctx) {
                    {{{paramReads}}}{{{invokeBody}}}
                }
            }
            """;
    }

    public Scope.Extensions GenerateSchedule()
    {
        var extensions = new Scope.Extensions(Scope, FullName);

        // Mark the API as unstable. We use name `STDB_UNSTABLE` because:
        // 1. It's a close equivalent of the `unstable` Cargo feature in Rust.
        // 2. Our diagnostic IDs use either BSATN or STDB prefix depending on the package.
        // 3. We don't expect to mark individual experimental features with numeric IDs, so we don't use the standard 1234 suffix.
        extensions.Contents.Append(
            $$"""
            [System.Diagnostics.CodeAnalysis.Experimental("STDB_UNSTABLE")]
            public static void VolatileNonatomicScheduleImmediate{{Name}}({{string.Join(
                ", ",
                Args.Select(a => $"{a.Type.Name} {a.Name}")
            )}}) {
                using var stream = new MemoryStream();
                using var writer = new BinaryWriter(stream);
                {{string.Join(
                    "\n",
                    Args.Select(a => $"new {a.Type.ToBSATNString()}().Write(writer, {a.Name});")
                )}}
                SpacetimeDB.Internal.ProcedureExtensions.VolatileNonatomicScheduleImmediate(nameof({{Name}}), stream);
            }
            """
        );

        return extensions;
    }

    private bool TryGetTxOutcomeType(out TypeUse payloadType)
    {
        if (
            _returnTypeSymbol
                is INamedTypeSymbol
                {
                    Name: "TxOutcome",
                    ContainingType: { Name: "ProcedureContext" }
                } named
            && named.TypeArguments.Length == 1
        )
        {
            payloadType = TypeUse.Parse(_methodSymbol, named.TypeArguments[0], _diag);
            return true;
        }

        payloadType = default!;
        return false;
    }

    private bool TryGetTxResultTypes(out TypeUse payloadType, out TypeUse errorType)
    {
        if (
            _returnTypeSymbol
                is INamedTypeSymbol
                {
                    Name: "TxResult",
                    ContainingType: { Name: "ProcedureContext" }
                } named
            && named.TypeArguments.Length == 2
        )
        {
            payloadType = TypeUse.Parse(_methodSymbol, named.TypeArguments[0], _diag);
            errorType = TypeUse.Parse(_methodSymbol, named.TypeArguments[1], _diag);
            return true;
        }

        payloadType = default!;
        errorType = default!;
        return false;
    }
}

record ClientVisibilityFilterDeclaration
{
    public readonly string FullName;

    public string GlobalName => $"global::{FullName}";

    public ClientVisibilityFilterDeclaration(
        GeneratorAttributeSyntaxContext context,
        DiagReporter diag
    )
    {
        var fieldSymbol = (IFieldSymbol)context.TargetSymbol;

        if (
            !fieldSymbol.IsStatic
            || !fieldSymbol.IsReadOnly
            || fieldSymbol.DeclaredAccessibility != Accessibility.Public
        )
        {
            diag.Report(ErrorDescriptor.ClientVisibilityNotPublicStaticReadonly, fieldSymbol);
        }

        if (fieldSymbol.Type.ToString() is not "SpacetimeDB.Filter")
        {
            diag.Report(ErrorDescriptor.ClientVisibilityNotFilter, fieldSymbol);
        }

        FullName = SymbolToName(fieldSymbol);
    }
}

[Generator]
public class Module : IIncrementalGenerator
{
    /// <summary>
    /// Collects distinct items from a source sequence, ensuring no duplicate export names exist.
    /// </summary>
    /// <typeparam name="T">The type of items being collected</typeparam>
    /// <param name="kind">The category/type of items being collected (used for error messages)</param>
    /// <param name="context">The incremental generator context for reporting diagnostics</param>
    /// <param name="source">The source sequence of items to process</param>
    /// <param name="toExportName">Function to get the export name for an item (used for deduplication)</param>
    /// <param name="toFullName">Function to get the full name of an item (used for error messages)</param>
    /// <returns>An incremental value provider containing the distinct items</returns>
    private static IncrementalValueProvider<EquatableArray<T>> CollectDistinct<T>(
        string kind,
        IncrementalGeneratorInitializationContext context,
        IncrementalValuesProvider<T> source,
        Func<T, string> toExportName,
        Func<T, string> toFullName
    )
        where T : IEquatable<T>
    {
        var results = source
            .Collect()
            .Select(
                (collected, ct) =>
                    DiagReporter.With(
                        Location.None,
                        diag =>
                        {
                            var grouped = collected
                                .GroupBy(toExportName)
                                // Sort tables and reducers by name to match Rust behaviour.
                                // Not really important outside of testing, but for testing
                                // it matters because we commit module-bindings
                                // so they need to match 1:1 between different langs.
                                .OrderBy(g => g.Key);

                            foreach (var group in grouped.Where(group => group.Count() > 1))
                            {
                                diag.Report(
                                    ErrorDescriptor.DuplicateExport,
                                    (kind, group.Key, group.Select(toFullName))
                                );
                            }

                            return new EquatableArray<T>(
                                // Only return first item from each group.
                                // We already reported duplicates ourselves, and don't want MSBuild to produce lots of duplicate errors too.
                                grouped.Select(Enumerable.First).ToImmutableArray()
                            );
                        }
                    )
            );

        context.RegisterSourceOutput(
            results,
            (context, results) =>
            {
                foreach (var result in results.Diag)
                {
                    context.ReportDiagnostic(result);
                }
            }
        );

        return results
            .Select((result, ct) => result.Parsed)
            .WithTrackingName($"SpacetimeDB.{kind}.Collect");
    }

    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        var tables = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: typeof(TableAttribute).FullName,
                predicate: (node, ct) => true, // already covered by attribute restrictions
                transform: (context, ct) =>
                    context.ParseWithDiags(diag => new TableDeclaration(context, diag))
            )
            .ReportDiagnostics(context)
            .WithTrackingName("SpacetimeDB.Table.Parse");

        tables
            .Select((t, ct) => t.ToExtensions())
            .WithTrackingName("SpacetimeDB.Table.GenerateExtensions")
            .RegisterSourceOutputs(context);

        var viewDeclarations = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: typeof(ViewAttribute).FullName!,
                predicate: (node, _) => node is MethodDeclarationSyntax,
                transform: (ctx, _) => ctx.ParseWithDiags(diag => new ViewDeclaration(ctx, diag))
            )
            .ReportDiagnostics(context)
            .WithTrackingName("SpacetimeDB.View.Parse");

        var views = CollectDistinct(
            "View",
            context,
            viewDeclarations,
            v => v.Name,
            v => v.FullName
        );

        var reducers = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: typeof(ReducerAttribute).FullName,
                predicate: (node, ct) => true, // already covered by attribute restrictions
                transform: (context, ct) =>
                    context.ParseWithDiags(diag => new ReducerDeclaration(context, diag))
            )
            .ReportDiagnostics(context)
            .WithTrackingName("SpacetimeDB.Reducer.Parse");

        reducers
            .Select((r, ct) => r.GenerateSchedule())
            .WithTrackingName("SpacetimeDB.Reducer.GenerateSchedule")
            .RegisterSourceOutputs(context);

        context.RegisterSourceOutput(
            reducers
                .Where(r => r.Kind != ReducerKind.UserDefined)
                .Collect()
                .SelectMany(
                    (reducers, ct) =>
                        reducers
                            .GroupBy(r => r.Kind)
                            .Where(group => group.Count() > 1)
                            .Select(group =>
                                ErrorDescriptor.DuplicateSpecialReducer.ToDiag(
                                    (group.Key, group.Select(r => r.FullName))
                                )
                            )
                ),
            (ctx, diag) => ctx.ReportDiagnostic(diag)
        );

        var addReducers = CollectDistinct(
            "Reducer",
            context,
            reducers
                .Select((r, ct) => (r.Name, r.FullName, Class: r.GenerateClass()))
                .WithTrackingName("SpacetimeDB.Reducer.GenerateClass"),
            r => r.Name,
            r => r.FullName
        );

        var procedures = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: typeof(ProcedureAttribute).FullName,
                predicate: (node, ct) => true, // already covered by attribute restrictions
                transform: (context, ct) =>
                    context.ParseWithDiags(diag => new ProcedureDeclaration(context, diag))
            )
            .ReportDiagnostics(context)
            .WithTrackingName("SpacetimeDB.Procedure.Parse");

        procedures
            .Select((p, ct) => p.GenerateSchedule())
            .WithTrackingName("SpacetimeDB.Procedure.GenerateSchedule")
            .RegisterSourceOutputs(context);

        var addProcedures = CollectDistinct(
            "Procedure",
            context,
            procedures
                .Select((p, ct) => (p.Name, p.FullName, Class: p.GenerateClass()))
                .WithTrackingName("SpacetimeDB.Procedure.GenerateClass"),
            p => p.Name,
            p => p.FullName
        );

        var tableAccessors = CollectDistinct(
            "Table",
            context,
            tables
                .SelectMany((t, ct) => t.GenerateTableAccessors())
                .WithTrackingName("SpacetimeDB.Table.GenerateTableAccessors"),
            v => v.tableAccessorName,
            v => v.tableName
        );

        var readOnlyAccessors = CollectDistinct(
            "TableReadOnly",
            context,
            tables
                .SelectMany((t, ct) => t.GenerateReadOnlyAccessors())
                .WithTrackingName("SpacetimeDB.Table.GenerateReadOnlyAccessors"),
            v => v.tableAccessorName + "ReadOnly",
            v => v.tableName
        );

        var rlsFilters = context
            .SyntaxProvider.ForAttributeWithMetadataName(
#pragma warning disable STDB_UNSTABLE
                fullyQualifiedMetadataName: typeof(ClientVisibilityFilterAttribute).FullName,
#pragma warning restore STDB_UNSTABLE
                predicate: (node, ct) => true,
                transform: (context, ct) =>
                    context.ParseWithDiags(diag => new ClientVisibilityFilterDeclaration(
                        context,
                        diag
                    ))
            )
            .ReportDiagnostics(context)
            .WithTrackingName("SpacetimeDB.ClientVisibilityFilter.Parse");

        var rlsFiltersArray = CollectDistinct(
            "ClientVisibilityFilter",
            context,
            rlsFilters,
            (f) => f.FullName,
            (f) => f.FullName
        );

        var columnDefaultValues = CollectDistinct(
            "ColumnDefaultValues",
            context,
            tables
                .SelectMany((t, ct) => t.GenerateDefaultValues())
                .WithTrackingName("SpacetimeDB.Table.GenerateDefaultValues"),
            v => v.tableName + "_" + v.columnId,
            v => v.tableName + "_" + v.columnId
        );

        // Register the generated source code with the compilation context as part of module publishing
        // Once the compilation is complete, the generated code will be used to create tables and reducers in the database
        context.RegisterSourceOutput(
            tableAccessors
                .Combine(addReducers)
                .Combine(addProcedures)
                .Combine(readOnlyAccessors)
                .Combine(views)
                .Combine(rlsFiltersArray)
                .Combine(columnDefaultValues),
            (context, tuple) =>
            {
                var (
                    (
                        (
                            (((tableAccessors, addReducers), addProcedures), readOnlyAccessors),
                            views
                        ),
                        rlsFilters
                    ),
                    columnDefaultValues
                ) = tuple;
                // Don't generate the FFI boilerplate if there are no tables or reducers.
                if (
                    tableAccessors.Array.IsEmpty
                    && addReducers.Array.IsEmpty
                    && addProcedures.Array.IsEmpty
                )
                {
                    return;
                }
                context.AddSource(
                    "FFI.cs",
                    $$"""
                    // <auto-generated />
                    #nullable enable
                    // The runtime already defines SpacetimeDB.Internal.LocalReadOnly in Runtime\Internal\Module.cs as an empty partial type.
                    // This is needed so every module build doesn't generate a full LocalReadOnly type, but just adds on to the existing.
                    // We extend it here with generated table accessors, and just need to suppress the duplicate-type warning.
                    #pragma warning disable CS0436
                    #pragma warning disable STDB_UNSTABLE

                    using System.Diagnostics.CodeAnalysis;
                    using System.Runtime.CompilerServices;
                    using System.Runtime.InteropServices;
                    using Internal = SpacetimeDB.Internal;
                    using TxContext = SpacetimeDB.Internal.TxContext;

                    namespace SpacetimeDB {
                        public sealed record ReducerContext : DbContext<Local>, Internal.IReducerContext {
                            public readonly Identity Sender;
                            public readonly ConnectionId? ConnectionId;
                            public readonly Random Rng;
                            public readonly Timestamp Timestamp;
                            public readonly AuthCtx SenderAuth;
                            // **Note:** must be 0..=u32::MAX
                            internal int CounterUuid;
                            // We need this property to be non-static for parity with client SDK.
                            public Identity Identity => Internal.IReducerContext.GetIdentity();

                            internal ReducerContext(Identity identity, ConnectionId? connectionId, Random random,
                                            Timestamp time, AuthCtx? senderAuth = null)
                            {
                                Sender = identity;
                                ConnectionId = connectionId;
                                Rng = random;
                                Timestamp = time;
                                SenderAuth = senderAuth ?? AuthCtx.BuildFromSystemTables(connectionId, identity);
                                CounterUuid = 0;
                            }
                            /// <summary>
                            /// Create a new random <see cref="Uuid"/> `v4` using the built-in RNG.
                            /// </summary>
                            /// <remarks>
                            /// This method fills the random bytes using the context RNG.
                            /// </remarks>
                            /// <example>
                            /// <code>
                            /// var uuid = ctx.NewUuidV4();
                            /// Log.Info(uuid);
                            /// </code>
                            /// </example>
                            public Uuid NewUuidV4()
                            {
                                var bytes = new byte[16];
                                Rng.NextBytes(bytes);
                                return Uuid.FromRandomBytesV4(bytes);
                            }

                            /// <summary>
                            /// Create a new sortable <see cref="Uuid"/> `v7` using the built-in RNG, monotonic counter,
                            /// and timestamp.
                            /// </summary>
                            /// <returns>
                            /// A newly generated <see cref="Uuid"/> `v7` that is monotonically ordered
                            /// and suitable for use as a primary key or for ordered storage.
                            /// </returns>
                            /// <exception cref="Exception">
                            /// Thrown if <see cref="Uuid"/> generation fails.
                            /// </exception>
                            /// <example>
                            /// <code>
                            /// [SpacetimeDB.Reducer]
                            /// public static Guid GenerateUuidV7(ReducerContext ctx)
                            /// {
                            ///     Guid uuid = ctx.NewUuidV7();
                            ///     Log.Info(uuid);
                            /// }
                            /// </code>
                            /// </example>
                            public Uuid NewUuidV7()
                            {
                                var bytes = new byte[4];
                                Rng.NextBytes(bytes);
                                return Uuid.FromCounterV7(ref CounterUuid, Timestamp, bytes);
                            }
                        }
                        
                        public sealed partial class ProcedureContext : global::SpacetimeDB.ProcedureContextBase {
                            private readonly Local _db = new();

                            internal ProcedureContext(Identity identity, ConnectionId? connectionId, Random random, Timestamp time)
                                : base(identity, connectionId, random, time) {}

                            protected override global::SpacetimeDB.LocalBase CreateLocal() => _db;
                            protected override global::SpacetimeDB.ProcedureTxContextBase CreateTxContext(Internal.TxContext inner) =>
                                _cached ??= new ProcedureTxContext(inner);

                            private ProcedureTxContext? _cached;

                            [Experimental("STDB_UNSTABLE")]
                            public Local Db => _db;
                            
                            [Experimental("STDB_UNSTABLE")]
                            public TResult WithTx<TResult>(Func<ProcedureTxContext, TResult> body) =>
                                base.WithTx(tx => body((ProcedureTxContext)tx));
                            
                            [Experimental("STDB_UNSTABLE")]
                            public TxOutcome<TResult> TryWithTx<TResult, TError>(
                                Func<ProcedureTxContext, Result<TResult, TError>> body)
                                where TError : Exception =>
                                base.TryWithTx(tx => body((ProcedureTxContext)tx));

                            /// <summary>
                            /// Create a new random <see cref="Uuid"/> `v4` using the built-in RNG.
                            /// </summary>
                            /// <remarks>
                            /// This method fills the random bytes using the context RNG.
                            /// </remarks>
                            /// <example>
                            /// <code>
                            /// var uuid = ctx.NewUuidV4();
                            /// Log.Info(uuid);
                            /// </code>
                            /// </example>
                            public Uuid NewUuidV4()
                            {
                                var bytes = new byte[16];
                                Rng.NextBytes(bytes);
                                return Uuid.FromRandomBytesV4(bytes);
                            }

                            /// <summary>
                            /// Create a new sortable <see cref="Uuid"/> `v7` using the built-in RNG, monotonic counter,
                            /// and timestamp.
                            /// </summary>
                            /// <returns>
                            /// A newly generated <see cref="Uuid"/> `v7` that is monotonically ordered
                            /// and suitable for use as a primary key or for ordered storage.
                            /// </returns>
                            /// <exception cref="Exception">
                            /// Thrown if UUID generation fails.
                            /// </exception>
                            /// <example>
                            /// <code>
                            /// [SpacetimeDB.Procedure]
                            /// public static Guid GenerateUuidV7(ReducerContext ctx)
                            /// {
                            ///     Guid uuid = ctx.NewUuidV7();
                            ///     Log.Info(uuid);
                            /// }
                            /// </code>
                            /// </example>
                            public Uuid NewUuidV7()
                            {
                                var bytes = new byte[4];
                                Rng.NextBytes(bytes);
                                return Uuid.FromCounterV7(ref CounterUuid, Timestamp, bytes);
                            }
                        }

                        [Experimental("STDB_UNSTABLE")]
                        public sealed class ProcedureTxContext : global::SpacetimeDB.ProcedureTxContextBase {
                            internal ProcedureTxContext(Internal.TxContext inner) : base(inner) {}

                            public new Local Db => (Local)base.Db;
                        }

                        public sealed class Local : global::SpacetimeDB.LocalBase {
                            {{string.Join("\n", tableAccessors.Select(v => v.getter))}}
                        }
                        
                        public sealed record ViewContext : DbContext<Internal.LocalReadOnly>, Internal.IViewContext 
                        {
                            public Identity Sender { get; }
                        
                            internal ViewContext(Identity sender, Internal.LocalReadOnly db)
                                : base(db)
                            {
                                Sender = sender;
                            }
                        }
                        
                        public sealed record AnonymousViewContext : DbContext<Internal.LocalReadOnly>, Internal.IAnonymousViewContext 
                        {
                            internal AnonymousViewContext(Internal.LocalReadOnly db)
                                : base(db) { }
                        }
                    }
                    
                    namespace SpacetimeDB.Internal.TableHandles {
                        {{string.Join("\n", tableAccessors.Select(v => v.tableAccessor))}}
                    }
                    
                    {{string.Join("\n",
                        views.Array.Where(v => !v.IsAnonymous)
                            .Select((v, i) => v.GenerateDispatcherClass((uint)i))
                            .Concat(
                                views.Array.Where(v => v.IsAnonymous)
                                    .Select((v, i) => v.GenerateDispatcherClass((uint)i))
                            )
                    )}}
                        
                    namespace SpacetimeDB.Internal.ViewHandles {
                        {{string.Join("\n", readOnlyAccessors.Array.Select(v => v.readOnlyAccessor))}}
                    }
                    
                    namespace SpacetimeDB.Internal {
                        public sealed partial class LocalReadOnly {
                            {{string.Join("\n", readOnlyAccessors.Select(v => v.readOnlyGetter))}}
                        }
                    }
                    
                    static class ModuleRegistration {
                        {{string.Join("\n", addReducers.Select(r => r.Class))}}
                        
                        {{string.Join("\n", addProcedures.Select(r => r.Class))}}

                        public static List<T> ToListOrEmpty<T>(T? value) where T : struct
                                => value is null ? new List<T>() : new List<T> { value.Value };

                        public static List<T> ToListOrEmpty<T>(T? value) where T : class
                                => value is null ? new List<T>() : new List<T> { value };

                    #if EXPERIMENTAL_WASM_AOT
                        // In AOT mode we're building a library.
                        // Main method won't be called automatically, so we need to export it as a preinit function.
                        [UnmanagedCallersOnly(EntryPoint = "__preinit__10_init_csharp")]
                    #else
                        // Prevent trimming of FFI exports that are invoked from C and not visible to C# trimmer.
                        [DynamicDependency(DynamicallyAccessedMemberTypes.PublicMethods, typeof(SpacetimeDB.Internal.Module))]
                    #endif
                        public static void Main() {
                          SpacetimeDB.Internal.Module.SetReducerContextConstructor((identity, connectionId, random, time) => new SpacetimeDB.ReducerContext(identity, connectionId, random, time));
                          SpacetimeDB.Internal.Module.SetViewContextConstructor(identity => new SpacetimeDB.ViewContext(identity, new SpacetimeDB.Internal.LocalReadOnly()));
                          SpacetimeDB.Internal.Module.SetAnonymousViewContextConstructor(() => new SpacetimeDB.AnonymousViewContext(new SpacetimeDB.Internal.LocalReadOnly()));
                          SpacetimeDB.Internal.Module.SetProcedureContextConstructor((identity, connectionId, random, time) => new SpacetimeDB.ProcedureContext(identity, connectionId, random, time));
                          var __memoryStream = new MemoryStream();
                          var __writer = new BinaryWriter(__memoryStream);

                            {{string.Join(
                                "\n",
                                addReducers.Select(r =>
                                    $"SpacetimeDB.Internal.Module.RegisterReducer<{r.Name}>();"
                                )
                            )}}
                            {{string.Join(
                                "\n",
                                addProcedures.Select(r =>
                                    $"SpacetimeDB.Internal.Module.RegisterProcedure<{r.Name}>();"
                                )
                            )}}

                            // IMPORTANT: The order in which we register views matters.
                            // It must correspond to the order in which we call `GenerateDispatcherClass`.
                            // See the comment on `GenerateDispatcherClass` for more explanation.
                            {{string.Join("\n",
                                views.Array.Where(v => !v.IsAnonymous)
                                    .Select(v => $"SpacetimeDB.Internal.Module.RegisterView<{v.Name}ViewDispatcher>();")
                                    .Concat(
                                        views.Array.Where(v => v.IsAnonymous)
                                            .Select(v => $"SpacetimeDB.Internal.Module.RegisterAnonymousView<{v.Name}ViewDispatcher>();")
                                    )
                            )}}                            

                            {{string.Join(
                                "\n",
                                tableAccessors.Select(t => $"SpacetimeDB.Internal.Module.RegisterTable<{t.tableName}, SpacetimeDB.Internal.TableHandles.{t.tableAccessorName}>();")
                            )}}
                            {{string.Join(
                                "\n",
                                rlsFilters.Select(f => $"SpacetimeDB.Internal.Module.RegisterClientVisibilityFilter({f.GlobalName});")
                            )}}
                            {{string.Join(
                                "\n",
                                columnDefaultValues.Select(d =>
                                    "{\n"
                                         + $"var value = new {d.BSATNTypeName}();\n"
                                         + "__memoryStream.Position = 0;\n"
                                         + "__memoryStream.SetLength(0);\n"
                                         + $"value.Write(__writer, {d.value});\n"
                                         + "var array = __memoryStream.ToArray();\n"
                                         + $"SpacetimeDB.Internal.Module.RegisterTableDefaultValue(\"{d.tableName}\", {d.columnId}, array);"
                                         + "\n}\n")
                            )}}
                        }

                    // Exports only work from the main assembly, so we need to generate forwarding methods.
                    #if EXPERIMENTAL_WASM_AOT
                        [UnmanagedCallersOnly(EntryPoint = "__describe_module__")]
                        public static void __describe_module__(SpacetimeDB.Internal.BytesSink d) => SpacetimeDB.Internal.Module.__describe_module__(d);

                        [UnmanagedCallersOnly(EntryPoint = "__call_reducer__")]
                        public static SpacetimeDB.Internal.Errno __call_reducer__(
                            uint id,
                            ulong sender_0,
                            ulong sender_1,
                            ulong sender_2,
                            ulong sender_3,
                            ulong conn_id_0,
                            ulong conn_id_1,
                            SpacetimeDB.Timestamp timestamp,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink error
                        ) => SpacetimeDB.Internal.Module.__call_reducer__(
                            id,
                            sender_0,
                            sender_1,
                            sender_2,
                            sender_3,
                            conn_id_0,
                            conn_id_1,
                            timestamp,
                            args,
                            error
                        );
                        
                        [UnmanagedCallersOnly(EntryPoint = "__call_procedure__")]
                        public static SpacetimeDB.Internal.Errno __call_procedure__(
                            uint id,
                            ulong sender_0,
                            ulong sender_1,
                            ulong sender_2,
                            ulong sender_3,
                            ulong conn_id_0,
                            ulong conn_id_1,
                            SpacetimeDB.Timestamp timestamp,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink result_sink
                        ) => SpacetimeDB.Internal.Module.__call_procedure__(
                            id,
                            sender_0,
                            sender_1,
                            sender_2,
                            sender_3,
                            conn_id_0,
                            conn_id_1,
                            timestamp,
                            args,
                            result_sink
                        );
                        
                        [UnmanagedCallersOnly(EntryPoint = "__call_view__")]
                        public static SpacetimeDB.Internal.Errno __call_view__(
                            uint id,
                            ulong sender_0,
                            ulong sender_1,
                            ulong sender_2,
                            ulong sender_3,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink sink
                        ) => SpacetimeDB.Internal.Module.__call_view__(
                            id,
                            sender_0,
                            sender_1,
                            sender_2,
                            sender_3,
                            args,
                            sink
                        );

                        [UnmanagedCallersOnly(EntryPoint = "__call_view_anon__")]
                        public static SpacetimeDB.Internal.Errno __call_view_anon__(
                            uint id,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink sink
                        ) => SpacetimeDB.Internal.Module.__call_view_anon__(
                            id,
                            args,
                            sink
                        );                                                
                    #endif
                    }
                    
                    #pragma warning restore STDB_UNSTABLE
                    #pragma warning restore CS0436
                    """
                );
            }
        );
    }
}
