namespace SpacetimeDB.Codegen;

using System.Collections.Immutable;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

[Flags]
enum ColumnAttrs : byte
{
    None = 0b0000,
    Indexed = 0b0001,
    AutoInc = 0b0010,
    Unique = Indexed | 0b0100,
    PrimaryKey = Unique | 0b1000,
}

[Generator]
public class Module : IIncrementalGenerator
{
    public void Initialize(IncrementalGeneratorInitializationContext context)
    {
        var rowTypes = context.SyntaxProvider.ForAttributeWithMetadataName(
            fullyQualifiedMetadataName: "SpacetimeDB.TableAttribute",
            predicate: Utils.Always,
            transform: (c, ct) =>
                c.SemanticModel.GetDeclaredSymbol((TypeDeclarationSyntax)c.TargetNode, ct)!
        );

        var reducers = context.SyntaxProvider.ForAttributeWithMetadataName(
            fullyQualifiedMetadataName: "SpacetimeDB.ReducerAttribute",
            predicate: Utils.Always,
            transform: (c, ct) =>
            {
                var sym = c.SemanticModel.GetDeclaredSymbol(
                    (MethodDeclarationSyntax)c.TargetNode,
                    ct
                )!;
                if (!sym.ReturnsVoid)
                {
                    throw new Exception($"Reducer '{sym.Name}' must return void");
                }

                var name =
                    c.Attributes.Where(a =>
                            a.AttributeClass?.OriginalDefinition.ToString()
                            == "SpacetimeDB.ReducerAttribute"
                        )
                        .SelectMany(a => a.NamedArguments)
                        .FirstOrDefault(x => x.Key == "Name")
                        .Value.Value as string
                    ?? sym.Name;

                var ps = sym.Parameters;
                if (ps.Length == 0 || ps[0].Type.ToString() != "ReducerContext")
                {
                    throw new Exception(
                        $"First argument to a reducer must be of type ReducerContext, but got a {ps[0].Type}"
                    );
                }
                var var = ps.Length == 1 ? '_' : 'r';
                var args = ps.Skip(1).ToImmutableArray();
                return (sym, name, var, args);
            }
        );

        var source = rowTypes
            .Collect()
            .Combine(reducers.Collect())
            .WithTrackingName("SpacetimeDB.Codegen.Analyze")
            .Select(
                (xs, _) =>
                {
                    var satsSyms = new HashSet<ITypeSymbol>(SymbolEqualityComparer.Default);
                    var usedSyms = new HashSet<ITypeSymbol>(SymbolEqualityComparer.Default);
                    var rowTypes = xs.Left.Sort((a, b) => a.Name.CompareTo(b.Name));
                    var reducers = xs.Right.Sort((a, b) => a.name.CompareTo(b.name));

                    foreach (var t in rowTypes)
                    {
                        Utils.CollectType(usedSyms, t);
                    }

                    foreach (var r in reducers)
                    {
                        foreach (var a in r.args)
                        {
                            Utils.CollectType(usedSyms, a.Type);
                            satsSyms.Add(a.Type);
                        }
                    }

                    var types = Utils.AnalyzeTypes(usedSyms);
                    var tableTypes = rowTypes.Aggregate(
                        new HashSet<ISymbol>(SymbolEqualityComparer.Default),
                        (xs, x) =>
                        {
                            xs.Add(x);
                            return xs;
                        }
                    );
                    var typeSyms = usedSyms.Where(t => types[t].prim == null).ToImmutableArray();

                    var tables = rowTypes
                        .SelectMany(sym =>
                            sym.GetAttributes()
                                .Where(a =>
                                    a.AttributeClass?.ToString() == "SpacetimeDB.TableAttribute"
                                )
                                .Select(attr => (sym, attr))
                        )
                        .Select(t =>
                        {
                            var sym = t.sym;
                            var vis = t.sym.DeclaredAccessibility switch
                            {
                                Accessibility.ProtectedAndInternal
                                or Accessibility.NotApplicable
                                or Accessibility.Internal => "internal",
                                Accessibility.Public => "public",
                                _ => throw new Exception(
                                    "Table row type visibility must be public or internal."
                                ),
                            };
                            var args = t.attr.NamedArguments;
                            var name =
                                args.FirstOrDefault(x => x.Key == "Name").Value.Value as string
                                ?? t.sym.Name;
                            var isPublic = args.Any(x =>
                                x.Key == "Public" && x.Value.Value is true
                            );
                            var fields = sym.GetMembers()
                                .OfType<IFieldSymbol>()
                                .Select(
                                    (sym, idx) =>
                                    {
                                        var attrs = sym.GetAttributes()
                                            .Select(a =>
                                                a.AttributeClass?.OriginalDefinition.ToString() switch
                                                {
                                                    "SpacetimeDB.PrimaryKeyAttribute" =>
                                                        MatchColumn(
                                                            a,
                                                            name,
                                                            ColumnAttrs.PrimaryKey
                                                        ),
                                                    "SpacetimeDB.UniqueAttribute" => MatchColumn(
                                                        a,
                                                        name,
                                                        ColumnAttrs.Unique
                                                    ),
                                                    "SpacetimeDB.AutoIncAttribute" => MatchColumn(
                                                        a,
                                                        name,
                                                        ColumnAttrs.AutoInc
                                                    ),
                                                    "SpacetimeDB.IndexedAttribute" => MatchColumn(
                                                        a,
                                                        name,
                                                        ColumnAttrs.Indexed
                                                    ),
                                                    _ => ColumnAttrs.None,
                                                }
                                            )
                                            .Aggregate(ColumnAttrs.None, (xs, x) => xs | x);

                                        if (
                                            attrs.HasFlag(ColumnAttrs.AutoInc)
                                            && !types[sym.Type].isInt
                                        )
                                        {
                                            throw new Exception(
                                                $"[AutoInc] must be used with integer data types, got type {sym.Type.Name} on field {sym.Type.ToDisplayString()}."
                                            );
                                        }

                                        if (
                                            attrs.HasFlag(ColumnAttrs.Unique)
                                            && !types[sym.Type].isEq
                                        )
                                        { // PrimaryKey implies Unique
                                            throw new Exception(
                                                $"[PrimaryKey] and [Unique] must be used with equatable data types, got type {sym.Type.ToDisplayString()}."
                                            );
                                        }

                                        return (sym, idx, attrs);
                                    }
                                )
                                .ToImmutableArray();
                            return (sym, vis, name, fields, isPublic);
                        });

                    return (types, satsSyms, typeSyms, tables, tableTypes, reducers, rowTypes);
                }
            );

        context.RegisterSourceOutput(
            source,
            (c, s) =>
                c.AddSource(
                    "Registration.cs",
                    $$"""
                // <auto-generated />
                #nullable enable
                namespace SpacetimeDB;

                using System.Diagnostics.CodeAnalysis;

                #if EXPERIMENTAL_WASM_AOT
                using System.Runtime.InteropServices;
                #endif

                public sealed class ReducerContext : BaseReducerContext<Local> {}

                public readonly struct Local {
                {{string.Join("\n\n", s.tables.Select((t, idx) =>
                {
                    var tfqn = s.types[t.sym].fqn;
                    return $$"""
                    {{t.vis}} readonly ref struct {{t.name}}Handle(global::SpacetimeDB.Internal.FFI.TableId id) {
                        [global::System.Runtime.CompilerServices.MethodImpl(global::System.Runtime.CompilerServices.MethodImplOptions.AggressiveInlining)]
                        public global::SpacetimeDB.LocalTableIter<{{tfqn}}> Iter() => new(id);

                        [global::System.Runtime.CompilerServices.MethodImpl(global::System.Runtime.CompilerServices.MethodImplOptions.AggressiveInlining)]
                        public void Insert(in {{tfqn}} row) => global::SpacetimeDB.Internal.Module.Insert(id, row);
                {{string.Join("", t.fields
                            .Where(f => f.attrs.HasFlag(ColumnAttrs.Indexed))
                            .Select(f =>
                            {
                                var ffqn = s.types[f.sym.Type].fqn;
                                var bsatn = Utils.ResolveBSATN(s.types, f.sym.Type);
                                return $$"""

                         [global::System.Runtime.CompilerServices.MethodImpl(global::System.Runtime.CompilerServices.MethodImplOptions.AggressiveInlining)]
                         {{t.vis}} void UpdateBy{{f.sym.Name}}({{ffqn}} k, in {{tfqn}} v) =>
                             global::SpacetimeDB.Internal.Module.Update<{{tfqn}}, {{ffqn}}, {{bsatn}}>(id, new({{f.idx}}), k, v);

                         [global::System.Runtime.CompilerServices.MethodImpl(global::System.Runtime.CompilerServices.MethodImplOptions.AggressiveInlining)]
                         {{t.vis}} void DeleteBy{{f.sym.Name}}({{ffqn}} k) =>
                             global::SpacetimeDB.Internal.Module.Delete<{{ffqn}}, {{bsatn}}>(id, new({{f.idx}}), k);
                 """;
                            }))}}
                    }

                    [global::System.Runtime.CompilerServices.MethodImpl(global::System.Runtime.CompilerServices.MethodImplOptions.AggressiveInlining)]
                    {{t.vis}} {{t.name}}Handle {{t.name}}() => new(global::SpacetimeDB.Internal.Module.GetTableId({{idx}}, "{{t.name}}"));
                """;
                }))}}
                }

                static class ModuleRegistration {
                // Exports only work from the main assembly, so we need to generate forwarding methods.
                #if EXPERIMENTAL_WASM_AOT
                    [UnmanagedCallersOnly(EntryPoint = "__describe_module__")]
                    public static Buffer __describe_module__() => Module.__describe_module__();

                    [UnmanagedCallersOnly(EntryPoint = "__call_reducer__")]
                    public static SpacetimeDB.Internal.Buffer __call_reducer__(
                        uint id,
                        ulong sender_0,
                        ulong sender_1,
                        ulong sender_2,
                        ulong sender_3,
                        ulong address_0,
                        ulong address_1,
                        SpacetimeDB.Internal.DateTimeOffsetRepr timestamp,
                        SpacetimeDB.Internal.Buffer args
                    ) => SpacetimeDB.Internal.Module.__call_reducer__(
                        id,
                        sender_0,
                        sender_1,
                        sender_2,
                        sender_3,
                        address_0,
                        address_0,
                        timestamp,
                        args);

                    // In AOT mode we're building a library.
                    // Main method won't be called automatically, so we need to export it as a preinit function.
                    [UnmanagedCallersOnly(EntryPoint = "__preinit__10_init_csharp")]
                #else
                    // Prevent trimming of FFI exports that are invoked from C and not visible to C# trimmer.
                    [DynamicDependency(DynamicallyAccessedMemberTypes.PublicMethods, typeof(SpacetimeDB.Internal.Module))]
                #endif
                    public static void Main() {
                        // Type references
                {{string.Join("\n", s.typeSyms.Where(t => IsRef(s.types[t])).Select(t => $$"""
                        var {{s.types[t].var}}Ref = new global::SpacetimeDB.BSATN.AlgebraicType.Ref({{s.types[t].idx}});
                """))}}

                        // Type definitions
                {{string.Join("\n", s.typeSyms.Where(t => !IsRef(s.types[t]) || s.types[t].isOpt).Select(t => $$"""
                        var {{s.types[t].var}} = {{EmitType(s.types, t)}};
                """))}}

                        // Serialization state
                {{string.Join("\n", s.satsSyms.Select(t => $$"""
                        {{Utils.ResolveBSATN(s.types, t)}} _{{s.types[t].var}} = new();
                """))}}

                        // Registration
                        var ctx = new ReducerContext();
                        global::SpacetimeDB.Internal.Module.Initialize([
                            // Algebraic types
                            {{string.Join(",\n            ", s.typeSyms
                                .Where(t => IsRef(s.types[t]))
                                .Select(t => $"/* {s.types[t].var} */ " + EmitType(s.types, t)))}}
                        ], [
                            // Type aliases
                            {{string.Join(",\n            ", s.typeSyms
                                .Where(t => IsRef(s.types[t]) && !s.tableTypes.Contains(t))
                                .Select(t => @$"new(new(""{s.types[t].var}"", {s.types[t].idx}))"))}}
                        ], [
                            // Table descriptors
                {{string.Join(",\n", s.tables.Select(t => $$"""
                            new(new("{{t.name}}", [
                {{string.Join(",\n", t.fields
                    .Select(f => $$"""
                                new(new(nameof({{f.sym.ToDisplayString(Utils.fmt)}}), {{GetAlg(s.types, f.sym.Type)}}), (global::SpacetimeDB.Internal.ColumnAttrs){{(byte)f.attrs}})
                """))}}
                            ], {{Utils.Bool(t.isPublic)}}, {{"null"}}), {{t.sym.Name}}Ref)
                """))}}
                        ], [
                            // Reducer definitions
                {{string.Join(",\n", s.reducers.Select(r =>
                $$"""
                            new("{{r.name}}", [{{string.Join(", ",
                                r.args.Select(arg => @$"new(""{arg.Name}"", {GetAlg(s.types, arg.Type)})"))}}])
                """))}}
                        ], [
                            // Reducer calls
                {{string.Join(",\n", s.reducers.Select(r =>
                $$"""
                            {{r.var}} => {{r.sym.ToDisplayString(Utils.fmt)}}(ctx{{CallReducer(s.types, r.args)}})
                """))}}
                        ]);
                    }
                }
                """
                )
        );
    }

    public static string GetAlg(
        IReadOnlyDictionary<ISymbol, Utils.AnalyzedType> types,
        ITypeSymbol sym
    )
    {
        var alg = types[sym].alg;
        if (sym.NullableAnnotation == NullableAnnotation.Annotated && !sym.IsValueType)
        {
            return $"global::SpacetimeDB.BSATN.AlgebraicType.MakeOption({EmitType(types, sym, true)})";
        }
        else
        {
            return alg;
        }
    }

    static bool IsRef(Utils.AnalyzedType t) =>
        t.kind is Utils.TypeKind.Enum or Utils.TypeKind.Sum or Utils.TypeKind.Product;

    static string EmitType(
        IReadOnlyDictionary<ISymbol, Utils.AnalyzedType> types,
        ITypeSymbol sym,
        bool isOpt = false
    )
    {
        var t = types[sym];
        if (t.isOpt && !isOpt)
        {
            return $"global::SpacetimeDB.BSATN.AlgebraicType.MakeOption({EmitType(types, sym, true)})";
        }
        return t.kind switch
        {
            Utils.TypeKind.Prim => t.alg,
            Utils.TypeKind.Enum => $"global::SpacetimeDB.BSATN.AlgebraicType.MakeEnum<{t.fqn}>()",
            Utils.TypeKind.Builtin => $"global::SpacetimeDB.BSATN.AlgebraicTypes.{t.sym.Name}",
            Utils.TypeKind.Option =>
                $"global::SpacetimeDB.BSATN.AlgebraicType.MakeOption({GetAlg(types, Utils.Generic(sym, 0))})",
            Utils.TypeKind.List =>
                $"new global::SpacetimeDB.BSATN.AlgebraicType.Array({GetAlg(types, Utils.Generic(sym, 0))})",
            Utils.TypeKind.Array when sym is IArrayTypeSymbol { ElementType: var elem } =>
                elem.SpecialType == SpecialType.System_Byte
                    ? "global::SpacetimeDB.BSATN.AlgebraicTypes.U8Array"
                    : $"new global::SpacetimeDB.BSATN.AlgebraicType.Array({GetAlg(types, elem)})",
            Utils.TypeKind.Map =>
                $"new global::SpacetimeDB.BSATN.AlgebraicType.Map(new({GetAlg(types, Utils.Generic(sym, 0))}, {GetAlg(types, Utils.Generic(sym, 1))}))",
            Utils.TypeKind.Sum =>
                $"new global::SpacetimeDB.BSATN.AlgebraicType.Sum([\n                {string.Join(",\n                ", Utils.GetSumElements(sym)
                    .Select(f => @$"new(""{f.Name}"", {GetAlg(types, f.Type)})")
                )}\n            ])",
            Utils.TypeKind.Product =>
                $"new global::SpacetimeDB.BSATN.AlgebraicType.Product([\n                {string.Join(",\n                ", sym
                    .GetMembers()
                    .OfType<IFieldSymbol>()
                    .Select(f => @$"new(""{f.Name}"", {GetAlg(types, f.Type)})"))}\n            ])",
            _ => throw new InvalidOperationException($"Failed to emit type {types[sym].fqn}"),
        };
    }

    static ColumnAttrs MatchColumn(AttributeData attr, string name, ColumnAttrs column)
    {
        foreach (var a in attr.NamedArguments)
        {
            if (a.Key == "Table")
            {
                return a.Value.Value as string == name ? column : ColumnAttrs.None;
            }
        }
        return column;
    }

    static string CallReducer(
        IReadOnlyDictionary<ISymbol, Utils.AnalyzedType> types,
        ImmutableArray<IParameterSymbol> args
    )
    {
        if (args.IsEmpty)
        {
            return "";
        }

        var sb = Utils.StringBuilder();
        foreach (var arg in args)
        {
            sb.Append(", _");
            sb.Append(types[arg.Type].var);
            sb.Append(".Read(r)");
        }
        return sb.ToString();
    }
}
