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
    public readonly EquatableArray<ViewIndex> Indexes;
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
                .Where(ViewIndex.CanParse)
                .Select(a => new ViewIndex(new ColumnRef(index, field.Name), a, diag))
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
                            or "SpacetimeDB.Identity",
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

    public ColumnAttrs GetAttrs(TableView view) =>
        CombineColumnAttrs(Attrs.Where(x => x.Table == null || x.Table == view.Name));

    // For the `TableDesc` constructor.
    public string GenerateColumnDef() =>
        $"new (nameof({Name}), BSATN.{Name}{TypeUse.BsatnFieldSuffix}.GetAlgebraicType(registrar))";
}

record Scheduled(string ReducerName, int ScheduledAtColumn);

record TableView
{
    public readonly string Name;
    public readonly bool IsPublic;
    public readonly Scheduled? Scheduled;

    public TableView(TableDeclaration table, AttributeData data, DiagReporter diag)
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

enum ViewIndexType
{
    BTree,
}

/// <summary>
/// Represents an index on a database table view, used to optimize queries.
/// Supports B-tree indexing (and potentially other types in the future).
/// </summary>
record ViewIndex
{
    public readonly EquatableArray<ColumnRef> Columns;
    public readonly string? Table;
    public readonly string AccessorName;
    public readonly ViewIndexType Type;

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
    private ViewIndex(
        string? accessorName,
        ImmutableArray<ColumnRef> columns,
        string? tableName,
        ViewIndexType type
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
    public ViewIndex(ColumnRef col)
        : this(
            null,
            ImmutableArray.Create(col),
            null,
            ViewIndexType.BTree // this might become hash in the future
        ) { }

    /// <summary>
    /// Creates an index with the given attribute and columns.
    /// Used internally by other constructors that parse attributes.
    /// </summary>
    private ViewIndex(Index.BTreeAttribute attr, ImmutableArray<ColumnRef> columns)
        : this(attr.Name, columns, attr.Table, ViewIndexType.BTree) { }

    /// <summary>
    /// Creates an index from a table declaration and attribute data.
    /// Validates the index configuration and reports any errors through the diag reporter.
    /// </summary>
    private ViewIndex(
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
    public ViewIndex(TableDeclaration table, AttributeData data, DiagReporter diag)
        : this(table, data.ParseAs<Index.BTreeAttribute>(), data, diag) { }

    /// <summary>
    /// Creates an index for a single column with attribute data.
    /// Validates that no additional columns were specified in the attribute.
    /// </summary>
    private ViewIndex(
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
    public ViewIndex(ColumnRef col, AttributeData data, DiagReporter diag)
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

    public string StandardIndexName(TableView view) => view.Name + StandardNameSuffix;
}

/// <summary>
/// Represents a table declaration in a module.
/// Handles table metadata, views, indexes, and column declarations for code generation.
/// </summary>
record TableDeclaration : BaseTypeDeclaration<ColumnDeclaration>
{
    public readonly Accessibility Visibility;
    public readonly EquatableArray<TableView> Views;
    public readonly EquatableArray<ViewIndex> Indexes;

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

        Views = new(
            context.Attributes.Select(a => new TableView(this, a, diag)).ToImmutableArray()
        );
        Indexes = new(
            context
                .TargetSymbol.GetAttributes()
                .Where(ViewIndex.CanParse)
                .Select(a => new ViewIndex(this, a, diag))
                .ToImmutableArray()
        );
    }

    protected override ColumnDeclaration ConvertMember(
        int index,
        IFieldSymbol field,
        DiagReporter diag
    ) => new(FullName, index, field, diag);

    public IEnumerable<string> GenerateViewFilters(TableView view)
    {
        var vis = SyntaxFacts.GetText(Visibility);
        var globalName = $"global::{FullName}";

        foreach (var ct in GetConstraints(view, ColumnAttrs.Unique))
        {
            var f = ct.Col;
            if (!f.IsEquatable)
            {
                // Skip - we already emitted diagnostic for this during parsing, and generated code would
                // only produce a lot of noisy typechecking errors.
                continue;
            }
            var standardIndexName = ct.ToIndex().StandardIndexName(view);
            yield return $$"""
                {{vis}} sealed class {{f.Name}}UniqueIndex : UniqueIndex<{{view.Name}}, {{globalName}}, {{f.Type.Name}}, {{f.Type.BSATNName}}> {
                    internal {{f.Name}}UniqueIndex() : base("{{standardIndexName}}") {}
                    // Important: don't move this to the base class.
                    // C# generics don't play well with nullable types and can't accept both struct-type-based and class-type-based
                    // `globalName` in one generic definition, leading to buggy `Row?` expansion for either one or another.
                    public {{globalName}}? Find({{f.Type.Name}} key) => DoFilter(key).Cast<{{globalName}}?>().SingleOrDefault();
                    public {{globalName}} Update({{globalName}} row) => DoUpdate(row);
                }
                {{vis}} {{f.Name}}UniqueIndex {{f.Name}} => new();
                """;
        }

        foreach (var index in GetIndexes(view))
        {
            var name = index.AccessorName;

            // Skip bad declarations. Empty name means no columns, which we have already reported with a meaningful error.
            // Emitting this will result in further compilation errors due to missing property name.
            if (name == "")
            {
                continue;
            }

            var members = index.Columns.Select(c => Members[c.Index]).ToArray();
            var standardIndexName = index.StandardIndexName(view);

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
                var lastBounds = $"Bound<{members[n].Type.Name}> {members[n].Name}";
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

    /// <summary>
    /// Represents a generated view for a table, providing different access patterns
    /// and visibility levels for the underlying table data.
    /// </summary>
    /// <param name="viewName">Name of the generated view type</param>
    /// <param name="tableName">Fully qualified name of the table type</param>
    /// <param name="view">C# source code for the view implementation</param>
    /// <param name="getter">C# property getter for accessing the view</param>
    public record struct View(string viewName, string tableName, string view, string getter);

    /// <summary>
    /// Generates view implementations for all table views defined in this table declaration.
    /// Each view represents a different way to access or filter the table's data.
    /// </summary>
    /// <returns>Collection of View records containing generated code for each view</returns>
    public IEnumerable<View> GenerateViews()
    {
        // Don't try to generate views if this table is a sum type.
        // We already emitted a diagnostic, and attempting to generate views will only result in more noisy errors.
        if (Kind is TypeKind.Sum)
        {
            yield break;
        }
        foreach (var v in Views)
        {
            var autoIncFields = Members.Where(m => m.GetAttrs(v).HasFlag(ColumnAttrs.AutoInc));

            var globalName = $"global::{FullName}";
            var iTable = $"SpacetimeDB.Internal.ITableView<{v.Name}, {globalName}>";
            yield return new(
                v.Name,
                globalName,
                $$"""
            {{SyntaxFacts.GetText(Visibility)}} readonly struct {{v.Name}} : {{iTable}} {
                static {{globalName}} {{iTable}}.ReadGenFields(System.IO.BinaryReader reader, {{globalName}} row) {
                    {{string.Join(
                        "\n",
                        autoIncFields.Select(m =>
                            $$"""
                            if (row.{{m.Name}} == default)
                            {
                                row.{{m.Name}} = {{globalName}}.BSATN.{{m.Name}}{{TypeUse.BsatnFieldSuffix}}.Read(reader);
                            }
                            """
                        )
                    )}}
                    return row;
                }

                static SpacetimeDB.Internal.RawTableDefV9 {{iTable}}.MakeTableDesc(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                    Name: nameof({{v.Name}}),
                    ProductTypeRef: (uint) new {{globalName}}.BSATN().GetAlgebraicType(registrar).Ref_,
                    PrimaryKey: [{{GetPrimaryKey(v)?.ToString() ?? ""}}],
                    Indexes: [
                        {{string.Join(
                            ",\n",
                            GetConstraints(v, ColumnAttrs.Unique)
                            .Select(c => c.ToIndex())
                            .Concat(GetIndexes(v))
                            .Select(b => b.GenerateIndexDef())
                        )}}
                    ],
                    Constraints: {{GenConstraintList(v, ColumnAttrs.Unique, $"{iTable}.MakeUniqueConstraint")}},
                    Sequences: {{GenConstraintList(v, ColumnAttrs.AutoInc, $"{iTable}.MakeSequence")}},
                    Schedule: {{(
                        v.Scheduled is { } scheduled
                        ? $"{iTable}.MakeSchedule(\"{scheduled.ReducerName}\", {scheduled.ScheduledAtColumn})"
                        : "null"
                    )}},
                    TableType: SpacetimeDB.Internal.TableType.User,
                    TableAccess: SpacetimeDB.Internal.TableAccess.{{(v.IsPublic ? "Public" : "Private")}}
                );

                public ulong Count => {{iTable}}.DoCount();
                public IEnumerable<{{globalName}}> Iter() => {{iTable}}.DoIter();
                public {{globalName}} Insert({{globalName}} row) => {{iTable}}.DoInsert(row);
                public bool Delete({{globalName}} row) => {{iTable}}.DoDelete(row);

                {{string.Join("\n", GenerateViewFilters(v))}}
            }
            """,
                $"{SyntaxFacts.GetText(Visibility)} Internal.TableHandles.{v.Name} {v.Name} => new();"
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

        foreach (var view in Views)
        {
            var members = string.Join(", ", Members.Select(m => m.Name));
            var fieldsWithDefaultValues = Members.Where(m =>
                m.GetAttrs(view).HasFlag(ColumnAttrs.Default)
            );
            var defaultValueAttributes = string.Join(
                ", ",
                Members
                    .Where(m => m.GetAttrs(view).HasFlag(ColumnAttrs.Default))
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
                            view.Name,
                            fieldsWithDefaultValue.ColumnIndex.ToString(),
                            $"({fieldsWithDefaultValue.Type.Name}){fieldsWithDefaultValue.ColumnDefaultValue}",
                            fieldsWithDefaultValue.Type.BSATNName
                        );
                    }
                    else
                    {
                        yield return new FieldDefaultValue(
                            view.Name,
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
        public ViewIndex ToIndex() => new(new ColumnRef(Pos, Col.Name));
    }

    public IEnumerable<Constraint> GetConstraints(
        TableView view,
        ColumnAttrs filterByAttr = ~ColumnAttrs.UnSet
    ) =>
        Members
            // Important: the position must be stored here, before filtering.
            .Select((col, pos) => new Constraint(col, pos, col.GetAttrs(view)))
            .Where(c => c.Attr.HasFlag(filterByAttr));

    public IEnumerable<ViewIndex> GetIndexes(TableView view) =>
        Indexes
            .Concat(Members.SelectMany(m => m.Indexes))
            .Where(i => i.Table == null || i.Table == view.Name);

    // Reimplementation of V8 -> V9 constraint conversion in Rust.
    // See https://github.com/clockworklabs/SpacetimeDB/blob/13a800e9f88cbe885b98eab9e45b0fcfd3ab7014/crates/schema/src/def/validate/v8.rs#L74-L78
    // and https://github.com/clockworklabs/SpacetimeDB/blob/13a800e9f88cbe885b98eab9e45b0fcfd3ab7014/crates/lib/src/db/raw_def/v8.rs#L460-L510
    private string GenConstraintList(
        TableView view,
        ColumnAttrs filterByAttr,
        string makeConstraintFn
    ) =>
        $$"""
        [
            {{string.Join(
                ",\n",
                GetConstraints(view, filterByAttr)
                    .Select(pair => $"{makeConstraintFn}({pair.Pos})")
            )}}
        ]
        """;

    internal int? GetPrimaryKey(TableView view) =>
        GetConstraints(view, ColumnAttrs.PrimaryKey).Select(c => (int?)c.Pos).SingleOrDefault();
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
                    Args.Select(a => $"new {a.Type.BSATNName}().Write(writer, {a.Name});")
                )}}
                SpacetimeDB.Internal.IReducer.VolatileNonatomicScheduleImmediate(nameof({{Name}}), stream);
            }
            """
        );

        return extensions;
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

        var tableViews = CollectDistinct(
            "Table",
            context,
            tables
                .SelectMany((t, ct) => t.GenerateViews())
                .WithTrackingName("SpacetimeDB.Table.GenerateViews"),
            v => v.viewName,
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
            tableViews.Combine(addReducers).Combine(rlsFiltersArray).Combine(columnDefaultValues),
            (context, tuple) =>
            {
                var (((tableViews, addReducers), rlsFilters), columnDefaultValues) = tuple;
                // Don't generate the FFI boilerplate if there are no tables or reducers.
                if (tableViews.Array.IsEmpty && addReducers.Array.IsEmpty)
                {
                    return;
                }
                context.AddSource(
                    "FFI.cs",
                    $$"""
                    // <auto-generated />
                    #nullable enable

                    using System.Diagnostics.CodeAnalysis;
                    using System.Runtime.CompilerServices;
                    using System.Runtime.InteropServices;

                    namespace SpacetimeDB {
                        public sealed record ReducerContext : DbContext<Local>, Internal.IReducerContext {
                            public readonly Identity Sender;
                            public readonly ConnectionId? ConnectionId;
                            public readonly Random Rng;
                            public readonly Timestamp Timestamp;
                            public readonly AuthCtx SenderAuth;

                            // We need this property to be non-static for parity with client SDK.
                            public Identity Identity => Internal.IReducerContext.GetIdentity();

                    internal ReducerContext(Identity identity, ConnectionId? connectionId, Random random, Timestamp time) {
                                Sender = identity;
                                ConnectionId = connectionId;
                                Rng = random;
                                Timestamp = time;
                                SenderAuth = AuthCtx.BuildFromSystemTables(connectionId, identity);
                            }
                        }

                        namespace Internal.TableHandles {
                            {{string.Join("\n", tableViews.Select(v => v.view))}}
                        }

                        public sealed class Local {
                            {{string.Join("\n", tableViews.Select(v => v.getter))}}
                        }
                    }

                    static class ModuleRegistration {
                        {{string.Join("\n", addReducers.Select(r => r.Class))}}

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
                                tableViews.Select(t => $"SpacetimeDB.Internal.Module.RegisterTable<{t.tableName}, SpacetimeDB.Internal.TableHandles.{t.viewName}>();")
                            )}}
                            {{string.Join(
                                "\n",
                                rlsFilters.Select(f => $"SpacetimeDB.Internal.Module.RegisterClientVisibilityFilter({f.GlobalName});")
                            )}}
                            {{string.Join(
                                "\n",
                                columnDefaultValues.Select(d => 
                                    "{\n"
                                         +$"var value = new {d.BSATNTypeName}();\n"
                                         +"__memoryStream.Position = 0;\n"
                                         +"__memoryStream.SetLength(0);\n"
                                         +$"value.Write(__writer, {d.value});\n"   
                                         +"var array = __memoryStream.ToArray();\n" 
                                         +$"SpacetimeDB.Internal.Module.RegisterTableDefaultValue(\"{d.tableName}\", {d.columnId}, array);"
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
                    #endif
                    }
                    """
                );
            }
        );
    }
}
