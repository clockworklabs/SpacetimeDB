namespace SpacetimeDB.Codegen;

using System;
using System.Collections.Generic;
using System.Collections.Immutable;
using System.Linq;
using System.Text.RegularExpressions;
using System.Threading;
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

record SettingsDeclaration
{
    public readonly string FullName;
    public readonly string? CaseConversionPolicy;

    private static readonly string[] CaseConversionPolicyTypeNames =
    [
        "SpacetimeDB.CaseConversionPolicy",
        "SpacetimeDB.Internal.CaseConversionPolicy", // backward compat
    ];

    public SettingsDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
    {
        var fieldSymbol = (IFieldSymbol)context.TargetSymbol;
        FullName = SymbolToName(fieldSymbol);

        if (!fieldSymbol.IsConst)
        {
            diag.Report(ErrorDescriptor.SettingsMustBeConstCaseConversionPolicy, fieldSymbol);
            return;
        }
        if (!CaseConversionPolicyTypeNames.Contains(fieldSymbol.Type.ToString()))
        {
            diag.Report(ErrorDescriptor.SettingsMustBeConstCaseConversionPolicy, fieldSymbol);
            return;
        }
        if (fieldSymbol.ConstantValue is null)
        {
            diag.Report(ErrorDescriptor.SettingsMustBeConstCaseConversionPolicy, fieldSymbol);
            return;
        }

        try
        {
            var n = Convert.ToInt32(fieldSymbol.ConstantValue);
            CaseConversionPolicy = n switch
            {
                0 => "None",
                1 => "SnakeCase",
                2 => "CamelCase",
                3 => "PascalCase",
                _ => null,
            };
        }
        catch
        {
            CaseConversionPolicy = null;
        }

        if (CaseConversionPolicy is null)
        {
            diag.Report(ErrorDescriptor.SettingsMustBeConstCaseConversionPolicy, fieldSymbol);
        }
    }
}

/// <summary>
/// Represents a reference to a column in a table, combining its index and name.
/// Used to maintain references to columns for indexing and querying purposes.
/// </summary>
/// <param name="Index">The zero-based index of the column in the table</param>
/// <param name="Name">The name of the column as defined in the source code</param>
record ColumnRef(int Index, string Name);

static class ColumnTypeValidation
{
    public static bool IsInteger(ITypeSymbol type) =>
        type.SpecialType switch
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

    public static bool IsNoPayloadEnum(ITypeSymbol type)
    {
        if (type.TypeKind == Microsoft.CodeAnalysis.TypeKind.Enum)
        {
            return true;
        }

        if (type.BaseType?.OriginalDefinition.ToString() != "SpacetimeDB.TaggedEnum<Variants>")
        {
            return false;
        }

        return type.BaseType.TypeArguments.FirstOrDefault()
                is INamedTypeSymbol { IsTupleType: true, TupleElements: var variants }
            && variants.All(field => field.Type.ToString() == "SpacetimeDB.Unit");
    }

    public static bool IsEquatable(ITypeSymbol type) =>
        (
            IsInteger(type)
            || IsNoPayloadEnum(type)
            || type.SpecialType switch
            {
                SpecialType.System_String or SpecialType.System_Boolean => true,
                SpecialType.None => type.ToString()
                    is "SpacetimeDB.ConnectionId"
                        or "SpacetimeDB.Identity"
                        or "SpacetimeDB.Timestamp"
                        or "SpacetimeDB.Uuid",
                _ => false,
            }
        )
        && type.NullableAnnotation != NullableAnnotation.Annotated;
}

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

        var attrs = CombineColumnAttrs(Attrs);

        if (attrs.HasFlag(ColumnAttrs.AutoInc) && !ColumnTypeValidation.IsInteger(type))
        {
            diag.Report(ErrorDescriptor.AutoIncNotInteger, field);
        }

        IsEquatable = ColumnTypeValidation.IsEquatable(type);

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
        $"new (nameof({Identifier}), BSATN.{Identifier}{TypeUse.BsatnFieldSuffix}.GetAlgebraicType(registrar))";
}

record Scheduled(string ReducerName, int ScheduledAtColumn);

record TableAccessor
{
    public readonly string Name;
    public readonly string? CanonicalName;
    public readonly bool IsPublic;
    public readonly bool IsEvent;
    public readonly Scheduled? Scheduled;

    public string Identifier => EscapeIdentifier(Name);

    public TableAccessor(TableDeclaration table, AttributeData data, DiagReporter diag)
    {
        var attr = data.ParseAs<TableAttribute>();

        Name = attr.Accessor ?? table.ShortName;
        CanonicalName = attr.Name;
        IsPublic = attr.Public;
        IsEvent = attr.Event;
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
    public readonly string? CanonicalName;
    public readonly TableIndexType Type;

    public string AccessorIdentifier => EscapeIdentifier(AccessorName);

    // See: bindings_sys::index_id_from_name for documentation of this format.
    // Guaranteed not to contain quotes, so does not need to be escaped when embedded in a string.
    private readonly string StandardNameSuffix;

    /// <summary>
    /// Primary constructor that initializes all fields.
    /// Other constructors delegate to this one to avoid code duplication.
    /// </summary>
    /// <param name="accessorName">Name to use when accessing this index. If null, will be generated from column names.</param>
    /// <param name="canonicalName">Explicit canonical name override for this index, if any.</param>
    /// <param name="columns">The columns that make up this index.</param>
    /// <param name="tableName">The name of the table this index belongs to, if any.</param>
    /// <param name="type">The type of index (currently only B-tree is supported).</param>
    private TableIndex(
        string? accessorName,
        string? canonicalName,
        ImmutableArray<ColumnRef> columns,
        string? tableName,
        TableIndexType type
    )
    {
        Columns = new(columns);
        Table = tableName;
        var columnNames = string.Join("_", columns.Select(c => c.Name));
        AccessorName = accessorName ?? columnNames;
        CanonicalName = canonicalName;
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
            null,
            ImmutableArray.Create(col),
            null,
            TableIndexType.BTree // this might become hash in the future
        ) { }

    /// <summary>
    /// Creates an index with the given attribute and columns.
    /// Used internally by other constructors that parse attributes.
    /// </summary>
    private TableIndex(
        global::SpacetimeDB.Index.BTreeAttribute attr,
        ImmutableArray<ColumnRef> columns
    )
        : this(attr.Accessor, attr.Name, columns, attr.Table, TableIndexType.BTree) { }

    /// <summary>
    /// Creates an index from a table declaration and attribute data.
    /// Validates the index configuration and reports any errors through the diag reporter.
    /// </summary>
    private TableIndex(
        TableDeclaration table,
        global::SpacetimeDB.Index.BTreeAttribute attr,
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
        if (string.IsNullOrWhiteSpace(attr.Accessor))
        {
            diag.Report(ErrorDescriptor.TableLevelIndexMissingAccessor, data);
        }

        if (attr.Columns.Length == 0)
        {
            diag.Report(ErrorDescriptor.EmptyIndexColumns, data);
        }
    }

    /// <summary>
    /// Creates an index by parsing attribute data from a table declaration.
    /// </summary>
    public TableIndex(TableDeclaration table, AttributeData data, DiagReporter diag)
        : this(table, data.ParseAs<global::SpacetimeDB.Index.BTreeAttribute>(), data, diag) { }

    /// <summary>
    /// Creates an index for a single column with attribute data.
    /// Validates that no additional columns were specified in the attribute.
    /// </summary>
    private TableIndex(
        ColumnRef column,
        global::SpacetimeDB.Index.BTreeAttribute attr,
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
        : this(col, data.ParseAs<global::SpacetimeDB.Index.BTreeAttribute>(), data, diag) { }

    // `FullName` and Roslyn have different ways of representing nested types in full names -
    // one uses a `Parent+Child` syntax, the other uses `Parent.Child`.
    // Manually fixup one to the other.
    private static readonly string BTreeAttrName =
        typeof(global::SpacetimeDB.Index.BTreeAttribute).FullName.Replace('+', '.');

    public static bool CanParse(AttributeData data) =>
        data.AttributeClass?.ToString() == BTreeAttrName;

    public string GenerateIndexDef(TableAccessor tableAccessor) =>
        $$"""
            new(
                SourceName: "{{StandardIndexName(tableAccessor)}}",
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
    private readonly string assemblyIdentity;
    private readonly bool sharedContexts;
    private readonly string handlesNamespace;

    private string TableHandlesNamespace => handlesNamespace + ".TableHandles";
    private string ViewHandlesNamespace => handlesNamespace + ".ViewHandles";

    private string LookupName(string localName) =>
        sharedContexts
            ? $"global::SpacetimeDB.Internal.Module.ResolveName({SymbolDisplay.FormatLiteral(assemblyIdentity, true)}, {SymbolDisplay.FormatLiteral(localName, true)})"
            : SymbolDisplay.FormatLiteral(localName, true);

    private string HandleLookupName(string localName) =>
        sharedContexts
            ? $"global::SpacetimeDB.Internal.Module.ResolveName(instanceId, {SymbolDisplay.FormatLiteral(localName, true)})"
            : LookupName(localName);

    private string HandleCache(string typeName) =>
        $$"""
            private static readonly {{typeName}}?[] __instances = new {{typeName}}?[global::SpacetimeDB.Internal.Module.InstanceCount];
            private readonly int __instanceId;
            static {{typeName}}() { }

            [global::System.Runtime.CompilerServices.MethodImpl(global::System.Runtime.CompilerServices.MethodImplOptions.AggressiveInlining)]
            public static {{typeName}} Get(int contextInstance) =>
                __instances[contextInstance] ??= Create(contextInstance);

            private static {{typeName}} Create(int contextInstance)
            {
                var instanceId = global::SpacetimeDB.Internal.Module.ResolveInstance(contextInstance, {{SymbolDisplay.FormatLiteral(
                assemblyIdentity,
                true
            )}});
                return __instances[instanceId] ??= new(instanceId);
            }
            """;

    private string IndexInstanceCache(string typeName, string identifier) =>
        sharedContexts
            ? $$"""
                private {{typeName}}? __{{identifier.TrimStart('@')}};
                """
            : "";

    private string IndexInstance(
        string identifier,
        bool writable = false,
        bool primaryKey = false
    ) =>
        sharedContexts
            ? writable
                ? $"__state.__{identifier.TrimStart('@')} ??= new(__state.InstanceId{(primaryKey ? ", __state" : "")})"
                : $"__{identifier.TrimStart('@')} ??= new(__instanceId)"
            : "new()";

    private IEnumerable<string> WritableIndexCaches(TableAccessor table)
    {
        foreach (
            var constraint in GetConstraints(table, ColumnAttrs.Unique)
                .Where(c => c.Col.IsEquatable)
        )
        {
            var identifier = constraint.Col.Identifier;
            yield return $"internal {identifier}UniqueIndex? __{identifier.TrimStart('@')};";
        }
        foreach (var index in GetIndexes(table).Where(i => i.AccessorName.Length != 0))
        {
            var identifier = index.AccessorIdentifier;
            yield return $"internal {identifier}Index? __{identifier.TrimStart('@')};";
        }
    }

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

        var compilation = context.SemanticModel.Compilation;
        assemblyIdentity = compilation.Assembly.Identity.ToString();
        sharedContexts = Module.UsesSharedContexts(compilation);
        handlesNamespace = sharedContexts
            ? Module.AssemblyNamespace(compilation.Assembly)
            : "SpacetimeDB.Internal";

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
        if (sharedContexts)
        {
            ValidateGeneratedNames(diag, typeSyntax.GetLocation());
        }
    }

    private void ValidateGeneratedNames(DiagReporter diag, Location location)
    {
        var names = new GeneratedNames(
            (scope, name, first, second) =>
                diag.Report(
                    ErrorDescriptor.GeneratedNameCollision,
                    (location, scope, name, first, second)
                )
        );
        foreach (var table in TableAccessors)
        {
            var owner = $"table '{table.Name}' on '{FullName}'";
            var writable = $"{TableHandlesNamespace}.{table.Name}";
            var readOnly = $"{ViewHandlesNamespace}.{table.Name}ReadOnly";
            names.Add(writable, table.Identifier, "enclosing table handle");
            names.Add(readOnly, table.Identifier + "ReadOnly", "enclosing read-only handle");
            foreach (
                var member in new[]
                {
                    "LookupName",
                    "ReadGenFields",
                    "MakeTableDesc",
                    "MakeScheduleDesc",
                    "Count",
                    "Iter",
                    "Insert",
                    "Delete",
                    "Clear",
                    "Get",
                    "Create",
                    "__instances",
                    "__instanceId",
                    "__state",
                    "InstanceState",
                }
            )
                names.Add(writable, member, "generated table member");
            foreach (
                var member in new[]
                {
                    "Get",
                    "Create",
                    "__instances",
                    "__instanceId",
                    "Count",
                    "Iter",
                }
            )
                names.Add(readOnly, member, "generated read-only table member");

            void Index(string identifier, bool unique, string contributor)
            {
                foreach (var scope in new[] { writable, readOnly })
                {
                    names.Add(scope, identifier, contributor);
                    names.Add(
                        scope,
                        "__" + identifier.TrimStart('@'),
                        $"cache field for {contributor}"
                    );
                    names.Add(
                        scope,
                        identifier + (unique && scope == writable ? "UniqueIndex" : "Index"),
                        $"index type for {contributor}"
                    );
                }
            }
            foreach (
                var constraint in GetConstraints(table, ColumnAttrs.Unique)
                    .Where(c => c.Col.IsEquatable)
            )
                Index(
                    constraint.Col.Identifier,
                    true,
                    $"unique column '{constraint.Col.Name}' of {owner}"
                );
            foreach (var index in GetIndexes(table).Where(i => i.AccessorName.Length != 0))
                Index(index.AccessorIdentifier, false, $"index '{index.AccessorName}' of {owner}");

            foreach (var container in new[] { "Tables", "ReadOnlyTables", "Queries" })
                if (table.Name == container)
                {
                    names.Add(container, container, "enclosing descriptor container");
                }

            foreach (var container in new[] { "Tables", "ReadOnlyTables", "Queries" })
                names.Add(container, table.Identifier, owner);
            foreach (var container in new[] { "Tables", "ReadOnlyTables" })
            {
                if (table.Name == "__instanceId")
                {
                    names.Add(container, "__instanceId", "generated instance field");
                }
            }
            if (table.Name is "GetType" or "ToString" or "Equals" or "GetHashCode")
            {
                diag.Report(
                    ErrorDescriptor.GeneratedNameCollision,
                    (
                        location,
                        "context database/query receiver",
                        table.Name,
                        "existing receiver member",
                        owner
                    )
                );
            }

            var cols = table.Identifier + "Cols";
            names.Add(cols, cols, "enclosing query columns type");
            foreach (var column in Members)
                names.Add(cols, column.Identifier, $"column '{column.Name}' of {owner}");
            var ixCols = table.Identifier + "IxCols";
            names.Add(ixCols, ixCols, "enclosing indexed query columns type");
            var indexedPositions = new HashSet<int>(
                GetConstraints(table, ColumnAttrs.PrimaryKey | ColumnAttrs.Unique)
                    .Select(c => c.Pos)
            );
            foreach (var index in GetIndexes(table))
            foreach (var column in index.Columns.Array)
                indexedPositions.Add(column.Index);
            foreach (var position in indexedPositions)
            {
                var column = Members[position];
                names.Add(ixCols, column.Identifier, $"indexed column '{column.Name}' of {owner}");
            }
        }
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

        var uniqueIndexBase =
            "global::SpacetimeDB.Internal." + (isRowStruct ? "UniqueIndex" : "RefUniqueIndex");

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
            var primaryKey = ct.Attr.HasFlag(ColumnAttrs.PrimaryKey);
            var instanceTable = sharedContexts && primaryKey;
            var updateMethod = primaryKey
                ? $"public {globalName} Update({globalName} row) => DoUpdate(row{(instanceTable ? ", __table" : "")});"
                : "";
            yield return $$"""
                {{vis}} sealed class {{f.Identifier}}UniqueIndex : {{uniqueIndexBase}}<{{tableAccessor.Identifier}}, {{globalName}}, {{f.Type.Name}}, {{f.Type.BSATNName}}> {
                    {{(instanceTable ? "private readonly global::SpacetimeDB.Internal.TableHandle __table;" : "")}}
                    internal {{f.Identifier}}UniqueIndex({{(sharedContexts ? "int instanceId" + (instanceTable ? ", global::SpacetimeDB.Internal.TableHandle table" : "") : "")}}) : base({{HandleLookupName(
                    standardIndexName
                )}}) { {{(instanceTable ? "__table = table;" : "")}} }
                    // Important: don't move this to the base class.
                    // C# generics don't play well with nullable types and can't accept both struct-type-based and class-type-based
                    // `globalName` in one generic definition, leading to buggy `Row?` expansion for either one or another.
                    public {{globalName}}? Find({{f.Type.Name}} key) => FindSingle(key);
                    {{updateMethod}}
                }
                {{vis}} {{f.Identifier}}UniqueIndex {{f.Identifier}} => {{IndexInstance(
                    f.Identifier, writable: true, primaryKey: primaryKey
                )}};
                """;
        }

        foreach (var index in GetIndexes(tableAccessor))
        {
            var name = index.AccessorName;
            var identifierName = index.AccessorIdentifier;

            // Skip bad declarations. Empty name means no columns, which we have already reported with a meaningful error.
            // Emitting this will result in further compilation errors due to missing property name.
            if (name == "")
            {
                continue;
            }

            var members = index.Columns.Select(c => Members[c.Index]).ToArray();
            var standardIndexName = index.StandardIndexName(tableAccessor);

            yield return $$"""
                    {{vis}} sealed class {{identifierName}}Index({{(sharedContexts ? "int instanceId" : "")}}) : SpacetimeDB.Internal.IndexBase<{{globalName}}>({{HandleLookupName(
                    standardIndexName
                )}}) {
                """;

            for (var n = 0; n < members.Length; n++)
            {
                var types = string.Join(
                    ", ",
                    members.Take(n + 1).Select(m => $"{m.Type.Name}, {m.Type.BSATNName}")
                );
                var scalars = members.Take(n).Select(m => $"{m.Type.Name} {m.Identifier}");
                var lastScalar = $"{members[n].Type.Name} {members[n].Identifier}";
                var lastBounds =
                    $"global::SpacetimeDB.Bound<{members[n].Type.Name}> {members[n].Identifier}";
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
                    argName = members[0].Identifier;
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

            yield return $"}}\n {vis} {identifierName}Index {identifierName} => {IndexInstance(identifierName, writable: true)};\n";
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
                public sealed class {{{f.Identifier}}}Index
                    : {{{uniqueIndexBase}}}<
                          global::{{{ViewHandlesNamespace}}}.{{{tableAccessor.Identifier}}}ReadOnly,
                          {{{globalName}}},
                          {{{f.Type.Name}}},
                          {{{f.Type.BSATNName}}}>
                {
                    internal {{{f.Identifier}}}Index({{{(sharedContexts ? "int instanceId" : "")}}}) : base({{{HandleLookupName(
                    standardIndexName
                )}}}) { }

                    public {{{globalName}}}? Find({{{f.Type.Name}}} key) => FindSingle(key);
                }

                {{{IndexInstanceCache(f.Identifier + "Index", f.Identifier)}}}
                public {{{f.Identifier}}}Index {{{f.Identifier}}} => {{{IndexInstance(
                    f.Identifier
                )}}};
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
            var identifierName = index.AccessorIdentifier;

            var blocks = new List<string>
            {
                $$$"""
                    public sealed class {{{identifierName}}}Index
                    : global::SpacetimeDB.Internal.ReadOnlyIndexBase<{{{globalName}}}>
                    {
                    internal {{{identifierName}}}Index({{{(sharedContexts ? "int instanceId" : "")}}}) : base({{{HandleLookupName(
                        standardIndexName
                    )}}}) {}
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
                    declaringMembers.Select(m => $"{m.Type.Name} {m.Identifier}")
                );
                var boundsArgs = string.Join(
                    ", ",
                    declaringMembers
                        .Take(n)
                        .Select(m => $"{m.Type.Name} {m.Identifier}")
                        .Append(
                            $"global::SpacetimeDB.Bound<{declaringMembers[^1].Type.Name}> {declaringMembers[^1].Identifier}"
                        )
                );

                var ctorArg = n == 0 ? declaringMembers[0].Identifier : "f";

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

            blocks.Add(
                $"}}\n{IndexInstanceCache(identifierName + "Index", identifierName)}\n{vis} {identifierName}Index {identifierName} => {IndexInstance(identifierName)};"
            );
            yield return string.Join("\n", blocks);
        }
    }

    /// <summary>
    /// Represents a generated accessor for a table, providing different access patterns
    /// and visibility levels for the underlying table data.
    /// </summary>
    /// <param name="TableAccessorName">Name of the generated accessor type</param>
    /// <param name="TableName">Fully qualified name of the table type</param>
    /// <param name="TableAccessor">C# source code for the accessor implementation</param>
    /// <param name="Getter">C# property getter for accessing the accessor</param>
    public record struct GeneratedTableAccessor(
        string TableAccessorName,
        string TableName,
        string TableAccessor,
        string Getter
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
            var accessorIdentifier = v.Identifier;
            var iTable =
                $"global::SpacetimeDB.Internal.ITableView<{accessorIdentifier}, {globalName}>";
            var tableArgument = sharedContexts ? "__state" : "";
            var rowArgument = sharedContexts ? "row, __state" : "row";
            var instanceMembers = sharedContexts
                ? $$"""
                    private sealed class InstanceState(int instanceId) : global::SpacetimeDB.Internal.TableHandle({{HandleLookupName(
                        v.Name
                    )}})
                    {
                        internal readonly int InstanceId = instanceId;
                        {{string.Join("\n", WritableIndexCaches(v))}}
                    }

                    private static readonly InstanceState?[] __instances = new InstanceState?[global::SpacetimeDB.Internal.Module.InstanceCount];
                    private readonly InstanceState __state;
                    static {{accessorIdentifier}}() { }
                    public {{accessorIdentifier}}() : this(Get(0).__state) { }
                    private {{accessorIdentifier}}(InstanceState state) { __state = state; }

                    [global::System.Runtime.CompilerServices.MethodImpl(global::System.Runtime.CompilerServices.MethodImplOptions.AggressiveInlining)]
                    public static {{accessorIdentifier}} Get(int contextInstance) => new(__instances[contextInstance] ??= Create(contextInstance));

                    private static InstanceState Create(int contextInstance)
                    {
                        var instanceId = global::SpacetimeDB.Internal.Module.ResolveInstance(contextInstance, {{SymbolDisplay.FormatLiteral(
                        assemblyIdentity,
                        true
                    )}});
                        return __instances[instanceId] ??= new(instanceId);
                    }
                    """
                : "";
            yield return new(
                v.Name,
                globalName,
                $$$"""
            {{{SyntaxFacts.GetText(Visibility)}}} readonly struct {{{accessorIdentifier}}} : {{{iTable}}} {
                {{{instanceMembers}}}
                public static {{{globalName}}} ReadGenFields(System.IO.BinaryReader reader, {{{globalName}}} row) {
                    {{{string.Join(
                        "\n",
                        autoIncFields.Select(m =>
                            $$"""
                            if (row.{{m.Identifier}} == default)
                            {
                                row.{{m.Identifier}} = {{globalName}}.BSATN.{{m.Identifier}}{{TypeUse.BsatnFieldSuffix}}.Read(reader);
                            }
                            """
                        )
                    )}}}
                    return row;
                }

                public static SpacetimeDB.Internal.RawTableDefV10 MakeTableDesc(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                    SourceName: nameof({{{accessorIdentifier}}}),
                    ProductTypeRef: (uint) new {{{globalName}}}.BSATN().GetAlgebraicType(registrar).Ref_,
                    PrimaryKey: [{{{GetPrimaryKey(v)?.ToString() ?? ""}}}],
                    Indexes: [
                        {{{string.Join(
                            ",\n",
                            GetConstraints(v, ColumnAttrs.Unique)
                            .Select(c => c.ToIndex())
                            .Concat(GetIndexes(v))
                            .Select(b => b.GenerateIndexDef(v))
                        )}}}
                    ],
                    Constraints: {{{GenConstraintList(v, ColumnAttrs.Unique, $"{iTable}.MakeUniqueConstraint")}}},
                    Sequences: {{{GenConstraintList(v, ColumnAttrs.AutoInc, $"{iTable}.MakeSequence")}}},
                    TableType: SpacetimeDB.Internal.TableType.User,
                    TableAccess: SpacetimeDB.Internal.TableAccess.{{{(v.IsPublic ? "Public" : "Private")}}},
                    DefaultValues: [],
                    IsEvent: {{{(v.IsEvent ? "true" : "false")}}}
                );

                public static SpacetimeDB.Internal.RawScheduleDefV10? MakeScheduleDesc() => {{{(
                        v.Scheduled is { } scheduled
                        ? $"{iTable}.MakeSchedule(\"{scheduled.ReducerName}\", {scheduled.ScheduledAtColumn})"
                        : "null"
                    )}}};

                /// <summary>
                /// Returns the number of rows in this table.
                ///
                /// This reads datastore metadata, so it runs in constant time.
                /// It also takes into account modifications by the current transaction.
                /// </summary>
                public ulong Count => {{{iTable}}}.DoCount({{{tableArgument}}});
                public IEnumerable<{{{globalName}}}> Iter() => {{{iTable}}}.DoIter({{{tableArgument}}});
                public {{{globalName}}} Insert({{{globalName}}} row) => {{{iTable}}}.DoInsert({{{rowArgument}}});
                public bool Delete({{{globalName}}} row) => {{{iTable}}}.DoDelete({{{rowArgument}}});
                public ulong Clear() => {{{iTable}}}.DoClear({{{tableArgument}}});

                {{{string.Join("\n", GenerateTableAccessorFilters(v))}}}
            }
            """,
                $"{SyntaxFacts.GetText(Visibility)} global::{TableHandlesNamespace}.{accessorIdentifier} {accessorIdentifier} => {(sharedContexts ? $"global::{TableHandlesNamespace}.{accessorIdentifier}.Get(global::SpacetimeDB.Internal.Module.GetInstanceId(db))" : "new()")};"
            );
        }
    }

    public record struct GeneratedReadOnlyAccessor(
        string TableAccessorName,
        string TableName,
        string ReadOnlyAccessor,
        string ReadOnlyGetter
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
            var accessorIdentifier = accessor.Identifier;

            var readOnlyIndexDecls = string.Join("\n", GenerateReadOnlyAccessorFilters(accessor));
            var visibility = SyntaxFacts.GetText(Visibility);
            yield return new(
                accessor.Name,
                globalName,
                $$$"""
                {{{visibility}}} sealed class {{{accessorIdentifier}}}ReadOnly
                    : global::SpacetimeDB.Internal.ReadOnlyTableView<{{{globalName}}}>
                {
                    {{{(sharedContexts ? HandleCache(accessorIdentifier + "ReadOnly") : "")}}}
                    internal {{{accessorIdentifier}}}ReadOnly({{{(sharedContexts ? "int instanceId" : "")}}}) : base({{{HandleLookupName(
                    accessor.Name
                )}}}) { {{{(sharedContexts ? "__instanceId = instanceId;" : "")}}} }

                    /// <summary>
                    /// Returns the number of rows in this table.
                    ///
                    /// This reads datastore metadata, so it runs in constant time.
                    /// It also takes into account modifications by the current transaction.
                    /// </summary>
                    public ulong Count => DoCount();

                    {{{readOnlyIndexDecls}}}
                }
                """,
                $"{visibility} global::{ViewHandlesNamespace}.{accessorIdentifier}ReadOnly {accessorIdentifier} => {(sharedContexts ? $"global::{ViewHandlesNamespace}.{accessorIdentifier}ReadOnly.Get(global::SpacetimeDB.Internal.Module.GetInstanceId(db))" : "new()")};"
            );
        }
    }

    // useExtensions means we're in a .NET 10 context
    public IEnumerable<string> GenerateQueryBuilderMembers(bool useExtensions = false)
    {
        if (Kind is TypeKind.Sum)
        {
            yield break;
        }

        var vis = SyntaxFacts.GetText(Visibility);
        var globalRowName = $"global::{FullName}";

        foreach (var accessor in TableAccessors)
        {
            var accessorIdentifier = accessor.Identifier;
            var tableName = accessor.Name;
            var colsTypeName = $"{accessorIdentifier}Cols";
            var ixColsTypeName = $"{accessorIdentifier}IxCols";

            string ColDecl(ColumnDeclaration col)
            {
                var typeName = col.Type.Name;
                var isNullable = typeName.EndsWith("?", StringComparison.Ordinal);
                var valueTypeName = isNullable ? typeName[..^1] : typeName;
                return $"public readonly global::SpacetimeDB.Col<{globalRowName}, {valueTypeName}> {col.Identifier};";
            }

            string ColInit(ColumnDeclaration col)
            {
                var typeName = col.Type.Name;
                var isNullable = typeName.EndsWith("?", StringComparison.Ordinal);
                var valueTypeName = isNullable ? typeName[..^1] : typeName;
                return $"{col.Identifier} = new global::SpacetimeDB.Col<{globalRowName}, {valueTypeName}>(tableName, \"{col.Name}\");";
            }

            var colsDecls = string.Join("\n    ", Members.Select(ColDecl));
            var colsInits = string.Join("\n        ", Members.Select(ColInit));

            var ixPositions = new HashSet<int>();
            foreach (var c in GetConstraints(accessor, ColumnAttrs.PrimaryKey | ColumnAttrs.Unique))
            {
                ixPositions.Add(c.Pos);
            }

            foreach (var ix in GetIndexes(accessor))
            {
                foreach (var colRef in ix.Columns.Array)
                {
                    ixPositions.Add(colRef.Index);
                }
            }

            var ixMembers = Members
                .Select((m, i) => (m, i))
                .Where(pair => ixPositions.Contains(pair.i))
                .Select(pair => pair.m)
                .ToArray();

            string IxColDecl(ColumnDeclaration col)
            {
                var typeName = col.Type.Name;
                var isNullable = typeName.EndsWith("?", StringComparison.Ordinal);
                var valueTypeName = isNullable ? typeName[..^1] : typeName;
                return $"public readonly global::SpacetimeDB.IxCol<{globalRowName}, {valueTypeName}> {col.Identifier};";
            }

            string IxColInit(ColumnDeclaration col)
            {
                var typeName = col.Type.Name;
                var isNullable = typeName.EndsWith("?", StringComparison.Ordinal);
                var valueTypeName = isNullable ? typeName[..^1] : typeName;
                return $"{col.Identifier} = new global::SpacetimeDB.IxCol<{globalRowName}, {valueTypeName}>(tableName, \"{col.Name}\");";
            }

            var ixColsDecls = string.Join("\n    ", ixMembers.Select(IxColDecl));
            var ixColsInits = string.Join("\n        ", ixMembers.Select(IxColInit));
            var nameType = useExtensions ? "global::SpacetimeDB.SqlTableName" : "string";
            var queryType =
                $"global::SpacetimeDB.Table<{globalRowName}, {colsTypeName}, {ixColsTypeName}>";
            var queryMember = useExtensions
                ? $$"""
                    public static partial class AssemblyDescriptor
                    {
                        private static class {{accessorIdentifier}}SqlNameCache
                        {
                            private static readonly global::SpacetimeDB.SqlTableName?[] Names = new global::SpacetimeDB.SqlTableName?[global::SpacetimeDB.Internal.Module.InstanceCount];

                            internal static global::SpacetimeDB.SqlTableName Get(int contextInstance) => Names[contextInstance] ??= Create(contextInstance);

                            private static global::SpacetimeDB.SqlTableName Create(int contextInstance)
                            {
                                var instanceId = global::SpacetimeDB.Internal.Module.ResolveInstance(contextInstance, {{SymbolDisplay.FormatLiteral(
                        assemblyIdentity,
                        true
                    )}});
                                return Names[instanceId] ??= global::SpacetimeDB.Internal.Module.ResolveSqlName(instanceId, {{SymbolDisplay.FormatLiteral(tableName, true)}});
                            }
                            // Prevent eager initialization before the root installs namespace placements.
                            static {{accessorIdentifier}}SqlNameCache() { }
                        }

                        public readonly partial struct Queries
                        {
                            {{vis}} {{queryType}} {{accessorIdentifier}}()
                            {
                                var tableName = {{accessorIdentifier}}SqlNameCache.Get(__instanceId);
                                return new(tableName, new {{colsTypeName}}(tableName), new {{ixColsTypeName}}(tableName));
                            }
                        }
                    }

                    public static partial class QueryTableExtensions
                    {
                        extension(global::SpacetimeDB.QueryBuilder from)
                        {
                            {{vis}} {{queryType}} {{accessorIdentifier}}() => new AssemblyDescriptor.Queries(global::SpacetimeDB.Internal.Module.GetInstanceId(from)).{{accessorIdentifier}}();
                        }
                    }
                    """
                : $$"""
                    public readonly partial struct QueryBuilder
                    {
                        {{vis}} {{queryType}} {{accessorIdentifier}}() =>
                            new("{{tableName}}", new {{colsTypeName}}("{{tableName}}"), new {{ixColsTypeName}}("{{tableName}}"));
                    }
                    """;

            yield return $$"""
                {{vis}} readonly struct {{colsTypeName}}
                {
                    {{colsDecls}}

                    internal {{colsTypeName}}({{nameType}} tableName)
                    {
                        {{colsInits}}
                    }
                }

                {{vis}} readonly struct {{ixColsTypeName}}
                {
                    {{ixColsDecls}}

                    internal {{ixColsTypeName}}({{nameType}} tableName)
                    {
                        {{ixColsInits}}
                    }
                }

                {{queryMember}}
                """;
        }
    }

    /// <summary>
    /// Represents a default value for a table field, used during table creation.
    /// </summary>
    /// <param name="TableName">Name of the table containing the field</param>
    /// <param name="ColumnId">Index of the column in the table</param>
    /// <param name="Value">String representation of the default value</param>
    /// <param name="BSATNTypeName">BSATN Type name of the default value</param>
    public record struct FieldDefaultValue(
        string TableName,
        string ColumnId,
        string Value,
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
                fieldsWithDefaultValues as ColumnDeclaration[] ?? [.. fieldsWithDefaultValues];
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
    public readonly string? CanonicalName;
    public readonly string? PrimaryKey;
    public readonly string FullName;
    public readonly bool IsAnonymous;
    public readonly bool IsPublic;
    public readonly bool ReturnsQuery;
    public readonly bool ReturnsEnumerable;
    public readonly TypeUse ReturnType;
    public readonly TypeUse? QueryRowType;
    public readonly EquatableArray<MemberDeclaration> Parameters;
    public readonly Scope Scope;

    private static ITypeSymbol? NullableElementType(ITypeSymbol type) =>
        type switch
        {
            INamedTypeSymbol
            {
                OriginalDefinition.SpecialType: SpecialType.System_Nullable_T
            } nullable => nullable.TypeArguments[0],
            _ when IsNullableReferenceType(type) => type.WithNullableAnnotation(
                NullableAnnotation.None
            ),
            _ => null,
        };

    private static IFieldSymbol? FindPrimaryKeyField(ITypeSymbol rowType, string primaryKey) =>
        SpacetimeDbFieldDiscovery.FindSpacetimeDbField(rowType, primaryKey);

    private static SyntaxNode FindAttributeNamedArgumentExpression(
        AttributeData attrData,
        string argumentName,
        SyntaxNode fallback
    )
    {
        if (
            attrData.ApplicationSyntaxReference?.GetSyntax() is AttributeSyntax
            {
                ArgumentList: { } argumentList
            }
        )
        {
            foreach (var argument in argumentList.Arguments)
            {
                if (argument.NameEquals?.Name.Identifier.ValueText == argumentName)
                {
                    return argument.Expression;
                }
            }
        }

        return fallback;
    }

    private static string EscapeStringLiteral(string s) =>
        s.Replace("\\", "\\\\")
            .Replace("\"", "\\\"")
            .Replace("\r", "\\r")
            .Replace("\n", "\\n")
            .Replace("\t", "\\t");

    public ViewDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
    {
        var methodSyntax = (MethodDeclarationSyntax)context.TargetNode;
        var method = (IMethodSymbol)context.TargetSymbol;
        var attrData = context.Attributes.Single();
        var attr = attrData.ParseAs<ViewAttribute>();
        var hasContextParam = method.Parameters.Length > 0;
        var firstParamType = hasContextParam ? method.Parameters[0].Type : null;
        var isAnonymousContext = firstParamType?.Name == "AnonymousViewContext";
        var hasArguments = method.Parameters.Length > 1;

        if (string.IsNullOrEmpty(attr.Accessor))
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

        Name = attr.Accessor ?? method.Name;
        CanonicalName = attr.Name;
        PrimaryKey = string.IsNullOrEmpty(attr.PrimaryKey) ? null : attr.PrimaryKey;
        FullName = SymbolToName(method);
        IsPublic = attr.Public;
        IsAnonymous = isAnonymousContext;

        ReturnsQuery = false;
        ReturnsEnumerable = false;
        ITypeSymbol? returnRowType = null;
        INamedTypeSymbol? iquery = null;
        if (
            method.ReturnType is INamedTypeSymbol
            {
                Name: "IQuery",
                ContainingNamespace: { Name: "SpacetimeDB" },
                TypeArguments: [var _]
            } directIQuery
        )
        {
            iquery = directIQuery;
        }
        else
        {
            iquery = method
                .ReturnType.AllInterfaces.OfType<INamedTypeSymbol>()
                .FirstOrDefault(i =>
                    i
                        is {
                            Name: "IQuery",
                            ContainingNamespace: { Name: "SpacetimeDB" },
                            TypeArguments.Length: 1
                        }
                );
        }

        if (iquery is { TypeArguments: [var queryRowType] })
        {
            ReturnsQuery = true;
            var rowType = TypeUse.Parse(method, queryRowType, diag);
            QueryRowType = rowType;
            ReturnType = rowType;
            returnRowType = queryRowType;
        }
        else if (
            method.ReturnType
                is INamedTypeSymbol
                {
                    OriginalDefinition: var originalDefinition,
                    TypeArguments: [var enumerableElementType]
                }
            && originalDefinition.ToString() == "System.Collections.Generic.IEnumerable<T>"
        )
        {
            ReturnsEnumerable = true;
            var elementType = TypeUse.Parse(method, enumerableElementType, diag);
            var elementTypeName = SymbolToName(enumerableElementType);
            var listTypeName = $"System.Collections.Generic.List<{elementTypeName}>";
            var listTypeInfo =
                $"SpacetimeDB.BSATN.List<{elementTypeName}, {elementType.BSATNName}>";
            ReturnType = new ListUse(listTypeName, listTypeInfo, elementType);
            returnRowType = enumerableElementType;
        }
        else
        {
            QueryRowType = null;
            ReturnType = TypeUse.Parse(method, method.ReturnType, diag);

            if (
                method.ReturnType
                    is INamedTypeSymbol
                    {
                        OriginalDefinition: var listDefinition,
                        TypeArguments: [var listElementType],
                    }
                && listDefinition.ToString() == "System.Collections.Generic.List<T>"
            )
            {
                ReturnsEnumerable = true;
                returnRowType = listElementType;
            }
            else if (NullableElementType(method.ReturnType) is { } optionElementType)
            {
                returnRowType = optionElementType;
            }
        }
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

        // Validate return type: must be List<T>, T?, or IEnumerable<T>
        var isOption =
            ReturnType.BSATNName.Contains("SpacetimeDB.BSATN.ValueOption")
            || ReturnType.BSATNName.Contains("SpacetimeDB.BSATN.RefOption");

        if (!ReturnsQuery && !ReturnsEnumerable && !isOption)
        {
            diag.Report(ErrorDescriptor.ViewInvalidReturn, methodSyntax);
        }

        if (PrimaryKey is { } primaryKey && returnRowType is { } rowTypeForPrimaryKey)
        {
            var primaryKeySyntax = FindAttributeNamedArgumentExpression(
                attrData,
                nameof(ViewAttribute.PrimaryKey),
                methodSyntax
            );

            if (FindPrimaryKeyField(rowTypeForPrimaryKey, primaryKey) is not { } field)
            {
                diag.Report(
                    ErrorDescriptor.ViewPrimaryKeyColumnNotFound,
                    (methodSyntax, primaryKeySyntax, primaryKey, SymbolToName(rowTypeForPrimaryKey))
                );
            }
            else if (!ColumnTypeValidation.IsEquatable(field.Type))
            {
                diag.Report(
                    ErrorDescriptor.ViewPrimaryKeyNotFilterable,
                    (methodSyntax, primaryKeySyntax, primaryKey, SymbolToName(field.Type))
                );
            }
        }

        Parameters = new(
            method
                .Parameters.Skip(1)
                .Select(p => new MemberDeclaration(p, p.Type, diag))
                .ToImmutableArray()
        );
    }

    public string GenerateViewDef(uint Index)
    {
        var returnTypeExpr = ReturnsQuery
            ? $"global::SpacetimeDB.BSATN.AlgebraicType.MakeQueryBuilderProductType(new {QueryRowType!.BSATNName}().GetAlgebraicType(registrar))"
            : $"new {ReturnType.BSATNName}().GetAlgebraicType(registrar)";
        return $$$"""
            new global::SpacetimeDB.Internal.RawViewDefV10(
                SourceName: "{{{Name}}}",
                Index: {{{Index}}},
                IsPublic: {{{IsPublic.ToString().ToLower()}}},
                IsAnonymous: {{{IsAnonymous.ToString().ToLower()}}},
                Params: [{{{MemberDeclaration.GenerateDefs(Parameters)}}}],
                ReturnType: {{{returnTypeExpr}}}
            );
            """;
    }

    public string? GenerateViewPrimaryKeyRegistration()
    {
        if (PrimaryKey is null)
        {
            return null;
        }

        return $"builder.RegisterViewPrimaryKey(\"{EscapeStringLiteral(Name)}\", [\"{EscapeStringLiteral(PrimaryKey)}\"]);";
    }

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
                $"var {p.Identifier} = {p.Identifier}{TypeUse.BsatnFieldSuffix}.Read(reader);"
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

        var writeOutput =
            ReturnsQuery
                ? $$$"""
                        var header = new global::SpacetimeDB.Internal.ViewResultHeader.RawSql(returnValue.ToSql());
                        var headerRW = new global::SpacetimeDB.Internal.ViewResultHeader.BSATN();
                        using var output = new System.IO.MemoryStream();
                        using var writer = new System.IO.BinaryWriter(output);
                        headerRW.Write(writer, header);
                        return output.ToArray();
                    """
            : isOption
                ? $$$"""
                        var listSerializer = {{{ReturnType.BSATNName}}}.GetListSerializer();
                        var listValue = ModuleRegistration.ToListOrEmpty(returnValue);
                        var header = new global::SpacetimeDB.Internal.ViewResultHeader.RowData(default);
                        var headerRW = new global::SpacetimeDB.Internal.ViewResultHeader.BSATN();
                        using var output = new System.IO.MemoryStream();
                        using var writer = new System.IO.BinaryWriter(output);
                        headerRW.Write(writer, header);
                        listSerializer.Write(writer, listValue);
                        return output.ToArray();
                    """
            : ReturnsEnumerable
                ? $$$"""
                        var listSerializer = new {{{ReturnType.BSATNName}}}();
                        var listValue = global::System.Linq.Enumerable.ToList(returnValue);
                        var header = new global::SpacetimeDB.Internal.ViewResultHeader.RowData(default);
                        var headerRW = new global::SpacetimeDB.Internal.ViewResultHeader.BSATN();
                        using var output = new System.IO.MemoryStream();
                        using var writer = new System.IO.BinaryWriter(output);
                        headerRW.Write(writer, header);
                        listSerializer.Write(writer, listValue);
                        return output.ToArray();
                    """
            : $$$"""
                    {{{ReturnType.BSATNName}}} returnRW = new();
                    var header = new global::SpacetimeDB.Internal.ViewResultHeader.RowData(default);
                    var headerRW = new global::SpacetimeDB.Internal.ViewResultHeader.BSATN();
                    using var output = new System.IO.MemoryStream();
                    using var writer = new System.IO.BinaryWriter(output);
                    headerRW.Write(writer, header);
                    returnRW.Write(writer, returnValue);
                    return output.ToArray();            
                """;

        var invocationArgs =
            Parameters.Length == 0
                ? ""
                : ", " + string.Join(", ", Parameters.Select(p => p.Identifier));
        return $$$"""
            sealed class {{{Name}}}ViewDispatcher : {{{interfaceName}}} {
                {{{MemberDeclaration.GenerateBsatnFields(Accessibility.Private, Parameters)}}}
                
                public SpacetimeDB.Internal.RawViewDefV10 {{{makeViewDefMethod}}}(SpacetimeDB.BSATN.ITypeRegistrar registrar)
                    => {{{GenerateViewDef(index)}}}

                public static byte[] Invoke(
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
    private readonly string? declaringAssembly;
    public readonly string Name;
    public readonly string? CanonicalName;
    public readonly ReducerKind Kind;
    public readonly string FullName;
    public readonly EquatableArray<MemberDeclaration> Args;
    public readonly Scope Scope;
    private readonly bool HasWrongSignature;

    public string Identifier => EscapeIdentifier(Name);

    public ReducerDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
    {
        declaringAssembly = Module.UsesSharedContexts(context.SemanticModel.Compilation)
            ? context.SemanticModel.Compilation.Assembly.Identity.ToString()
            : null;
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
        CanonicalName = attr.Name;
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
                Args.Select(a => $"{a.Identifier}{TypeUse.BsatnFieldSuffix}.Read(reader)")
                    .Prepend("(SpacetimeDB.ReducerContext)ctx")
            )})";

        return $$"""
             sealed class {{Identifier}}: SpacetimeDB.Internal.IReducer {
                 {{MemberDeclaration.GenerateBsatnFields(Accessibility.Private, Args)}}

                 public SpacetimeDB.Internal.RawReducerDefV10 MakeReducerDef(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                     SourceName: nameof({{Identifier}}),
                     Params: [{{MemberDeclaration.GenerateDefs(Args)}}],
                     Visibility: SpacetimeDB.Internal.FunctionVisibility.ClientCallable,
                     OkReturnType: SpacetimeDB.BSATN.AlgebraicType.Unit,
                     ErrReturnType: new SpacetimeDB.BSATN.AlgebraicType.String(default)
                 );

                 public SpacetimeDB.Internal.Lifecycle? Lifecycle => {{Kind switch
        {
            ReducerKind.Init => "SpacetimeDB.Internal.Lifecycle.Init",
            ReducerKind.ClientConnected => "SpacetimeDB.Internal.Lifecycle.OnConnect",
            ReducerKind.ClientDisconnected => "SpacetimeDB.Internal.Lifecycle.OnDisconnect",
            _ => "null"
        }}};

                 public static void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx) {
                     {{invocation}};
                 }
             }
             """;
    }

    public Scope.Extensions GenerateSchedule()
    {
        var extensions = new Scope.Extensions(Scope, FullName);
        var functionName = string.IsNullOrEmpty(CanonicalName)
            ? $"nameof({Identifier})"
            : SymbolDisplay.FormatLiteral(CanonicalName!, true);
        if (declaringAssembly is not null)
        {
            var cacheName = $"__Schedule{Name}Name";
            extensions.Contents.Append(
                $$"""
                private static class {{cacheName}}
                {
                    internal static readonly string Name = global::SpacetimeDB.Internal.Module.ResolveFunctionName({{SymbolDisplay.FormatLiteral(
                    declaringAssembly,
                    true
                )}}, nameof({{Identifier}}), {{(string.IsNullOrEmpty(CanonicalName) ? "null" : SymbolDisplay.FormatLiteral(CanonicalName!, true))}});
                    // Prevent eager initialization before the root installs namespace placements.
                    static {{cacheName}}() { }
                }
                
                """
            );
            functionName = cacheName + ".Name";
        }

        // Mark the API as unstable. We use name `STDB_UNSTABLE` because:
        // 1. It's a close equivalent of the `unstable` Cargo feature in Rust.
        // 2. Our diagnostic IDs use either BSATN or STDB prefix depending on the package.
        // 3. We don't expect to mark individual experimental features with numeric IDs, so we don't use the standard 1234 suffix.
        extensions.Contents.Append(
            $$"""
            [System.Diagnostics.CodeAnalysis.Experimental("STDB_UNSTABLE")]
            public static void VolatileNonatomicScheduleImmediate{{Name}}({{string.Join(
                ", ",
                Args.Select(a => $"{a.Type.Name} {a.Identifier}")
            )}}) {
                using var stream = new MemoryStream();
                using var writer = new BinaryWriter(stream);
                {{string.Join(
                    "\n",
                    Args.Select(a => $"new {a.Type.ToBSATNString()}().Write(writer, {a.Identifier});")
                )}}
                SpacetimeDB.Internal.IReducer.VolatileNonatomicScheduleImmediate({{functionName}}, stream);
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
    private readonly string? declaringAssembly;
    public readonly string Name;
    public readonly string? CanonicalName;
    public readonly string FullName;
    public readonly EquatableArray<MemberDeclaration> Args;
    public readonly Scope Scope;
    private readonly bool HasWrongSignature;
    public readonly TypeUse ReturnType;
    private readonly bool HasTxWrapper;
    private readonly TypeUse? TxPayloadType;
    private readonly bool TxPayloadIsUnit;

    public string Identifier => EscapeIdentifier(Name);

    public ProcedureDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
    {
        declaringAssembly = Module.UsesSharedContexts(context.SemanticModel.Compilation)
            ? context.SemanticModel.Compilation.Assembly.Identity.ToString()
            : null;
        var methodSyntax = (MethodDeclarationSyntax)context.TargetNode;
        var method = (IMethodSymbol)context.TargetSymbol;
        var attr = context.Attributes.Single().ParseAs<ProcedureAttribute>();

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

        if (
            method.ReturnType
                is INamedTypeSymbol
                {
                    Name: "TxOutcome",
                    ContainingType: { Name: "ProcedureContext" }
                } txOutcome
            && txOutcome.TypeArguments.Length == 1
        )
        {
            HasTxWrapper = true;
            TxPayloadType = TypeUse.Parse(method, txOutcome.TypeArguments[0], diag);
            TxPayloadIsUnit = TxPayloadType.BSATNName == "SpacetimeDB.BSATN.Unit";
        }
        else if (
            method.ReturnType
                is INamedTypeSymbol
                {
                    Name: "TxResult",
                    ContainingType: { Name: "ProcedureContext" }
                } txResult
            && txResult.TypeArguments.Length == 2
        )
        {
            HasTxWrapper = true;
            TxPayloadType = TypeUse.Parse(method, txResult.TypeArguments[0], diag);
            TxPayloadIsUnit = TxPayloadType.BSATNName == "SpacetimeDB.BSATN.Unit";
        }

        CanonicalName = attr.Name;

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
            Args.Length == 0 ? "" : ", " + string.Join(", ", Args.Select(a => a.Identifier));
        var invocation = $"{FullName}((SpacetimeDB.ProcedureContext)ctx{invocationArgs})";

        var txPayload = TxPayloadType ?? ReturnType;
        var txPayloadIsUnit = TxPayloadIsUnit;

        string[] bodyLines;

        if (HasWrongSignature)
        {
            bodyLines =
            [
                "throw new System.InvalidOperationException(\"Invalid procedure signature.\");",
            ];
        }
        else if (HasTxWrapper)
        {
            string[] successLines = txPayloadIsUnit
                ? ["return System.Array.Empty<byte>();"]
                :
                [
                    "using var output = new MemoryStream();",
                    "using var writer = new BinaryWriter(output);",
                    "__txReturnRW.Write(writer, outcome.Value!);",
                    "return output.ToArray();",
                ];

            bodyLines =
            [
                $"var outcome = {invocation};",
                "if (!outcome.IsSuccess)",
                "{",
                "    throw outcome.Error ?? new System.InvalidOperationException(\"Transaction failed.\");",
                "}",
                .. successLines,
            ];
        }
        else if (ReturnType.Name == "SpacetimeDB.Unit")
        {
            bodyLines = [$"{invocation};", "return System.Array.Empty<byte>();"];
        }
        else
        {
            var serializer = $"new {ReturnType.ToBSATNString()}()";
            bodyLines =
            [
                $"var result = {invocation};",
                "using var output = new MemoryStream();",
                "using var writer = new BinaryWriter(output);",
                $"{serializer}.Write(writer, result);",
                "return output.ToArray();",
            ];
        }

        var invokeBody = string.Join("\n", bodyLines.Select(line => $"                    {line}"));
        var paramReads =
            Args.Length == 0
                ? string.Empty
                : string.Join(
                    "\n",
                    Args.Select(a =>
                        $"                    var {a.Identifier} = {a.Identifier}{TypeUse.BsatnFieldSuffix}.Read(reader);"
                    )
                ) + "\n";

        var returnTypeExpr = HasTxWrapper
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
        if (HasTxWrapper && !txPayloadIsUnit)
        {
            classFields +=
                $"\n        private {txPayload.BSATNName} __txReturnRW = new {txPayload.BSATNName}();";
        }

        return $$$"""
            sealed class {{{Identifier}}} : SpacetimeDB.Internal.IProcedure {
                {{{classFields}}}

                public SpacetimeDB.Internal.RawProcedureDefV10 MakeProcedureDef(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new(
                    SourceName: nameof({{{Identifier}}}),
                    Params: [{{{MemberDeclaration.GenerateDefs(Args)}}}],
                    ReturnType: {{{returnTypeExpr}}},
                    Visibility: SpacetimeDB.Internal.FunctionVisibility.ClientCallable
                );

                public static byte[] Invoke(BinaryReader reader, SpacetimeDB.Internal.IProcedureContext ctx) {
                    {{{paramReads}}}{{{invokeBody}}}
                }
            }
            """;
    }

    public Scope.Extensions GenerateSchedule()
    {
        var extensions = new Scope.Extensions(Scope, FullName);
        var functionName = string.IsNullOrEmpty(CanonicalName)
            ? $"nameof({Identifier})"
            : SymbolDisplay.FormatLiteral(CanonicalName!, true);
        if (declaringAssembly is not null)
        {
            var cacheName = $"__Schedule{Name}Name";
            extensions.Contents.Append(
                $$"""
                private static class {{cacheName}}
                {
                    internal static readonly string Name = global::SpacetimeDB.Internal.Module.ResolveFunctionName({{SymbolDisplay.FormatLiteral(
                    declaringAssembly,
                    true
                )}}, nameof({{Identifier}}), {{(string.IsNullOrEmpty(CanonicalName) ? "null" : SymbolDisplay.FormatLiteral(CanonicalName!, true))}});
                    // Prevent eager initialization before the root installs namespace placements.
                    static {{cacheName}}() { }
                }
                
                """
            );
            functionName = cacheName + ".Name";
        }

        // Mark the API as unstable. We use name `STDB_UNSTABLE` because:
        // 1. It's a close equivalent of the `unstable` Cargo feature in Rust.
        // 2. Our diagnostic IDs use either BSATN or STDB prefix depending on the package.
        // 3. We don't expect to mark individual experimental features with numeric IDs, so we don't use the standard 1234 suffix.
        extensions.Contents.Append(
            $$"""
            [System.Diagnostics.CodeAnalysis.Experimental("STDB_UNSTABLE")]
            public static void VolatileNonatomicScheduleImmediate{{Name}}({{string.Join(
                ", ",
                Args.Select(a => $"{a.Type.Name} {a.Identifier}")
            )}}) {
                using var stream = new MemoryStream();
                using var writer = new BinaryWriter(stream);
                {{string.Join(
                    "\n",
                    Args.Select(a => $"new {a.Type.ToBSATNString()}().Write(writer, {a.Identifier});")
                )}}
                SpacetimeDB.Internal.ProcedureExtensions.VolatileNonatomicScheduleImmediate({{functionName}}, stream);
            }
            """
        );

        return extensions;
    }
}

record HttpHandlerDeclaration
{
    public readonly string Name;
    public readonly string FullName;
    private readonly bool HasWrongSignature;

    public string Identifier => EscapeIdentifier(Name);

    public HttpHandlerDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
    {
        var methodSyntax = (MethodDeclarationSyntax)context.TargetNode;
        var method = (IMethodSymbol)context.TargetSymbol;
        var compilation = context.SemanticModel.Compilation;

        if (method.Arity != 0 || method.Parameters.Length != 2)
        {
            diag.Report(ErrorDescriptor.HttpHandlerSignature, methodSyntax);
            HasWrongSignature = true;
        }

        if (
            method.Parameters.FirstOrDefault()?.Type
                is not INamedTypeSymbol
                {
                    Name: "HandlerContext",
                    Arity: 0,
                    ContainingType: null,
                    ContainingNamespace:
                    { Name: "SpacetimeDB", ContainingNamespace: { IsGlobalNamespace: true } }
                }
            && methodSyntax.ParameterList.Parameters.FirstOrDefault()?.Type
                is not IdentifierNameSyntax { Identifier.ValueText: "HandlerContext" }
            && methodSyntax.ParameterList.Parameters.FirstOrDefault()?.Type
                is not QualifiedNameSyntax
                {
                    Left: IdentifierNameSyntax { Identifier.ValueText: "SpacetimeDB" },
                    Right: IdentifierNameSyntax { Identifier.ValueText: "HandlerContext" }
                }
            && methodSyntax.ParameterList.Parameters.FirstOrDefault()?.Type
                is not QualifiedNameSyntax
                {
                    Left: AliasQualifiedNameSyntax
                    {
                        Alias.Identifier.ValueText: "global",
                        Name: IdentifierNameSyntax { Identifier.ValueText: "SpacetimeDB" }
                    },
                    Right: IdentifierNameSyntax { Identifier.ValueText: "HandlerContext" }
                }
        )
        {
            diag.Report(ErrorDescriptor.HttpHandlerContextParam, methodSyntax);
            HasWrongSignature = true;
        }

        if (
            method.Parameters.ElementAtOrDefault(1)?.Type is not { } requestType
            || compilation.GetTypeByMetadataName("SpacetimeDB.HttpRequest")
                is not { } expectedRequestType
            || !SymbolEqualityComparer.Default.Equals(requestType, expectedRequestType)
        )
        {
            diag.Report(ErrorDescriptor.HttpHandlerRequestParam, methodSyntax);
            HasWrongSignature = true;
        }

        if (
            compilation.GetTypeByMetadataName("SpacetimeDB.HttpResponse")
                is not { } expectedResponseType
            || !SymbolEqualityComparer.Default.Equals(method.ReturnType, expectedResponseType)
        )
        {
            diag.Report(ErrorDescriptor.HttpHandlerReturnType, methodSyntax);
            HasWrongSignature = true;
        }

        Name = method.Name;
        if (Name.Length >= 2)
        {
            var prefix = Name[..2];
            if (prefix is "__" or "on" or "On")
            {
                diag.Report(ErrorDescriptor.HttpHandlerReservedPrefix, (methodSyntax, prefix));
            }
        }

        FullName = SymbolToName(method);
    }

    public string GenerateClass()
    {
        var body = HasWrongSignature
            ? "throw new System.InvalidOperationException(\"Invalid HTTP handler signature.\");"
            : $"return {FullName}((SpacetimeDB.HandlerContext)ctx, request);";

        return $$"""
            sealed class {{Identifier}} : SpacetimeDB.Internal.IHttpHandler {
                public SpacetimeDB.Internal.RawHttpHandlerDefV10 MakeHandlerDef() => new(
                    SourceName: nameof({{Identifier}})
                );

                public static SpacetimeDB.HttpResponse Invoke(
                    SpacetimeDB.HandlerContextBase ctx,
                    SpacetimeDB.HttpRequest request
                ) {
                    {{body}}
                }
            }
            """;
    }
}

record HttpRouterDeclaration
{
    public readonly string FullName;
    public readonly bool IsValid;

    public HttpRouterDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
    {
        var methodSyntax = (MethodDeclarationSyntax)context.TargetNode;
        var method = (IMethodSymbol)context.TargetSymbol;
        var compilation = context.SemanticModel.Compilation;

        if (
            !method.IsStatic
            || method.Arity != 0
            || method.Parameters.Length != 0
            || compilation.GetTypeByMetadataName("SpacetimeDB.Router") is not { } expectedRouterType
            || !SymbolEqualityComparer.Default.Equals(method.ReturnType, expectedRouterType)
        )
        {
            diag.Report(ErrorDescriptor.HttpRouterSignature, methodSyntax);
        }
        else
        {
            IsValid = true;
        }

        FullName = SymbolToName(method);
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

record AssemblyTableAccessor(string Name, string TypeName);

record AssemblyDeclaration(
    string Identity,
    string DescriptorTypeName,
    EquatableArray<NamespaceDeclaration> Mounts,
    string RootOnlyDeclarations,
    string? CaseConversionPolicy,
    EquatableArray<AssemblyTableAccessor> Tables,
    EquatableArray<AssemblyTableAccessor> ReadOnlyTables,
    EquatableArray<AssemblyTableAccessor> Queries
);

[Generator]
public class Module : IIncrementalGenerator
{
    internal static bool UsesSharedContexts(Compilation compilation) =>
        compilation.SyntaxTrees.Any(tree =>
            tree.Options is CSharpParseOptions options
            && options.PreprocessorSymbolNames.Contains("NET10_0_OR_GREATER")
        );

    internal static string AssemblyNamespace(IAssemblySymbol assembly)
    {
        var name = Regex.Replace(assembly.Name, @"[^A-Za-z0-9_]", "_");

        if (name.Length == 0 || char.IsDigit(name[0]))
        {
            name = "_" + name;
        }

        using var sha256 = System.Security.Cryptography.SHA256.Create();
        var hash = sha256.ComputeHash(
            System.Text.Encoding.UTF8.GetBytes(assembly.Identity.ToString())
        );
        var suffix = string.Concat(hash.Take(8).Select(b => b.ToString("X2")));

        return $"SpacetimeDB.Generated.{name}_{suffix}";
    }

    private static string IndentGeneratedCode(string code, int spaces) =>
        code.Replace("\n", "\n" + new string(' ', spaces));

    private static bool NeedsNamespaceContainer(CompositionNode node) =>
        node.Children.Array.Length != 0 || node.Contributors.Array.Length > 1;

    private static string NamespaceContainerType(
        CompositionNode node,
        string container,
        string extensionNamespace,
        Dictionary<string, AssemblyDeclaration> assemblies
    ) =>
        NeedsNamespaceContainer(node)
            ? $"global::{extensionNamespace}.NamespaceAccessors.Scope{node.Id}.{container}"
            : $"{assemblies[node.Contributors.Array[0]].DescriptorTypeName}.{container}";

    private static string GenerateNamespaceContainers(
        IEnumerable<CompositionNode> nodes,
        string extensionNamespace,
        Dictionary<string, AssemblyDeclaration> assemblies,
        SourceProductionContext context
    )
    {
        var scopes = new List<string>();
        foreach (var node in nodes.Where(node => node.Id != 0 && NeedsNamespaceContainer(node)))
        {
            var containers = new List<string>();
            foreach (var container in new[] { "Tables", "ReadOnlyTables", "Queries" })
            {
                var members = new List<string>();
                var used = new Dictionary<string, string>(StringComparer.Ordinal)
                {
                    ["Tables"] = "generated namespace container",
                    ["ReadOnlyTables"] = "generated namespace container",
                    ["Queries"] = "generated namespace container",
                    ["__instanceId"] = "generated instance field",
                };
                foreach (var contributor in node.Contributors)
                {
                    var assembly = assemblies[contributor];
                    var tables = container switch
                    {
                        "Tables" => assembly.Tables,
                        "Queries" => assembly.Queries,
                        _ => assembly.ReadOnlyTables,
                    };
                    var invocation = container == "Queries" ? "()" : "";
                    foreach (var table in tables)
                    {
                        if (used.TryGetValue(table.Name, out var previous))
                        {
                            if (container == "Tables")
                            {
                                context.ReportDiagnostic(
                                    ErrorDescriptor.NamespaceAccessorCollision.ToDiag(
                                        ($"{node.AccessorPath}.{table.Name}", previous, contributor)
                                    )
                                );
                            }
                            continue;
                        }
                        used.Add(table.Name, contributor);
                        members.Add(
                            $"public {table.TypeName} {EscapeIdentifier(table.Name)}{invocation} => new {assembly.DescriptorTypeName}.{container}(__instanceId).{EscapeIdentifier(table.Name)}{invocation};"
                        );
                    }
                }
                foreach (var child in node.Children)
                {
                    var mount = child.Mount!;
                    if (used.TryGetValue(mount.Accessor, out var previous))
                    {
                        if (container == "Tables")
                        {
                            context.ReportDiagnostic(
                                ErrorDescriptor.NamespaceAccessorCollision.ToDiag(
                                    (
                                        $"{node.AccessorPath}.{mount.Accessor}",
                                        previous,
                                        mount.AssemblyIdentity
                                    )
                                )
                            );
                        }
                        continue;
                    }
                    used.Add(mount.Accessor, mount.AssemblyIdentity);
                    members.Add(
                        $"public {NamespaceContainerType(child, container, extensionNamespace, assemblies)} {mount.AccessorIdentifier} => new(NamespaceBindings.Child{child.Id}[__instanceId]);"
                    );
                }
                containers.Add(
                    $$"""
                    public readonly struct {{container}} {
                        private readonly int __instanceId;
                        public {{container}}(int instanceId) { __instanceId = instanceId; }
                        {{IndentGeneratedCode(string.Join("\n", members), 4)}}
                    }
                    """
                );
            }
            scopes.Add(
                $$"""
                public static class Scope{{node.Id}} {
                    {{IndentGeneratedCode(string.Join("\n", containers), 4)}}
                }
                """
            );
        }
        return scopes.Count == 0
            ? ""
            : $$"""
                public static class NamespaceAccessors {
                    {{IndentGeneratedCode(string.Join("\n", scopes), 4)}}
                }
                """;
    }

    private static string GenerateContextSelectors(
        CompositionNode[] nodes,
        SourceProductionContext context
    )
    {
        if (nodes.Length == 1)
        {
            return "";
        }
        var scopes = new List<string>();
        foreach (var node in nodes)
        {
            foreach (var child in node.Children)
            {
                var accessor = child.Mount!.Accessor;
                if (
                    accessor == "__context"
                    || accessor == "__instanceId"
                    || accessor == $"Scope{node.Id}"
                )
                {
                    context.ReportDiagnostic(
                        ErrorDescriptor.NamespaceAccessorCollision.ToDiag(
                            (
                                child.AccessorPath,
                                "generated context selector",
                                child.Mount.AssemblyIdentity
                            )
                        )
                    );
                }
            }
            var members = node.Children.Select(
                (child, index) =>
                    $"public Scope{child.Id}<TContext> {child.Mount!.AccessorIdentifier} => new(__context, NamespaceBindings.{(node.Id == 0 ? $"Mount{index}" : $"Child{child.Id}")}[__instanceId]);"
            );
            scopes.Add(
                $$"""
                public readonly struct Scope{{node.Id}}<TContext>
                    where TContext : global::SpacetimeDB.Internal.IModuleContext<TContext>
                {
                    private readonly TContext __context;
                    private readonly int __instanceId;
                    public Scope{{node.Id}}(TContext context, int instanceId) { __context = context; __instanceId = instanceId; }
                    public static implicit operator TContext(Scope{{node.Id}}<TContext> selection) => selection.__context.SelectInstance(selection.__instanceId);
                    {{IndentGeneratedCode(string.Join("\n", members), 4)}}
                }
                """
            );
        }
        return $$"""
            public static class ContextSelectors {
                {{IndentGeneratedCode(string.Join("\n", scopes), 4)}}
                extension<TContext>(TContext context)
                    where TContext : global::SpacetimeDB.Internal.IModuleContext<TContext>
                {
                    public Scope0<TContext> As => new(context, context.InstanceId);
                }
            }
            """;
    }

    private static EquatableArray<AssemblyDeclaration> DiscoverAssemblies(
        Compilation compilation,
        DiagReporter diag,
        CancellationToken cancellationToken
    )
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (
            !compilation.SyntaxTrees.Any(tree =>
                tree.Options is CSharpParseOptions options
                && options.PreprocessorSymbolNames.Contains("NET10_0_OR_GREATER")
            )
        )
        {
            return new(ImmutableArray<AssemblyDeclaration>.Empty);
        }

        var markerType = compilation.GetTypeByMetadataName("SpacetimeDB.ModuleDescriptorAttribute");
        if (markerType is null)
        {
            return new(ImmutableArray<AssemblyDeclaration>.Empty);
        }

        var visited = new HashSet<AssemblyIdentity> { compilation.Assembly.Identity };
        var pending = new Stack<IAssemblySymbol>(
            compilation.SourceModule.ReferencedAssemblySymbols
        );
        var assemblies = new List<AssemblyDeclaration>();
        var symbols = new Dictionary<string, IAssemblySymbol>(StringComparer.Ordinal);
        while (pending.Count > 0)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var assembly = pending.Pop();
            if (!visited.Add(assembly.Identity))
            {
                continue;
            }

            // Unmarked utility assemblies may reference contributing modules.
            foreach (var module in assembly.Modules)
            {
                foreach (var reference in module.ReferencedAssemblySymbols)
                {
                    pending.Push(reference);
                }
            }

            var marker = assembly
                .GetAttributes()
                .FirstOrDefault(attribute =>
                    SymbolEqualityComparer.Default.Equals(attribute.AttributeClass, markerType)
                );
            if (
                marker is null
                || marker.ConstructorArguments.Length != 1
                || marker.ConstructorArguments[0].Kind != TypedConstantKind.Type
                || marker.ConstructorArguments[0].Value is not INamedTypeSymbol descriptor
            )
            {
                continue;
            }

            symbols.Add(assembly.Identity.ToString(), assembly);
            assemblies.Add(
                new AssemblyDeclaration(
                    assembly.Identity.ToString(),
                    descriptor.ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat),
                    new(ImmutableArray<NamespaceDeclaration>.Empty),
                    descriptor
                        .GetMembers("RootOnlyDeclarations")
                        .OfType<IFieldSymbol>()
                        .FirstOrDefault()
                        ?.ConstantValue as string
                        ?? "",
                    descriptor
                        .GetMembers("CaseConversionPolicy")
                        .OfType<IFieldSymbol>()
                        .FirstOrDefault()
                        ?.ConstantValue as string,
                    ReadAccessors("Tables"),
                    ReadAccessors("ReadOnlyTables"),
                    new(
                        descriptor
                            .GetTypeMembers("Queries")
                            .SelectMany(type => type.GetMembers())
                            .OfType<IMethodSymbol>()
                            .Where(method =>
                                method.DeclaredAccessibility == Accessibility.Public
                                && method.MethodKind == MethodKind.Ordinary
                                && method.Parameters.IsEmpty
                                && !method.IsStatic
                            )
                            .Select(method => new AssemblyTableAccessor(
                                method.Name,
                                method.ReturnType.ToDisplayString(
                                    SymbolDisplayFormat.FullyQualifiedFormat
                                )
                            ))
                            .ToImmutableArray()
                    )
                )
            );

            EquatableArray<AssemblyTableAccessor> ReadAccessors(string container) =>
                new(
                    descriptor
                        .GetTypeMembers(container)
                        .SelectMany(type => type.GetMembers())
                        .OfType<IPropertySymbol>()
                        .Where(property => property.DeclaredAccessibility == Accessibility.Public)
                        .Select(property => new AssemblyTableAccessor(
                            property.Name,
                            property.Type.ToDisplayString(SymbolDisplayFormat.FullyQualifiedFormat)
                        ))
                        .ToImmutableArray()
                );
        }

        // Read mounts after discovery so markers can target any descriptor-bearing dependency.
        return new(
            assemblies
                .OrderBy(assembly => assembly.Identity, StringComparer.Ordinal)
                .Select(assembly =>
                    assembly with
                    {
                        Mounts = NamespaceDeclaration.Parse(
                            compilation,
                            symbols[assembly.Identity],
                            symbols.Keys,
                            assembly.Tables.Select(table => table.Name),
                            diag,
                            cancellationToken
                        ),
                    }
                )
                .ToImmutableArray()
        );
    }

    private static string EscapeStringLiteral(string s) =>
        s.Replace("\\", "\\\\")
            .Replace("\"", "\\\"")
            .Replace("\r", "\\r")
            .Replace("\n", "\\n")
            .Replace("\t", "\\t");

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

    private static (
        TTableAccessors tableAccessors,
        TSettings settings,
        TTableDecls tableDecls,
        TReducers addReducers,
        TProcedures addProcedures,
        THttpHandlers addHttpHandlers,
        TReadOnlyAccessors readOnlyAccessors,
        THttpRouters httpRouters,
        TViews views,
        TRlsFilters rlsFilters,
        TColumnDefaultValues columnDefaultValues
    ) FlattenModuleOutputInputs<
        TTableAccessors,
        TSettings,
        TTableDecls,
        TReducers,
        TProcedures,
        THttpHandlers,
        TReadOnlyAccessors,
        THttpRouters,
        TViews,
        TRlsFilters,
        TColumnDefaultValues
    >(
        (
            (
                (
                    (
                        (
                            (
                                (
                                    (((TTableAccessors, TSettings), TTableDecls), TReducers),
                                    TProcedures
                                ),
                                THttpHandlers
                            ),
                            TReadOnlyAccessors
                        ),
                        THttpRouters
                    ),
                    TViews
                ),
                TRlsFilters
            ),
            TColumnDefaultValues
        ) tuple
    )
    {
        var (
            (
                (
                    (
                        (
                            (
                                (
                                    (((tableAccessors, settings), tableDecls), addReducers),
                                    addProcedures
                                ),
                                addHttpHandlers
                            ),
                            readOnlyAccessors
                        ),
                        httpRouters
                    ),
                    views
                ),
                rlsFilters
            ),
            columnDefaultValues
        ) = tuple;

        return (
            tableAccessors,
            settings,
            tableDecls,
            addReducers,
            addProcedures,
            addHttpHandlers,
            readOnlyAccessors,
            httpRouters,
            views,
            rlsFilters,
            columnDefaultValues
        );
    }

    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        var settings = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: typeof(SettingsAttribute).FullName,
                predicate: (node, ct) => true,
                transform: (context, ct) =>
                    context.ParseWithDiags(diag => new SettingsDeclaration(context, diag))
            )
            .ReportDiagnostics(context)
            .WithTrackingName("SpacetimeDB.Settings.Parse");

        var settingsArray = CollectDistinct(
            "Settings",
            context,
            settings,
            s => s.FullName,
            s => s.FullName
        );

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

        var tableDecls = CollectDistinct(
            "TableDecl",
            context,
            tables,
            t => t.FullName,
            t => t.FullName
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
                .Select(
                    (r, ct) =>
                        (r.Name, r.FullName, r.CanonicalName, r.Kind, Class: r.GenerateClass())
                )
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
                .Select((p, ct) => (p.Name, p.FullName, p.CanonicalName, Class: p.GenerateClass()))
                .WithTrackingName("SpacetimeDB.Procedure.GenerateClass"),
            p => p.Name,
            p => p.FullName
        );

        var httpHandlers = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: typeof(HttpHandlerAttribute).FullName,
                predicate: (node, ct) => true,
                transform: (context, ct) =>
                    context.ParseWithDiags(diag => new HttpHandlerDeclaration(context, diag))
            )
            .ReportDiagnostics(context)
            .WithTrackingName("SpacetimeDB.HttpHandler.Parse");

        var addHttpHandlers = CollectDistinct(
            "HttpHandler",
            context,
            httpHandlers
                .Select((h, ct) => (h.Name, h.FullName, Class: h.GenerateClass()))
                .WithTrackingName("SpacetimeDB.HttpHandler.GenerateClass"),
            h => h.Name,
            h => h.FullName
        );

        var httpRouters = context
            .SyntaxProvider.ForAttributeWithMetadataName(
                fullyQualifiedMetadataName: typeof(HttpRouterAttribute).FullName,
                predicate: (node, ct) => true,
                transform: (context, ct) =>
                    context.ParseWithDiags(diag => new HttpRouterDeclaration(context, diag))
            )
            .ReportDiagnostics(context)
            .Collect()
            .Select((routers, ct) => new EquatableArray<HttpRouterDeclaration>(routers))
            .WithTrackingName("SpacetimeDB.HttpRouter.Collect");

        var tableAccessors = CollectDistinct(
            "Table",
            context,
            tables
                .SelectMany((t, ct) => t.GenerateTableAccessors())
                .WithTrackingName("SpacetimeDB.Table.GenerateTableAccessors"),
            v => v.TableAccessorName,
            v => v.TableName
        );

        var readOnlyAccessors = CollectDistinct(
            "TableReadOnly",
            context,
            tables
                .SelectMany((t, ct) => t.GenerateReadOnlyAccessors())
                .WithTrackingName("SpacetimeDB.Table.GenerateReadOnlyAccessors"),
            v => v.TableAccessorName + "ReadOnly",
            v => v.TableName
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
            v => v.TableName + "_" + v.ColumnId,
            v => v.TableName + "_" + v.ColumnId
        );

        var moduleOutputInputs = tableAccessors
            .Combine(settingsArray)
            .Combine(tableDecls)
            .Combine(addReducers)
            .Combine(addProcedures)
            .Combine(addHttpHandlers)
            .Combine(readOnlyAccessors)
            .Combine(httpRouters)
            .Combine(views)
            .Combine(rlsFiltersArray)
            .Combine(columnDefaultValues)
            .Select((tuple, ct) => FlattenModuleOutputInputs(tuple));

        var environment = EnvironmentGenerator
            .Declarations(context)
            .Select(
                (types, _) =>
                    (
                        HasDeclarations: types.Length != 0,
                        Registrations: EnvironmentGenerator.RegistrationCode(types)
                    )
            );
        var extensionNamespace = context
            .CompilationProvider.Select(
                (compilation, _) =>
                    (
                        Name: AssemblyNamespace(compilation.Assembly),
                        Identity: compilation.Assembly.Identity.ToString(),
                        SharedContexts: UsesSharedContexts(compilation)
                    )
            )
            .Combine(environment)
            .Select(
                (input, _) =>
                    (
                        input.Left.Name,
                        input.Left.Identity,
                        input.Left.SharedContexts,
                        HasEnvironment: input.Right.HasDeclarations,
                        EnvironmentRegistrations: input.Right.Registrations
                    )
            );

        var referencedAssemblies = context
            .CompilationProvider.SelectMany(
                (compilation, ct) =>
                    new[]
                    {
                        DiagReporter.With(
                            Location.None,
                            diag => DiscoverAssemblies(compilation, diag, ct)
                        ),
                    }
            )
            .ReportDiagnostics(context)
            .WithTrackingName("SpacetimeDB.Assembly.Discover")
            .Collect()
            .Select(
                (results, _) =>
                    results.Length == 0
                        ? new EquatableArray<AssemblyDeclaration>(
                            ImmutableArray<AssemblyDeclaration>.Empty
                        )
                        : results.Single()
            );
        var namespaceDeclarations = context
            .CompilationProvider.Combine(referencedAssemblies)
            .Combine(tableDecls)
            .SelectMany(
                (input, ct) =>
                    new[]
                    {
                        DiagReporter.With(
                            Location.None,
                            diag =>
                                NamespaceDeclaration.Parse(
                                    input.Left.Left,
                                    input.Left.Left.Assembly,
                                    input.Left.Right.Select(assembly => assembly.Identity),
                                    input.Right.SelectMany(t =>
                                        t.TableAccessors.Select(a => a.Name)
                                    ),
                                    diag,
                                    ct
                                )
                        ),
                    }
            )
            .ReportDiagnostics(context)
            .WithTrackingName("SpacetimeDB.Namespace.Parse")
            .Collect();

        var composition = extensionNamespace
            .Combine(referencedAssemblies)
            .Combine(namespaceDeclarations)
            .SelectMany(
                (input, ct) =>
                    new[]
                    {
                        DiagReporter.With(
                            Location.None,
                            diag =>
                                ModuleComposition.Build(
                                    input.Left.Left.Identity,
                                    input.Right.SelectMany(mounts => mounts),
                                    input.Left.Right,
                                    diag,
                                    ct
                                )
                        ),
                    }
            )
            .ReportDiagnostics(context)
            .WithTrackingName("SpacetimeDB.Composition.Build");

        // Register the generated source code with the compilation context as part of module publishing
        // Once the compilation is complete, the generated code will be used to create tables and reducers in the database
        context.RegisterSourceOutput(
            moduleOutputInputs
                .Combine(extensionNamespace)
                .Combine(referencedAssemblies)
                .Combine(composition.Collect()),
            (context, input) =>
            {
                var (
                    (
                        (
                            inputs,
                            (
                                extensionNamespaceName,
                                identity,
                                sharedContexts,
                                hasEnvironment,
                                environmentRegistrations
                            )
                        ),
                        assemblies
                    ),
                    compositions
                ) = input;
                if (compositions.Length != 1 || !compositions[0].IsValid)
                {
                    return;
                }
                var tree = compositions[0];
                var handlesNamespace = sharedContexts
                    ? extensionNamespaceName
                    : "SpacetimeDB.Internal";
                var nodes = tree.Nodes.ToArray();
                var assemblyByIdentity = assemblies.ToDictionary(
                    a => a.Identity,
                    StringComparer.Ordinal
                );
                var publicScopeAssemblies = tree
                    .Root.Contributors.Skip(1)
                    .Select(contributor => assemblyByIdentity[contributor])
                    .ToArray();
                foreach (var node in nodes.Skip(1))
                {
                    foreach (var contributor in node.Contributors)
                    {
                        var assembly = assemblyByIdentity[contributor];
                        if (assembly.RootOnlyDeclarations.Length != 0)
                        {
                            context.ReportDiagnostic(
                                ErrorDescriptor.MountedRootOnlyDeclarations.ToDiag(
                                    (
                                        assembly.Identity,
                                        node.AccessorPath,
                                        assembly.RootOnlyDeclarations
                                    )
                                )
                            );
                        }
                    }
                }

                string Descriptor(string contributor) =>
                    contributor == identity
                        ? $"global::{extensionNamespaceName}.AssemblyDescriptor"
                        : assemblyByIdentity[contributor].DescriptorTypeName;

                string GenerateDispatchRouting(string category, string arguments, string unknownId)
                {
                    // The host assigns IDs depth-first, independently for each category.
                    var routes = nodes
                        .Where(node => category != "HttpHandler" || node.Id == 0)
                        .SelectMany(node =>
                            node.Contributors.Select(contributor =>
                                (Descriptor: Descriptor(contributor), InstanceId: node.Id)
                            )
                        )
                        .Select(route =>
                            $$"""
                            if ((uint)localId < (uint){{route.Descriptor}}.{{category}}Count)
                                return {{route.Descriptor}}.CallLocal{{category}}(localId, {{arguments}}{{(
                                sharedContexts && category != "HttpHandler"
                                    ? $", {route.InstanceId}"
                                    : ""
                            )}});
                            localId -= {{route.Descriptor}}.{{category}}Count;
                            """
                        );
                    return $"if (id < 0) {{ {unknownId} }}\nvar localId = id;\n"
                        + string.Join("\n", routes)
                        + "\n"
                        + unknownId;
                }

                var (
                    tableAccessors,
                    settings,
                    tableDecls,
                    addReducers,
                    addProcedures,
                    addHttpHandlers,
                    readOnlyAccessors,
                    httpRouters,
                    views,
                    rlsFilters,
                    columnDefaultValues
                ) = inputs;

                if (sharedContexts)
                {
                    var generatedNames = new GeneratedNames(
                        (scope, name, first, second) =>
                            context.ReportDiagnostic(
                                ErrorDescriptor.GeneratedNameCollision.ToDiag(
                                    (Location.None, scope, name, first, second)
                                )
                            )
                    );
                    if (nodes.Length > 1)
                    {
                        generatedNames.Add(
                            extensionNamespaceName,
                            "ContextSelectors",
                            "generated context selectors"
                        );
                    }
                    foreach (var table in tableAccessors)
                    {
                        var owner = $"table '{table.TableAccessorName}' on '{table.TableName}'";
                        generatedNames.Add(
                            extensionNamespaceName,
                            EscapeIdentifier(table.TableAccessorName + "Cols"),
                            owner
                        );
                        generatedNames.Add(
                            extensionNamespaceName,
                            EscapeIdentifier(table.TableAccessorName + "IxCols"),
                            owner
                        );
                    }
                }

                string ConsumerAccessors(string container)
                {
                    var receiver = container == "Queries" ? "from" : "db";
                    var members = new List<string>();
                    var used = new Dictionary<string, string>(StringComparer.Ordinal);
                    foreach (var table in tableAccessors)
                        used[table.TableAccessorName] = identity;
                    void Add(string name, string owner, string declaration)
                    {
                        if (used.TryGetValue(name, out var previous))
                        {
                            if (container == "Tables")
                            {
                                context.ReportDiagnostic(
                                    ErrorDescriptor.NamespaceAccessorCollision.ToDiag(
                                        (name, previous, owner)
                                    )
                                );
                            }

                            return;
                        }
                        used.Add(name, owner);
                        members.Add(declaration);
                    }
                    var accessScopes = publicScopeAssemblies
                        .Select(assembly =>
                            (Assembly: assembly, Child: (CompositionNode?)null, Index: 0)
                        )
                        .Concat(
                            tree.Root.Children.Select(
                                (child, index) =>
                                    (
                                        Assembly: assemblyByIdentity[child.Contributors.Array[0]],
                                        Child: (CompositionNode?)child,
                                        Index: index
                                    )
                            )
                        )
                        .OrderBy(scope => scope.Assembly.Identity, StringComparer.Ordinal)
                        .ThenBy(scope => scope.Child?.Mount?.Accessor, StringComparer.Ordinal);
                    foreach (var (assembly, child, index) in accessScopes)
                    {
                        if (child is not null)
                        {
                            var mount = child.Mount!;
                            Add(
                                mount.Accessor,
                                assembly.Identity,
                                $"public {NamespaceContainerType(child, container, extensionNamespaceName, assemblyByIdentity)} {mount.AccessorIdentifier} => new(NamespaceBindings.Mount{index}[global::SpacetimeDB.Internal.Module.GetInstanceId({receiver})]);"
                            );
                        }
                        else
                        {
                            var accessors = container switch
                            {
                                "ReadOnlyTables" => assembly.ReadOnlyTables,
                                "Queries" => assembly.Queries,
                                _ => assembly.Tables,
                            };
                            var invocation = container == "Queries" ? "()" : "";
                            foreach (var table in accessors)
                                Add(
                                    table.Name,
                                    assembly.Identity,
                                    $"public {table.TypeName} {EscapeIdentifier(table.Name)}{invocation} => new {assembly.DescriptorTypeName}.{container}(global::SpacetimeDB.Internal.Module.GetInstanceId({receiver})).{EscapeIdentifier(table.Name)}{invocation};"
                                );
                        }
                    }
                    return string.Join("\n", members);
                }
                var consumerWritableAccessors = ConsumerAccessors("Tables");
                var consumerReadOnlyAccessors = ConsumerAccessors("ReadOnlyTables");
                var consumerQueryAccessors = ConsumerAccessors("Queries");
                var namespaceContainers = GenerateNamespaceContainers(
                    nodes,
                    extensionNamespaceName,
                    assemblyByIdentity,
                    context
                );
                var contextSelectors = GenerateContextSelectors(nodes, context);
                var namespaceBindings =
                    tree.Root.Children.Array.Length == 0
                        ? ""
                        : $$"""
                    internal static class NamespaceBindings {
                        static NamespaceBindings() { }
                        {{IndentGeneratedCode(string.Join("\n", tree.Root.Children.Select((child, index) =>
                            $"internal static readonly global::SpacetimeDB.Internal.NamespaceBinding Mount{index} = new({SymbolDisplay.FormatLiteral(identity, true)}, {SymbolDisplay.FormatLiteral(child.Mount!.Accessor, true)});").Concat(nodes.Where(node => node.ParentId is not null and not 0).Select(node =>
                            $"internal static readonly global::SpacetimeDB.Internal.NamespaceBinding Child{node.Id} = new(null, {SymbolDisplay.FormatLiteral(node.Mount!.Accessor, true)});"))), 4)}}
                    }
                    """;

                var declaredCasePolicy =
                    settings.Array.Length == 1 ? settings.Array[0].CaseConversionPolicy : null;
                var rootCasePolicy = declaredCasePolicy ?? "SnakeCase";

                string Policy(CompositionNode node) =>
                    node.Id == 0
                        ? rootCasePolicy
                        : assemblyByIdentity[node.Contributors.Array[0]].CaseConversionPolicy
                            ?? "SnakeCase";
                string BuilderName(CompositionNode node) =>
                    node.Id == 0
                        ? "global::SpacetimeDB.Internal.Module.RootBuilder"
                        : $"child{node.Id - 1}";

                var compositionRegistration = new List<string>
                {
                    !sharedContexts
                        ? $"global::SpacetimeDB.Internal.Module.InstallNamespaces(new global::SpacetimeDB.Internal.NamespaceRegistry({SymbolDisplay.FormatLiteral(identity, true)}, global::SpacetimeDB.CaseConversionPolicy.{rootCasePolicy}, new (string, string, string?, global::SpacetimeDB.CaseConversionPolicy)[] {{}}));"
                        : "global::SpacetimeDB.Internal.Module.InstallNamespaces(new global::SpacetimeDB.Internal.NamespaceRegistry(new (int, string, string?, global::SpacetimeDB.CaseConversionPolicy, string[])[] {"
                            + string.Join(
                                ",",
                                nodes.Select(node =>
                                    $"({node.ParentId ?? -1}, {SymbolDisplay.FormatLiteral(node.Mount?.Accessor ?? "", true)}, {(node.Mount?.Name is { } name ? SymbolDisplay.FormatLiteral(name, true) : "null")}, global::SpacetimeDB.CaseConversionPolicy.{Policy(node)}, new string[] {{ {string.Join(",", node.Contributors.Select(contributor => SymbolDisplay.FormatLiteral(contributor, true)))} }})"
                                )
                            )
                            + "}));",
                };
                foreach (var node in nodes)
                {
                    if (node.Id != 0)
                    {
                        compositionRegistration.Add(
                            $"var {BuilderName(node)} = new global::SpacetimeDB.Internal.ModuleBuilder();"
                        );
                    }
                    foreach (var contributor in node.Contributors)
                    {
                        compositionRegistration.Add(
                            $"{Descriptor(contributor)}.Register({BuilderName(node)});"
                        );
                    }
                }
                // RegisterSubmodule takes a snapshot: finish descendants before attaching them.
                foreach (var node in Enumerable.Reverse(nodes))
                {
                    foreach (var child in node.Children)
                    {
                        var mount = child.Mount!;
                        compositionRegistration.Add(
                            $"{BuilderName(node)}.RegisterSubmodule({SymbolDisplay.FormatLiteral(mount.Accessor, true)}, {(mount.Name is { } name ? SymbolDisplay.FormatLiteral(name, true) : "null")}, {BuilderName(child)});"
                        );
                    }
                }

                if (settings.Array.Length > 1)
                {
                    context.ReportDiagnostic(
                        ErrorDescriptor.DuplicateSettings.ToDiag(
                            settings.Array.Select(s => s.FullName)
                        )
                    );
                }

                if (httpRouters.Array.Length > 1)
                {
                    context.ReportDiagnostic(
                        ErrorDescriptor.DuplicateHttpRouters.ToDiag(
                            httpRouters.Array.Select(r => r.FullName)
                        )
                    );
                }

                // A shared typespace also has one naming policy. Unspecified dependency
                // settings inherit their containing scope's policy (SnakeCase by default).
                foreach (var node in nodes)
                {
                    foreach (var contributor in node.Contributors.Skip(1))
                    {
                        var assembly = assemblyByIdentity[contributor];
                        if (
                            assembly.CaseConversionPolicy is { } dependencyPolicy
                            && dependencyPolicy != Policy(node)
                        )
                        {
                            context.ReportDiagnostic(
                                ErrorDescriptor.ConflictingCaseConversionPolicies.ToDiag(
                                    (
                                        node.Id == 0 ? "public" : node.AccessorPath,
                                        node.Contributors.Array[0],
                                        Policy(node),
                                        assembly.Identity,
                                        dependencyPolicy
                                    )
                                )
                            );
                        }
                    }
                }

                var instanceParameter = sharedContexts ? ",\nint instanceId = 0" : "";
                var instanceArgument = sharedContexts ? ", instanceId" : "";

                var settingsRegistration = declaredCasePolicy is { } policyName
                    ? $"builder.SetCaseConversionPolicy(SpacetimeDB.CaseConversionPolicy.{policyName});"
                    : string.Empty;

                var explicitTableRegistrations = string.Join(
                    "\n",
                    tableDecls.Array.SelectMany(t =>
                        t.TableAccessors.Where(a => !string.IsNullOrEmpty(a.CanonicalName))
                            .Select(a =>
                                $"builder.RegisterExplicitTableName(\"{EscapeStringLiteral(a.Name)}\", \"{EscapeStringLiteral(a.CanonicalName!)}\");"
                            )
                    )
                );

                var explicitFunctionRegistrations = string.Join(
                    "\n",
                    addReducers
                        .Array.Where(r => !string.IsNullOrEmpty(r.CanonicalName))
                        .Select(r =>
                            $"builder.RegisterExplicitFunctionName(\"{EscapeStringLiteral(r.Name)}\", \"{EscapeStringLiteral(r.CanonicalName!)}\");"
                        )
                        .Concat(
                            addProcedures
                                .Array.Where(p => !string.IsNullOrEmpty(p.CanonicalName))
                                .Select(p =>
                                    $"builder.RegisterExplicitFunctionName(\"{EscapeStringLiteral(p.Name)}\", \"{EscapeStringLiteral(p.CanonicalName!)}\");"
                                )
                        )
                        .Concat(
                            views
                                .Array.Where(v => !string.IsNullOrEmpty(v.CanonicalName))
                                .Select(v =>
                                    $"builder.RegisterExplicitFunctionName(\"{EscapeStringLiteral(v.Name)}\", \"{EscapeStringLiteral(v.CanonicalName!)}\");"
                                )
                        )
                );

                var explicitIndexRegistrations = string.Join(
                    "\n",
                    tableDecls.Array.SelectMany(t =>
                        t.TableAccessors.SelectMany(a =>
                            t.GetIndexes(a)
                                .Where(ix => !string.IsNullOrEmpty(ix.CanonicalName))
                                .Select(ix =>
                                    $"builder.RegisterExplicitIndexName(\"{EscapeStringLiteral(ix.StandardIndexName(a))}\", \"{EscapeStringLiteral(ix.CanonicalName!)}\");"
                                )
                        )
                    )
                );

                var preRegistrationLines = new[]
                {
                    sharedContexts ? environmentRegistrations : "",
                    settingsRegistration,
                    explicitTableRegistrations,
                    explicitFunctionRegistrations,
                    explicitIndexRegistrations,
                }
                    .Where(s => !string.IsNullOrWhiteSpace(s))
                    .ToArray();

                var preRegistrations = string.Join("\n", preRegistrationLines);

                var queryBuilderMembers = string.Join(
                    "\n",
                    tableDecls.Array.SelectMany(t => t.GenerateQueryBuilderMembers())
                );
                var queryBuilderExtensionMembers = string.Join(
                    "\n",
                    tableDecls.Array.SelectMany(t =>
                        t.GenerateQueryBuilderMembers(useExtensions: true)
                    )
                );
                if (string.IsNullOrWhiteSpace(queryBuilderMembers))
                {
                    queryBuilderMembers = "public readonly partial struct QueryBuilder { }";
                }
                // Don't generate the FFI boilerplate if there are no tables or reducers (or procedures, or views, or ...).
                if (
                    tableAccessors.Array.IsEmpty
                    && addReducers.Array.IsEmpty
                    && addProcedures.Array.IsEmpty
                    && addHttpHandlers.Array.IsEmpty
                    && views.Array.IsEmpty
                    && rlsFilters.Array.IsEmpty
                    && assemblies.Array.IsEmpty
                    && !hasEnvironment
                )
                {
                    return;
                }
                context.AddSource(
                    "FFI.cs",
                    $$"""
                    // <auto-generated />
                    #nullable enable
                    // .NET 8 generates a module-local LocalReadOnly which shadows the runtime shell.
                    #pragma warning disable CS0436
                    #pragma warning disable STDB_UNSTABLE

                    #if NET10_0_OR_GREATER
                    global using {{extensionNamespaceName}};
                    #endif
                    using System.Diagnostics.CodeAnalysis;
                    using System.Runtime.CompilerServices;
                    using System.Runtime.InteropServices;
                    using Internal = SpacetimeDB.Internal;
                    using TxContext = SpacetimeDB.Internal.TxContext;
                    
                    #if NET10_0_OR_GREATER
                    [assembly: global::SpacetimeDB.ModuleDescriptorAttribute(
                        typeof(global::{{extensionNamespaceName}}.AssemblyDescriptor))]
                    #endif

                    namespace SpacetimeDB {
                        #if !NET10_0_OR_GREATER
                        {{IndentGeneratedCode(queryBuilderMembers, 4)}}
                        #endif
                        internal static class Handlers {
                            {{IndentGeneratedCode(string.Join("\n", addHttpHandlers.Select(r =>
                                $"public static readonly global::SpacetimeDB.Handler {EscapeIdentifier(r.Name)} = new(nameof({r.FullName}));"
                            )), 8)}}
                        }

                        #if !NET10_0_OR_GREATER
                        public sealed record ReducerContext : DbContext<Local>, Internal.IReducerContext {
                            public global::SpacetimeDB.ModuleEnvironment Env => default;
                            public readonly Identity Sender;
                            public readonly ConnectionId? ConnectionId;
                            public readonly Random Rng;
                            public readonly Timestamp Timestamp;
                            public readonly AuthCtx SenderAuth;
                            // **Note:** must be 0..=u32::MAX
                            internal int CounterUuid;
                            public Identity DatabaseIdentity => Internal.IReducerContext.GetDatabaseIdentity();
                            // We keep this property for compatibility with existing module code.
                            [global::System.Obsolete("ReducerContext.Identity is deprecated. Use DatabaseIdentity instead.")]
                            public Identity Identity => DatabaseIdentity;

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
                            public new global::SpacetimeDB.ModuleEnvironment Env => default;
                            private readonly Local _db = new();

                            internal ProcedureContext(Identity identity, ConnectionId? connectionId, Random random, Timestamp time)
                                : base(identity, connectionId, random, time) {}

                            protected override global::SpacetimeDB.LocalBase CreateLocal() => _db;
                            protected override global::SpacetimeDB.ProcedureTxContextBase CreateTxContext(Internal.TxContext inner) =>
                                _cached ??= new ProcedureTxContext(inner);

                            private ProcedureTxContext? _cached;

                            public Local Db => _db;

                            public TResult WithTx<TResult>(Func<ProcedureTxContext, TResult> body) =>
                                base.WithTx(tx => body((ProcedureTxContext)tx));

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

                        public sealed partial class HandlerContext : global::SpacetimeDB.HandlerContextBase {
                            public new global::SpacetimeDB.ModuleEnvironment Env => default;
                            private readonly Local _db = new();

                            internal HandlerContext(Random random, Timestamp time)
                                : base(random, time) {}

                            protected override global::SpacetimeDB.LocalBase CreateLocal() => _db;
                            protected override global::SpacetimeDB.HandlerTxContextBase CreateTxContext(Internal.TxContext inner) =>
                                _cached ??= new HandlerTxContext(inner);

                            private HandlerTxContext? _cached;

                            [Experimental("STDB_UNSTABLE")]
                            public TResult WithTx<TResult>(Func<HandlerTxContext, TResult> body) =>
                                base.WithTx(tx => body((HandlerTxContext)tx));

                            [Experimental("STDB_UNSTABLE")]
                            public TxOutcome<TResult> TryWithTx<TResult, TError>(
                                Func<HandlerTxContext, Result<TResult, TError>> body)
                                where TError : Exception =>
                                base.TryWithTx(tx => body((HandlerTxContext)tx));

                            public Uuid NewUuidV4()
                            {
                                var bytes = new byte[16];
                                Rng.NextBytes(bytes);
                                return Uuid.FromRandomBytesV4(bytes);
                            }

                            public Uuid NewUuidV7()
                            {
                                var bytes = new byte[4];
                                Rng.NextBytes(bytes);
                                return Uuid.FromCounterV7(ref CounterUuid, Timestamp, bytes);
                            }
                        }

                        public sealed class ProcedureTxContext : global::SpacetimeDB.ProcedureTxContextBase {
                            public new global::SpacetimeDB.ModuleEnvironment Env => default;
                            internal ProcedureTxContext(Internal.TxContext inner) : base(inner) {}

                            public new Local Db => (Local)base.Db;
                        }

                        [Experimental("STDB_UNSTABLE")]
                        public sealed class HandlerTxContext : global::SpacetimeDB.HandlerTxContextBase {
                            public new global::SpacetimeDB.ModuleEnvironment Env => default;
                            internal HandlerTxContext(Internal.TxContext inner) : base(inner) {}

                            public new Local Db => (Local)base.Db;
                        }

                        public sealed class Local : global::SpacetimeDB.LocalBase {
                            {{IndentGeneratedCode(string.Join("\n", tableAccessors.Select(v => v.Getter)), 8)}}
                        }
                        
                        public sealed record ViewContext : DbContext<Internal.LocalReadOnly>, Internal.IViewContext 
                        {
                            public Identity Sender { get; }

                            public global::SpacetimeDB.ModuleEnvironment Env => default;
                            public QueryBuilder From => default;
                        
                            internal ViewContext(Identity sender, Internal.LocalReadOnly db)
                                : base(db)
                            {
                                Sender = sender;
                            }
                        }
                        
                        public sealed record AnonymousViewContext : DbContext<Internal.LocalReadOnly>, Internal.IAnonymousViewContext 
                        {
                            public global::SpacetimeDB.ModuleEnvironment Env => default;
                            public QueryBuilder From => default;

                            internal AnonymousViewContext(Internal.LocalReadOnly db)
                                : base(db) { }
                        }
                        #endif
                    }
                    
                    #if NET10_0_OR_GREATER
                    namespace {{extensionNamespaceName}} {
                        {{IndentGeneratedCode(namespaceBindings + (namespaceContainers.Length == 0 ? "" : "\n" + namespaceContainers) + (contextSelectors.Length == 0 ? "" : "\n" + contextSelectors), 4)}}
                        public static partial class AssemblyDescriptor {
                            public const string? CaseConversionPolicy = {{(declaredCasePolicy is null ? "null" : SymbolDisplay.FormatLiteral(declaredCasePolicy, true))}};
                            public const string RootOnlyDeclarations = {{SymbolDisplay.FormatLiteral(string.Join(", ", new[] {
                                rlsFilters.Array.Length != 0 ? "row-level security filters" : null,
                                environmentRegistrations.Length != 0 ? "environment variables" : null
                            }.Where(value => value is not null).Concat(
                                addReducers.Where(r => r.Kind != ReducerKind.UserDefined)
                                    .Select(r => $"lifecycle reducer {r.FullName} ({r.Kind})")
                            )), true)}};
                            public const int ReducerCount = {{addReducers.Array.Length}};
                            public const int ProcedureCount = {{addProcedures.Array.Length}};
                            public const int HttpHandlerCount = {{addHttpHandlers.Array.Length}};
                            public const int ViewCount = {{views.Array.Count(v => !v.IsAnonymous)}};
                            public const int AnonymousViewCount = {{views.Array.Count(v => v.IsAnonymous)}};

                            public static global::SpacetimeDB.Internal.Errno CallLocalReducer(
                                int id,
                                ulong sender_0,
                                ulong sender_1,
                                ulong sender_2,
                                ulong sender_3,
                                ulong conn_id_0,
                                ulong conn_id_1,
                                global::SpacetimeDB.Timestamp timestamp,
                                global::SpacetimeDB.Internal.BytesSource args,
                                global::SpacetimeDB.Internal.BytesSink error{{IndentGeneratedCode(instanceParameter, 12)}}
                            ) => global::ModuleRegistration.CallLocalReducer(
                                id, sender_0, sender_1, sender_2, sender_3, conn_id_0, conn_id_1, timestamp, args, error{{instanceArgument}}
                            );

                            public static global::SpacetimeDB.Internal.Errno CallLocalProcedure(
                                int id,
                                ulong sender_0,
                                ulong sender_1,
                                ulong sender_2,
                                ulong sender_3,
                                ulong conn_id_0,
                                ulong conn_id_1,
                                global::SpacetimeDB.Timestamp timestamp,
                                global::SpacetimeDB.Internal.BytesSource args,
                                global::SpacetimeDB.Internal.BytesSink result_sink{{IndentGeneratedCode(instanceParameter, 12)}}
                            ) => global::ModuleRegistration.CallLocalProcedure(
                                id, sender_0, sender_1, sender_2, sender_3, conn_id_0, conn_id_1, timestamp, args, result_sink{{instanceArgument}}
                            );

                            public static global::SpacetimeDB.Internal.Errno CallLocalHttpHandler(
                                int id,
                                global::SpacetimeDB.Timestamp timestamp,
                                global::SpacetimeDB.Internal.BytesSource request,
                                global::SpacetimeDB.Internal.BytesSource request_body,
                                global::SpacetimeDB.Internal.BytesSink response_sink,
                                global::SpacetimeDB.Internal.BytesSink response_body_sink
                            ) => global::ModuleRegistration.CallLocalHttpHandler(
                                id, timestamp, request, request_body, response_sink, response_body_sink
                            );

                            public static global::SpacetimeDB.Internal.Errno CallLocalView(
                                int id,
                                ulong sender_0,
                                ulong sender_1,
                                ulong sender_2,
                                ulong sender_3,
                                global::SpacetimeDB.Internal.BytesSource args,
                                global::SpacetimeDB.Internal.BytesSink sink{{IndentGeneratedCode(instanceParameter, 12)}}
                            ) => global::ModuleRegistration.CallLocalView(
                                id, sender_0, sender_1, sender_2, sender_3, args, sink{{instanceArgument}}
                            );

                            public static global::SpacetimeDB.Internal.Errno CallLocalAnonymousView(
                                int id,
                                global::SpacetimeDB.Internal.BytesSource args,
                                global::SpacetimeDB.Internal.BytesSink sink{{IndentGeneratedCode(instanceParameter, 12)}}
                            ) => global::ModuleRegistration.CallLocalAnonymousView(
                                id, args, sink{{instanceArgument}}
                            );

                            public static void Register(
                                global::SpacetimeDB.Internal.ModuleBuilder builder)
                                => global::ModuleRegistration.Register(builder);

                            public readonly struct Tables {
                                {{IndentGeneratedCode(sharedContexts ? "private readonly int __instanceId;\npublic Tables(int instanceId) { __instanceId = instanceId; }" : "", 12)}}
                                {{IndentGeneratedCode(string.Join("\n", tableAccessors.Select(v => v.Getter.Replace("global::SpacetimeDB.Internal.Module.GetInstanceId(db)", "__instanceId"))), 12)}}
                            }

                            public readonly struct ReadOnlyTables {
                                {{IndentGeneratedCode(sharedContexts ? "private readonly int __instanceId;\npublic ReadOnlyTables(int instanceId) { __instanceId = instanceId; }" : "", 12)}}
                                {{IndentGeneratedCode(string.Join("\n", readOnlyAccessors.Select(v => v.ReadOnlyGetter.Replace("global::SpacetimeDB.Internal.Module.GetInstanceId(db)", "__instanceId"))), 12)}}
                            }

                            public readonly partial struct Queries {
                                {{IndentGeneratedCode(sharedContexts ? "private readonly int __instanceId;\npublic Queries(int instanceId) { __instanceId = instanceId; }" : "", 12)}}
                            }
                        }
                        public static class LocalTableExtensions {
                            extension(global::SpacetimeDB.Local db) {
                                {{IndentGeneratedCode(string.Join("\n", tableAccessors.Select(v => v.Getter)), 12)}}
                                {{IndentGeneratedCode(consumerWritableAccessors, 12)}}
                            }
                        }
                        public static class ReadOnlyTableExtensions {
                            extension(global::SpacetimeDB.Internal.LocalReadOnly db) {
                                {{IndentGeneratedCode(string.Join("\n", readOnlyAccessors.Select(v => v.ReadOnlyGetter)), 12)}}
                                {{IndentGeneratedCode(consumerReadOnlyAccessors, 12)}}
                            }
                        }
                        public static partial class QueryTableExtensions {
                            extension(global::SpacetimeDB.QueryBuilder from) {
                                {{IndentGeneratedCode(consumerQueryAccessors, 12)}}
                            }
                        }
                        {{IndentGeneratedCode(queryBuilderExtensionMembers, 4)}}
                    }
                    #endif

                    namespace {{handlesNamespace}}.TableHandles {
                        {{IndentGeneratedCode(string.Join("\n", tableAccessors.Select(v => v.TableAccessor)), 4)}}
                    }
                    
                    {{IndentGeneratedCode(string.Join("\n",
                        views.Array.Where(v => !v.IsAnonymous)
                            .Select((v, i) => v.GenerateDispatcherClass((uint)i))
                            .Concat(
                                views.Array.Where(v => v.IsAnonymous)
                                    .Select((v, i) => v.GenerateDispatcherClass((uint)i))
                            )
                    ), 0)}}
                        
                    namespace {{handlesNamespace}}.ViewHandles {
                        {{IndentGeneratedCode(string.Join("\n", readOnlyAccessors.Array.Select(v => v.ReadOnlyAccessor)), 4)}}
                    }
                    
                    #if !NET10_0_OR_GREATER
                    namespace SpacetimeDB.Internal {
                        public sealed partial class LocalReadOnly {
                            {{IndentGeneratedCode(string.Join("\n", readOnlyAccessors.Select(v => v.ReadOnlyGetter)), 8)}}
                        }
                    }
                    #endif
                    
                    static class ModuleRegistration {
                        // Module host calls are single-threaded in Wasm today, so the generated
                        // entrypoints reuse buffers across calls to avoid per-invocation allocation.
                        private static byte[] reducerArgsBuffer = new byte[0x10_000];
                        private static byte[] procedureArgsBuffer = new byte[0x10_000];
                        private static byte[] httpRequestBuffer = new byte[0x10_000];
                        private static byte[] httpRequestBodyBuffer = new byte[0x10_000];
                        private static byte[] viewArgsBuffer = new byte[0x10_000];
                        private static byte[] anonymousViewArgsBuffer = new byte[0x10_000];

                        {{IndentGeneratedCode(string.Join("\n", addReducers.Select(r => r.Class)), 4)}}
                        
                        {{IndentGeneratedCode(string.Join("\n", addProcedures.Select(r => r.Class)), 4)}}

                        {{IndentGeneratedCode(string.Join("\n", addHttpHandlers.Select(r => r.Class)), 4)}}

                        public static List<T> ToListOrEmpty<T>(T? value) where T : struct
                                => value is null ? new List<T>() : new List<T> { value.Value };

                        public static List<T> ToListOrEmpty<T>(T? value) where T : class
                                => value is null ? new List<T>() : new List<T> { value };

                    #if EXPERIMENTAL_WASM_AOT || NET10_0_OR_GREATER
                        // In AOT mode we're building a library.
                        // Main method won't be called automatically, so we need to export it as a preinit function.
                        [UnmanagedCallersOnly(EntryPoint = "__preinit__10_init_csharp")]
                    #else
                        // Prevent trimming of FFI exports that are invoked from C and not visible to C# trimmer.
                        [DynamicDependency(DynamicallyAccessedMemberTypes.PublicMethods, typeof(ModuleRegistration))]
                    #endif
                        public static void Main() => Initialize();

                        internal static void Initialize() {
                            #if !NET10_0_OR_GREATER
                            SpacetimeDB.Internal.Module.SetReducerContextConstructor((identity, connectionId, random, time) => new SpacetimeDB.ReducerContext(identity, connectionId, random, time));
                            SpacetimeDB.Internal.Module.SetViewContextConstructor(identity => new SpacetimeDB.ViewContext(identity, new SpacetimeDB.Internal.LocalReadOnly()));
                            SpacetimeDB.Internal.Module.SetAnonymousViewContextConstructor(() => new SpacetimeDB.AnonymousViewContext(new SpacetimeDB.Internal.LocalReadOnly()));
                            SpacetimeDB.Internal.Module.SetProcedureContextConstructor((identity, connectionId, random, time) => new SpacetimeDB.ProcedureContext(identity, connectionId, random, time));
                            SpacetimeDB.Internal.Module.SetHandlerContextConstructor((random, time) => new SpacetimeDB.HandlerContext(random, time));
                            #endif

                            #if NET10_0_OR_GREATER
                            {{IndentGeneratedCode(string.Join("\n", compositionRegistration), 8)}}
                            #else
                            Register(global::SpacetimeDB.Internal.Module.RootBuilder);
                            #endif
                        }

                        internal static void Register(
                            global::SpacetimeDB.Internal.ModuleBuilder builder)
                        {
                            {{IndentGeneratedCode(preRegistrations, 8)}}
                            var __memoryStream = new MemoryStream();
                            var __writer = new BinaryWriter(__memoryStream);

                            {{IndentGeneratedCode(string.Join(
                                "\n",
                                addReducers.Select(r =>
                                    $"builder.RegisterReducer<{EscapeIdentifier(r.Name)}>();"
                                )
                            ), 8)}}
                            {{IndentGeneratedCode(string.Join(
                                "\n",
                                addProcedures.Select(r =>
                                    $"builder.RegisterProcedure<{EscapeIdentifier(r.Name)}>();"
                                )
                            ), 8)}}
                            {{IndentGeneratedCode(string.Join(
                                "\n",
                                addHttpHandlers.Select(r =>
                                    $"builder.RegisterHttpHandler<{EscapeIdentifier(r.Name)}>();"
                                )
                            ), 8)}}

                            // IMPORTANT: The order in which we register views matters.
                            // It must correspond to the order in which we call `GenerateDispatcherClass`.
                            // See the comment on `GenerateDispatcherClass` for more explanation.
                            {{IndentGeneratedCode(string.Join("\n",
                                views.Array.Where(v => !v.IsAnonymous)
                                    .Select(v => $"builder.RegisterView<{v.Name}ViewDispatcher>();")
                                    .Concat(
                                        views.Array.Where(v => v.IsAnonymous)
                                            .Select(v => $"builder.RegisterAnonymousView<{v.Name}ViewDispatcher>();")
                                    )
                            ), 8)}}

                            {{IndentGeneratedCode(string.Join("\n",
                                views.Array.Select(v => v.GenerateViewPrimaryKeyRegistration())
                                    .OfType<string>()
                            ), 8)}}

                            {{IndentGeneratedCode(string.Join(
                                "\n",
                                tableAccessors.Select(t => $"builder.RegisterTable<{t.TableName}, global::{handlesNamespace}.TableHandles.{EscapeIdentifier(t.TableAccessorName)}>();")
                            ), 8)}}
                            {{IndentGeneratedCode((
                                httpRouters.Array.FirstOrDefault(r => r.IsValid) is { } router
                                    ? $"builder.RegisterHttpRouter({router.FullName}());"
                                    : string.Empty
                            ), 8)}}
                            {{IndentGeneratedCode(string.Join(
                                "\n",
                                rlsFilters.Select(f => $"builder.RegisterClientVisibilityFilter({f.GlobalName});")
                            ), 8)}}
                            {{IndentGeneratedCode(string.Join(
                                "\n",
                                columnDefaultValues.Select(d =>
                                    "{\n"
                                         + $"var value = new {d.BSATNTypeName}();\n"
                                         + "__memoryStream.Position = 0;\n"
                                         + "__memoryStream.SetLength(0);\n"
                                         + $"value.Write(__writer, {d.Value});\n"
                                         + "var array = __memoryStream.ToArray();\n"
                                         + $"builder.RegisterTableDefaultValue(\"{d.TableName}\", {d.ColumnId}, array);"
                                         + "\n}\n")
                            ), 8)}}
                        }

                    // Export entrypoints live in generated module code so all build modes can
                    // dispatch directly to concrete generated functions.
                    #if EXPERIMENTAL_WASM_AOT || NET10_0_OR_GREATER
                        [UnmanagedCallersOnly(EntryPoint = "__describe_module__")]
                    #endif
                        public static void __describe_module__(SpacetimeDB.Internal.BytesSink d) => SpacetimeDB.Internal.Module.__describe_module__(d);

                        {{IndentGeneratedCode(string.Join(
                            "\n\n",
                            addReducers.Select((r, i) =>
                                $$"""
                                private static SpacetimeDB.Internal.Errno __call_reducer_{{i}}(
                                    ulong sender_0,
                                    ulong sender_1,
                                    ulong sender_2,
                                    ulong sender_3,
                                    ulong conn_id_0,
                                    ulong conn_id_1,
                                    SpacetimeDB.Timestamp timestamp,
                                    SpacetimeDB.Internal.BytesSource args,
                                    SpacetimeDB.Internal.BytesSink error{{IndentGeneratedCode(instanceParameter, 4)}}
                                ) {
                                    try {
                                        var ctx = SpacetimeDB.Internal.Module.CreateReducerContext(sender_0, sender_1, sender_2, sender_3, conn_id_0, conn_id_1, timestamp{{instanceArgument}});
                                        using var stream = SpacetimeDB.Internal.Module.ConsumeBytes(args, ref reducerArgsBuffer);
                                        using var reader = new System.IO.BinaryReader(stream);
                                        {{EscapeIdentifier(r.Name)}}.Invoke(reader, ctx);
                                        SpacetimeDB.Internal.Module.EnsureNoUnreadBytes(stream, "reducer arguments");
                                        return SpacetimeDB.Internal.Errno.OK;
                                    } catch (System.Exception e) {
                                        return SpacetimeDB.Internal.Module.WriteReducerError(error, e);
                                    }
                                }
                                """
                            )
                        ), 4)}}

                        {{IndentGeneratedCode(string.Join(
                            "\n\n",
                            addProcedures.Select((p, i) =>
                                $$"""
                                private static SpacetimeDB.Internal.Errno __call_procedure_{{i}}(
                                    ulong sender_0,
                                    ulong sender_1,
                                    ulong sender_2,
                                    ulong sender_3,
                                    ulong conn_id_0,
                                    ulong conn_id_1,
                                    SpacetimeDB.Timestamp timestamp,
                                    SpacetimeDB.Internal.BytesSource args,
                                    SpacetimeDB.Internal.BytesSink result_sink{{IndentGeneratedCode(instanceParameter, 4)}}
                                ) {
                                    try {
                                        var ctx = SpacetimeDB.Internal.Module.CreateProcedureContext(sender_0, sender_1, sender_2, sender_3, conn_id_0, conn_id_1, timestamp{{instanceArgument}});
                                        using var stream = SpacetimeDB.Internal.Module.ConsumeBytes(args, ref procedureArgsBuffer);
                                        using var reader = new System.IO.BinaryReader(stream);
                                        var bytes = {{EscapeIdentifier(p.Name)}}.Invoke(reader, ctx);
                                        SpacetimeDB.Internal.Module.EnsureNoUnreadBytes(stream, "procedure arguments");
                                        SpacetimeDB.Internal.Module.WriteBytes(result_sink, bytes);
                                        return SpacetimeDB.Internal.Errno.OK;
                                    } catch (System.Exception e) {
                                        SpacetimeDB.Log.Error($"Error while invoking procedure: {e}");
                                        throw;
                                    }
                                }
                                """
                            )
                        ), 4)}}

                        {{IndentGeneratedCode(string.Join(
                            "\n\n",
                            addHttpHandlers.Select((h, i) =>
                                $$"""
                                private static SpacetimeDB.Internal.Errno __call_http_handler_{{i}}(
                                    SpacetimeDB.Timestamp timestamp,
                                    SpacetimeDB.Internal.BytesSource request,
                                    SpacetimeDB.Internal.BytesSource request_body,
                                    SpacetimeDB.Internal.BytesSink response_sink,
                                    SpacetimeDB.Internal.BytesSink response_body_sink
                                ) {
                                    try {
                                        var ctx = SpacetimeDB.Internal.Module.CreateHandlerContext(timestamp);
                                        var response = {{EscapeIdentifier(h.Name)}}.Invoke(
                                            ctx,
                                            SpacetimeDB.Internal.Module.ReadHttpRequest(request, ref httpRequestBuffer, request_body, ref httpRequestBodyBuffer)
                                        );
                                        SpacetimeDB.Internal.Module.WriteHttpResponse(response_sink, response_body_sink, response);
                                        return SpacetimeDB.Internal.Errno.OK;
                                    } catch (System.Exception e) {
                                        SpacetimeDB.Log.Error($"Error while invoking HTTP handler: {e}");
                                        throw;
                                    }
                                }
                                """
                            )
                        ), 4)}}

                        {{IndentGeneratedCode(string.Join(
                            "\n\n",
                            views.Array.Where(v => !v.IsAnonymous).Select((v, i) =>
                                $$"""
                                private static SpacetimeDB.Internal.Errno __call_view_{{i}}(
                                    ulong sender_0,
                                    ulong sender_1,
                                    ulong sender_2,
                                    ulong sender_3,
                                    SpacetimeDB.Internal.BytesSource args,
                                    SpacetimeDB.Internal.BytesSink sink{{IndentGeneratedCode(instanceParameter, 4)}}
                                ) {
                                    try {
                                        var ctx = SpacetimeDB.Internal.Module.CreateViewContext(sender_0, sender_1, sender_2, sender_3{{instanceArgument}});
                                        using var stream = SpacetimeDB.Internal.Module.ConsumeBytes(args, ref viewArgsBuffer);
                                        using var reader = new System.IO.BinaryReader(stream);
                                        var bytes = {{v.Name}}ViewDispatcher.Invoke(reader, ctx);
                                        SpacetimeDB.Internal.Module.WriteBytes(sink, bytes);
                                        return (SpacetimeDB.Internal.Errno)2;
                                    } catch (System.Exception e) {
                                        SpacetimeDB.Log.Error($"Error while invoking view: {e}");
                                        return SpacetimeDB.Internal.Errno.HOST_CALL_FAILURE;
                                    }
                                }
                                """
                            )
                        ), 4)}}

                        {{IndentGeneratedCode(string.Join(
                            "\n\n",
                            views.Array.Where(v => v.IsAnonymous).Select((v, i) =>
                                $$"""
                                private static SpacetimeDB.Internal.Errno __call_view_anon_{{i}}(
                                    SpacetimeDB.Internal.BytesSource args,
                                    SpacetimeDB.Internal.BytesSink sink{{IndentGeneratedCode(instanceParameter, 4)}}
                                ) {
                                    try {
                                        var ctx = SpacetimeDB.Internal.Module.CreateAnonymousViewContext({{(sharedContexts ? "instanceId" : "")}});
                                        using var stream = SpacetimeDB.Internal.Module.ConsumeBytes(args, ref anonymousViewArgsBuffer);
                                        using var reader = new System.IO.BinaryReader(stream);
                                        var bytes = {{v.Name}}ViewDispatcher.Invoke(reader, ctx);
                                        SpacetimeDB.Internal.Module.WriteBytes(sink, bytes);
                                        return (SpacetimeDB.Internal.Errno)2;
                                    } catch (System.Exception e) {
                                        SpacetimeDB.Log.Error($"Error while invoking anonymous view: {e}");
                                        return SpacetimeDB.Internal.Errno.HOST_CALL_FAILURE;
                                    }
                                }
                                """
                            )
                        ), 4)}}

                    #if EXPERIMENTAL_WASM_AOT || NET10_0_OR_GREATER
                        [UnmanagedCallersOnly(EntryPoint = "__call_reducer__")]
                    #endif
                        public static SpacetimeDB.Internal.Errno __call_reducer__(
                            int id,
                            ulong sender_0,
                            ulong sender_1,
                            ulong sender_2,
                            ulong sender_3,
                            ulong conn_id_0,
                            ulong conn_id_1,
                            SpacetimeDB.Timestamp timestamp,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink error
                        ) {
                            #if NET10_0_OR_GREATER
                            {{IndentGeneratedCode(GenerateDispatchRouting("Reducer", "sender_0, sender_1, sender_2, sender_3, conn_id_0, conn_id_1, timestamp, args, error", "return SpacetimeDB.Internal.Module.WriteReducerError(error, new System.ArgumentOutOfRangeException(nameof(id), id, \"Unknown reducer id\"));"), 8)}}
                            #else
                            return CallLocalReducer(id, sender_0, sender_1, sender_2, sender_3, conn_id_0, conn_id_1, timestamp, args, error);
                            #endif
                        }

                        internal static SpacetimeDB.Internal.Errno CallLocalReducer(
                            int id,
                            ulong sender_0,
                            ulong sender_1,
                            ulong sender_2,
                            ulong sender_3,
                            ulong conn_id_0,
                            ulong conn_id_1,
                            SpacetimeDB.Timestamp timestamp,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink error{{IndentGeneratedCode(instanceParameter, 8)}}
                        ) => id switch {
                            {{IndentGeneratedCode(string.Join(
                                "\n",
                                addReducers.Select((r, i) =>
                                    $"{i} => __call_reducer_{i}(sender_0, sender_1, sender_2, sender_3, conn_id_0, conn_id_1, timestamp, args, error{instanceArgument}),"
                                )
                            ), 8)}}
                            _ => SpacetimeDB.Internal.Module.WriteReducerError(error, new System.ArgumentOutOfRangeException(nameof(id), id, "Unknown reducer id"))
                        };
                        
                    #if EXPERIMENTAL_WASM_AOT || NET10_0_OR_GREATER
                        [UnmanagedCallersOnly(EntryPoint = "__call_procedure__")]
                    #endif
                        public static SpacetimeDB.Internal.Errno __call_procedure__(
                            int id,
                            ulong sender_0,
                            ulong sender_1,
                            ulong sender_2,
                            ulong sender_3,
                            ulong conn_id_0,
                            ulong conn_id_1,
                            SpacetimeDB.Timestamp timestamp,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink result_sink
                        ) {
                            #if NET10_0_OR_GREATER
                            {{IndentGeneratedCode(GenerateDispatchRouting("Procedure", "sender_0, sender_1, sender_2, sender_3, conn_id_0, conn_id_1, timestamp, args, result_sink", "throw new System.ArgumentOutOfRangeException(nameof(id), id, \"Unknown procedure id\");"), 8)}}
                            #else
                            return CallLocalProcedure(id, sender_0, sender_1, sender_2, sender_3, conn_id_0, conn_id_1, timestamp, args, result_sink);
                            #endif
                        }

                        internal static SpacetimeDB.Internal.Errno CallLocalProcedure(
                            int id,
                            ulong sender_0,
                            ulong sender_1,
                            ulong sender_2,
                            ulong sender_3,
                            ulong conn_id_0,
                            ulong conn_id_1,
                            SpacetimeDB.Timestamp timestamp,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink result_sink{{IndentGeneratedCode(instanceParameter, 8)}}
                        ) => id switch {
                            {{IndentGeneratedCode(string.Join(
                                "\n",
                                addProcedures.Select((p, i) =>
                                    $"{i} => __call_procedure_{i}(sender_0, sender_1, sender_2, sender_3, conn_id_0, conn_id_1, timestamp, args, result_sink{instanceArgument}),"
                                )
                            ), 8)}}
                            _ => throw new System.ArgumentOutOfRangeException(nameof(id), id, "Unknown procedure id")
                        };

                    #if EXPERIMENTAL_WASM_AOT || NET10_0_OR_GREATER
                        [UnmanagedCallersOnly(EntryPoint = "__call_http_handler__")]
                    #endif
                        public static SpacetimeDB.Internal.Errno __call_http_handler__(
                            int id,
                            SpacetimeDB.Timestamp timestamp,
                            SpacetimeDB.Internal.BytesSource request,
                            SpacetimeDB.Internal.BytesSource request_body,
                            SpacetimeDB.Internal.BytesSink response_sink,
                            SpacetimeDB.Internal.BytesSink response_body_sink
                        ) {
                            #if NET10_0_OR_GREATER
                            {{IndentGeneratedCode(GenerateDispatchRouting("HttpHandler", "timestamp, request, request_body, response_sink, response_body_sink", "throw new System.ArgumentOutOfRangeException(nameof(id), id, \"Unknown HTTP handler id\");"), 8)}}
                            #else
                            return CallLocalHttpHandler(id, timestamp, request, request_body, response_sink, response_body_sink);
                            #endif
                        }

                        internal static SpacetimeDB.Internal.Errno CallLocalHttpHandler(
                            int id,
                            SpacetimeDB.Timestamp timestamp,
                            SpacetimeDB.Internal.BytesSource request,
                            SpacetimeDB.Internal.BytesSource request_body,
                            SpacetimeDB.Internal.BytesSink response_sink,
                            SpacetimeDB.Internal.BytesSink response_body_sink
                        ) => id switch {
                            {{IndentGeneratedCode(string.Join(
                                "\n",
                                addHttpHandlers.Select((h, i) =>
                                    $"{i} => __call_http_handler_{i}(timestamp, request, request_body, response_sink, response_body_sink),"
                                )
                            ), 8)}}
                            _ => throw new System.ArgumentOutOfRangeException(nameof(id), id, "Unknown HTTP handler id")
                        };
                        
                    #if EXPERIMENTAL_WASM_AOT || NET10_0_OR_GREATER
                        [UnmanagedCallersOnly(EntryPoint = "__call_view__")]
                    #endif
                        public static SpacetimeDB.Internal.Errno __call_view__(
                            int id,
                            ulong sender_0,
                            ulong sender_1,
                            ulong sender_2,
                            ulong sender_3,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink sink
                        ) {
                            #if NET10_0_OR_GREATER
                            {{IndentGeneratedCode(GenerateDispatchRouting("View", "sender_0, sender_1, sender_2, sender_3, args, sink", "return UnknownViewId(id);"), 8)}}
                            #else
                            return CallLocalView(id, sender_0, sender_1, sender_2, sender_3, args, sink);
                            #endif
                        }

                        internal static SpacetimeDB.Internal.Errno CallLocalView(
                            int id,
                            ulong sender_0,
                            ulong sender_1,
                            ulong sender_2,
                            ulong sender_3,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink sink{{IndentGeneratedCode(instanceParameter, 8)}}
                        ) => id switch {
                            {{IndentGeneratedCode(string.Join("\n",
                                views.Array.Where(v => !v.IsAnonymous)
                                    .Select((v, i) =>
                                        $"{i} => __call_view_{i}(sender_0, sender_1, sender_2, sender_3, args, sink{instanceArgument}),"
                                    )
                            ), 8)}}
                            _ => UnknownViewId(id)
                        };

                    #if EXPERIMENTAL_WASM_AOT || NET10_0_OR_GREATER
                        [UnmanagedCallersOnly(EntryPoint = "__call_view_anon__")]
                    #endif
                        public static SpacetimeDB.Internal.Errno __call_view_anon__(
                            int id,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink sink
                        ) {
                            #if NET10_0_OR_GREATER
                            {{IndentGeneratedCode(GenerateDispatchRouting("AnonymousView", "args, sink", "return UnknownAnonymousViewId(id);"), 8)}}
                            #else
                            return CallLocalAnonymousView(id, args, sink);
                            #endif
                        }

                        internal static SpacetimeDB.Internal.Errno CallLocalAnonymousView(
                            int id,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink sink{{IndentGeneratedCode(instanceParameter, 8)}}
                        ) => id switch {
                            {{IndentGeneratedCode(string.Join("\n",
                                views.Array.Where(v => v.IsAnonymous)
                                    .Select((v, i) =>
                                        $"{i} => __call_view_anon_{i}(args, sink{instanceArgument}),"
                                    )
                            ), 8)}}
                            _ => UnknownAnonymousViewId(id)
                        };

                        private static SpacetimeDB.Internal.Errno UnknownViewId(int id) {
                            SpacetimeDB.Log.Error($"Unknown view id: {id}");
                            return SpacetimeDB.Internal.Errno.HOST_CALL_FAILURE;
                        }

                        private static SpacetimeDB.Internal.Errno UnknownAnonymousViewId(int id) {
                            SpacetimeDB.Log.Error($"Unknown anonymous view id: {id}");
                            return SpacetimeDB.Internal.Errno.HOST_CALL_FAILURE;
                        }
                    }
                    
                    #pragma warning restore STDB_UNSTABLE
                    #pragma warning restore CS0436
                    """
                );
            }
        );
    }
}
