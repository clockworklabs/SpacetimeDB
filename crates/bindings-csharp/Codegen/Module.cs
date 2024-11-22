namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using SpacetimeDB.Internal;
using static Utils;

readonly record struct ColumnAttr(ColumnAttrs Mask, string? Table = null)
{
    private static readonly ImmutableDictionary<string, System.Type> AttrTypes = ImmutableArray
        .Create(typeof(AutoIncAttribute), typeof(PrimaryKeyAttribute), typeof(UniqueAttribute))
        .ToImmutableDictionary(t => t.FullName);

    public static ColumnAttr Parse(AttributeData attrData)
    {
        if (
            attrData.AttributeClass is not { } attrClass
            || !AttrTypes.TryGetValue(attrClass.ToString(), out var attrType)
        )
        {
            return default;
        }
        var attr = attrData.ParseAs<ColumnAttribute>(attrType);
        return new(attr.Mask, attr.Table);
    }
}

record ColumnDeclaration : MemberDeclaration
{
    public readonly EquatableArray<ColumnAttr> Attrs;
    public readonly bool IsEquatable;
    public readonly string FullTableName;

    public ColumnDeclaration(
        string tableName,
        string name,
        string type,
        string typeInfo,
        ColumnAttrs attrs,
        bool isEquatable
    )
        : base(name, type, typeInfo)
    {
        Attrs = new(ImmutableArray.Create(new ColumnAttr(attrs)));
        IsEquatable = isEquatable;
        FullTableName = tableName;
    }

    // A helper to combine multiple column attributes into a single mask.
    // Note: it doesn't check the table names, this is left up to the caller.
    private static ColumnAttrs CombineColumnAttrs(IEnumerable<ColumnAttr> attrs) =>
        attrs.Aggregate(ColumnAttrs.UnSet, (mask, attr) => mask | attr.Mask);

    public ColumnDeclaration(string tableName, IFieldSymbol field, DiagReporter diag)
        : base(field, diag)
    {
        FullTableName = tableName;

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

        IsEquatable =
            (
                isInteger
                || type.SpecialType switch
                {
                    SpecialType.System_String or SpecialType.System_Boolean => true,
                    SpecialType.None => type.ToString()
                        is "SpacetimeDB.Address"
                            or "SpacetimeDB.Identity",
                    _ => false,
                }
            )
            && type.NullableAnnotation != NullableAnnotation.Annotated;

        if (attrs.HasFlag(ColumnAttrs.Unique) && !IsEquatable)
        {
            diag.Report(ErrorDescriptor.UniqueNotEquatable, field);
        }
    }

    public ColumnAttrs GetAttrs(string tableName) =>
        CombineColumnAttrs(Attrs.Where(x => x.Table == null || x.Table == tableName));

    // For the `TableDesc` constructor.
    public string GenerateColumnDef() =>
        $"new (nameof({Name}), BSATN.{Name}.GetAlgebraicType(registrar))";
}

record TableView
{
    public readonly string Name;
    public readonly bool IsPublic;
    public readonly string? Scheduled;

    public TableView(TableDeclaration table, AttributeData data)
    {
        var attr = data.ParseAs<TableAttribute>();

        Name = attr.Name ?? table.ShortName;
        IsPublic = attr.Public;
        Scheduled = attr.Scheduled;
    }
}

abstract record ViewIndex
{
    public EquatableArray<string> Columns;
    public string? Table;
    public string Name;

    protected abstract string Type { get; }

    public ViewIndex(AttributeData data, TypeDeclarationSyntax decl, DiagReporter diag)
    {
        var attr = data.ParseAs<IndexAttribute>();
        Columns = new((attr.BTree ?? []).ToImmutableArray());
        Table = attr.Table;
        Name = attr.Name ?? string.Join("", Columns);

        if (Columns.Length == 0)
        {
            diag.Report(ErrorDescriptor.EmptyIndexColumns, decl);
        }
    }

    public string GenerateIndexDef(string viewName, IEnumerable<ColumnDeclaration> columns)
    {
        var cols = Columns.Select(c =>
            columns.Select((c, i) => (c, i)).First(cd => cd.c.Name == c)
        );
        var colIndices = cols.Select(col => col.i);
        var colNames = cols.Select(col => col.c.Name);

        // Note: when updating to v9, you could optionally just pass `null` instead of this string.
        // The same name will be auto-generated on the host.
        var standardIndexName = StandardIndexName("btree", viewName, colNames);
        return $"new(\"{standardIndexName}\", false, SpacetimeDB.Internal.IndexType.{Type}, [{string.Join(", ", colIndices)}])";
    }

    // See: bindings_sys::index_id_from_name for documentation of this format.
    // Guaranteed not to contain quotes, so does not need to be escaped when embedded in a string.
    public static string StandardIndexName(
        string type,
        string tableName,
        IEnumerable<string> columnNames
    ) => $"{tableName}_{string.Join("_", columnNames)}_idx_{type.ToLower()}";
}

record ViewBTree : ViewIndex
{
    public ViewBTree(AttributeData data, TypeDeclarationSyntax decl, DiagReporter diag)
        : base(data, decl, diag) { }

    protected override string Type => "BTree";
}

record TableDeclaration : BaseTypeDeclaration<ColumnDeclaration>
{
    public readonly Accessibility Visibility;
    public readonly EquatableArray<TableView> Views;
    public readonly EquatableArray<ViewBTree> BTrees;

    private static ColumnDeclaration[] ScheduledColumns(string tableName) =>
        [
            new(
                tableName,
                "ScheduledId",
                "ulong",
                "SpacetimeDB.BSATN.U64",
                ColumnAttrs.PrimaryKeyAuto,
                true
            ),
            new(
                tableName,
                "ScheduledAt",
                "SpacetimeDB.ScheduleAt",
                "SpacetimeDB.ScheduleAt.BSATN",
                ColumnAttrs.UnSet,
                false
            ),
        ];

    public TableDeclaration(GeneratorAttributeSyntaxContext context, DiagReporter diag)
        : base(context, diag)
    {
        if (Kind is TypeKind.Sum)
        {
            diag.Report(ErrorDescriptor.TableTaggedEnum, (TypeDeclarationSyntax)context.TargetNode);
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
                    diag.Report(
                        ErrorDescriptor.InvalidTableVisibility,
                        (TypeDeclarationSyntax)context.TargetNode
                    );
                    throw new Exception(
                        "Table row type visibility must be public or internal, including containing types."
                    );
            }

            container = container.ContainingType;
        }

        Views = new(context.Attributes.Select(a => new TableView(this, a)).ToImmutableArray());
        BTrees = new(
            context
                .TargetSymbol.GetAttributes()
                .Where(a => a.AttributeClass?.ToString() == typeof(IndexAttribute).FullName)
                .Select(a => new ViewBTree(a, (TypeDeclarationSyntax)context.TargetNode, diag))
                .ToImmutableArray()
        );

        var hasScheduled = Views.Select(t => t.Scheduled is not null).Distinct();

        if (hasScheduled.Count() != 1)
        {
            diag.Report(
                ErrorDescriptor.IncompatibleTableSchedule,
                (TypeDeclarationSyntax)context.TargetNode
            );
        }

        if (hasScheduled.Any(has => has))
        {
            // For scheduled tables, we append extra fields early in the pipeline,
            // both to the type itself and to the BSATN information, as if they
            // were part of the original declaration.
            Members = new(Members.Concat(ScheduledColumns(FullName)).ToImmutableArray());
        }
    }

    protected override ColumnDeclaration ConvertMember(IFieldSymbol field, DiagReporter diag) =>
        new(FullName, field, diag);

    public IEnumerable<string> GenerateViewFilters(string viewName)
    {
        var vis = SyntaxFacts.GetText(Visibility);
        var globalName = $"global::{FullName}";

        foreach (
            var ct in GetConstraints(viewName)
                .Where(tuple => tuple.attr.HasFlag(ColumnAttrs.Unique))
        )
        {
            var f = ct.col;
            if (f.GetAttrs(viewName).HasFlag(ColumnAttrs.Unique))
            {
                var iUniqueIndex = $"";

                var standardIndexName = ViewIndex.StandardIndexName(
                    "btree",
                    viewName,
                    [ct.col.Name]
                );
                yield return $$"""
                    {{vis}} sealed class {{viewName}}UniqueIndex : UniqueIndex<{{viewName}}, {{globalName}}, {{f.Type}}, {{f.TypeInfo}}> {
                        internal {{viewName}}UniqueIndex({{viewName}} handle) : base(handle, "{{standardIndexName}}") {}
                        // Important: don't move this to the base class.
                        // C# generics don't play well with nullable types and can't accept both struct-type-based and class-type-based
                        // `globalName` in one generic definition, leading to buggy `Row?` expansion for either one or another.
                        public {{globalName}}? Find({{f.Type}} key) => DoFilter(key).Cast<{{globalName}}?>().SingleOrDefault();
                        public bool Update({{globalName}} row) => DoUpdate(row.{{f.Name}}, row);
                    }
                    {{vis}} {{viewName}}UniqueIndex {{f.Name}} => new(this);
                    """;
            }
        }

        foreach (var btree in BTrees)
        {
            if (btree.Table != null && btree.Table != viewName)
            {
                continue;
            }

            var members = btree.Columns.Select(s => Members.First(x => x.Name == s)).ToArray();

            var standardIndexName = ViewIndex.StandardIndexName(
                "btree",
                viewName,
                members.Select(x => x.Name)
            );

            yield return $$"""
                    {{vis}} sealed class {{btree.Name}}Index() : SpacetimeDB.Internal.IndexBase<{{globalName}}>("{{standardIndexName}}") {
                """;

            for (var n = 0; n < members.Length; n++)
            {
                var types = string.Join(
                    ", ",
                    members.Take(n + 1).Select(m => $"{m.Type}, {m.TypeInfo}")
                );
                var scalars = members.Take(n).Select(m => $"{m.Type} {m.Name}");
                var lastScalar = $"{members[n].Type} {members[n].Name}";
                var lastBounds = $"Bound<{members[n].Type}> {members[n].Name}";
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

            yield return $"}}\n {vis} {btree.Name}Index {btree.Name} => new();\n";
        }
    }

    public record struct View(string viewName, string tableName, string view, string getter);

    public IEnumerable<View> GenerateViews()
    {
        foreach (var v in Views)
        {
            var autoIncFields = Members
                .Where(f => f.GetAttrs(v.Name).HasFlag(ColumnAttrs.AutoInc))
                .Select(f => f.Name);

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
                        autoIncFields.Select(name =>
                            $$"""
                            if (row.{{name}} == default)
                            {
                                row.{{name}} = {{globalName}}.BSATN.{{name}}.Read(reader);
                            }
                            """
                        )
                    )}}
                    return row;
                }

                public ulong Count => {{iTable}}.DoCount();
                public IEnumerable<{{globalName}}> Iter() => {{iTable}}.DoIter();
                public {{globalName}} Insert({{globalName}} row) => {{iTable}}.DoInsert(row);
                public bool Delete({{globalName}} row) => {{iTable}}.DoDelete(row);

                {{string.Join("\n", GenerateViewFilters(v.Name))}}
            }
            """,
                $"{SyntaxFacts.GetText(Visibility)} Internal.TableHandles.{v.Name} {v.Name} => new();"
            );
        }
    }

    public record struct Constraint(ColumnDeclaration col, int pos, ColumnAttrs attr);

    public IEnumerable<Constraint> GetConstraints(string viewName) =>
        Members
            // Important: the position must be stored here, before filtering.
            .Select((col, pos) => new Constraint(col, pos, col.GetAttrs(viewName)))
            .Where(c => c.attr != ColumnAttrs.UnSet);

    public override Scope.Extensions ToExtensions()
    {
        var extensions = base.ToExtensions();

        if (Views.Any(v => v.Scheduled is not null))
        {
            // For scheduled tables we're adding extra fields to the type source.
            extensions.Contents.Append(
                $$"""
                public ulong ScheduledId;
                public SpacetimeDB.ScheduleAt ScheduledAt;
                """
            );

            // When doing so, the compiler will warn about undefined ordering between partial declarations.
            // We don't care about ordering as we generate BSATN ourselves and don't use those structs in FFI,
            // so we can safely suppress the warning by saying "yes, we're okay with an auto/arbitrary layout".
            extensions.ExtraAttrs.Add(
                "[System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Auto)]"
            );
        }

        // If it's a tagged enum, then attempting to generate table methods will result in more noisy errors than it's worth.
        // We already reported useful diagnostic while parsing, and we generated what we could (type-level methods), so let's stop here.
        if (Kind is TypeKind.Sum)
        {
            return extensions;
        }

        var iTable = $"SpacetimeDB.Internal.ITable<{ShortName}>";

        // ITable inherits IStructuralReadWrite, so we can replace the base type instead of appending another one.
        extensions.BaseTypes.Clear();
        extensions.BaseTypes.Add(iTable);

        extensions.Contents.Append(
            $$"""
            static IEnumerable<SpacetimeDB.Internal.TableDesc> {{iTable}}.MakeTableDesc(SpacetimeDB.BSATN.ITypeRegistrar registrar) => [
            {{string.Join("\n", Views.Select(v => $$"""
            new (
                new (
                    TableName: nameof(SpacetimeDB.Local.{{v.Name}}),
                    Columns: [
                        {{string.Join(",\n", Members.Select(m => m.GenerateColumnDef()))}}
                    ],
                    Indexes: [
                        {{string.Join(",\n", BTrees
                            .Where(b => b.Table == null || b.Table == v.Name)
                            .Select(b => b.GenerateIndexDef(v.Name, Members))
                        )}}
                    ],Constraints: [
                        {{string.Join(
                            ",\n",
                            GetConstraints(v.Name)
                            .Select(ct =>
                                $$"""
                                new(
                                    "{{v.Name}}_{{ct.col.Name}}",
                                    (byte)SpacetimeDB.Internal.ColumnAttrs.{{ct.attr}},
                                    [{{ct.pos}}]
                                )
                                """
                            )
                        )}}
                    ],
                    Sequences: [],
                    // "system" | "user"
                    TableType: "user",
                    // "public" | "private"
                    TableAccess: "{{(v.IsPublic ? "public" : "private")}}",
                    Scheduled: {{(v.Scheduled is not null ? $"\"{v.Scheduled}\"" : "null")}}
                ),
                (uint) ((SpacetimeDB.BSATN.AlgebraicType.Ref) new BSATN().GetAlgebraicType(registrar)).Ref_
            ),
            """))}}
            ];
            """
        );

        return extensions;
    }
}

record ReducerDeclaration
{
    public readonly string Name;
    public readonly string ExportName;
    public readonly string FullName;
    public readonly EquatableArray<MemberDeclaration> Args;
    public readonly Scope Scope;

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

        ExportName = attr.Kind switch
        {
            ReducerKind.Init => "__init__",
            ReducerKind.ClientConnected => "__identity_connected__",
            ReducerKind.ClientDisconnected => "__identity_disconnected__",
            _ => Name,
        };
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
        var args = string.Join(
            ", ",
            Args.Select(a => $"{a.Name}.Read(reader)").Prepend("(SpacetimeDB.ReducerContext)ctx")
        );
        return $$"""
            class {{Name}}: SpacetimeDB.Internal.IReducer {
                {{MemberDeclaration.GenerateBsatnFields(Accessibility.Private, Args)}}

                public SpacetimeDB.Internal.ReducerDef MakeReducerDef(SpacetimeDB.BSATN.ITypeRegistrar registrar) => new (
                    "{{ExportName}}",
                    [{{MemberDeclaration.GenerateDefs(Args)}}]
                );

                public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx) {
                    {{FullName}}({{args}});
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
                Args.Select(a => $"{a.Type} {a.Name}")
            )}}) {
                using var stream = new MemoryStream();
                using var writer = new BinaryWriter(stream);
                {{string.Join(
                    "\n",
                    Args.Select(a => $"new {a.TypeInfo}().Write(writer, {a.Name});")
                )}}
                SpacetimeDB.Internal.IReducer.VolatileNonatomicScheduleImmediate("{{ExportName}}", stream);
            }
            """
        );

        return extensions;
    }
}

[Generator]
public class Module : IIncrementalGenerator
{
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

        var addReducers = CollectDistinct(
            "Reducer",
            context,
            reducers
                .Select((r, ct) => (r.Name, r.ExportName, r.FullName, Class: r.GenerateClass()))
                .WithTrackingName("SpacetimeDB.Reducer.GenerateClass"),
            r => r.ExportName,
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

        context.RegisterSourceOutput(
            tableViews.Combine(addReducers),
            (context, tuple) =>
            {
                var (tableViews, addReducers) = tuple;
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
                            public readonly Identity CallerIdentity;
                            public readonly Address? CallerAddress;
                            public readonly Random Rng;
                            public readonly DateTimeOffset Timestamp;

                            internal ReducerContext(Identity identity, Address? address, Random random, DateTimeOffset time) {
                                CallerIdentity = identity;
                                CallerAddress = address;
                                Rng = random;
                                Timestamp = time;
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
                            SpacetimeDB.Internal.Module.SetReducerContextConstructor((identity, address, random, time) => new SpacetimeDB.ReducerContext(identity, address, random, time));

                            {{string.Join(
                                "\n",
                                addReducers.Select(r =>
                                    $"SpacetimeDB.Internal.Module.RegisterReducer<{r.Name}>();"
                                )
                            )}}
                            {{string.Join(
                                "\n",
                                tableViews.Select(t => $"SpacetimeDB.Internal.Module.RegisterTable<{t.tableName}>();").Distinct()
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
                            ulong address_0,
                            ulong address_1,
                            SpacetimeDB.Internal.DateTimeOffsetRepr timestamp,
                            SpacetimeDB.Internal.BytesSource args,
                            SpacetimeDB.Internal.BytesSink error
                        ) => SpacetimeDB.Internal.Module.__call_reducer__(
                            id,
                            sender_0,
                            sender_1,
                            sender_2,
                            sender_3,
                            address_0,
                            address_1,
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
