﻿//HintName: FFI.cs
// <auto-generated />
#nullable enable

using System.Diagnostics.CodeAnalysis;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace SpacetimeDB
{
    public sealed record ReducerContext : DbContext<Local>, Internal.IReducerContext
    {
        public readonly Identity CallerIdentity;
        public readonly Address? CallerAddress;
        public readonly Random Rng;
        public readonly DateTimeOffset Timestamp;

        internal ReducerContext(
            Identity identity,
            Address? address,
            Random random,
            DateTimeOffset time
        )
        {
            CallerIdentity = identity;
            CallerAddress = address;
            Rng = random;
            Timestamp = time;
        }
    }

    namespace Internal.TableHandles
    {
        internal readonly struct BTreeMultiColumn
            : SpacetimeDB.Internal.ITableView<BTreeMultiColumn, global::BTreeMultiColumn>
        {
            static global::BTreeMultiColumn SpacetimeDB.Internal.ITableView<
                BTreeMultiColumn,
                global::BTreeMultiColumn
            >.ReadGenFields(System.IO.BinaryReader reader, global::BTreeMultiColumn row)
            {
                return row;
            }

            public ulong Count =>
                SpacetimeDB.Internal.ITableView<
                    BTreeMultiColumn,
                    global::BTreeMultiColumn
                >.DoCount();

            public IEnumerable<global::BTreeMultiColumn> Iter() =>
                SpacetimeDB.Internal.ITableView<
                    BTreeMultiColumn,
                    global::BTreeMultiColumn
                >.DoIter();

            public global::BTreeMultiColumn Insert(global::BTreeMultiColumn row) =>
                SpacetimeDB.Internal.ITableView<
                    BTreeMultiColumn,
                    global::BTreeMultiColumn
                >.DoInsert(row);

            public bool Delete(global::BTreeMultiColumn row) =>
                SpacetimeDB.Internal.ITableView<
                    BTreeMultiColumn,
                    global::BTreeMultiColumn
                >.DoDelete(row);

            internal sealed class LocationIndex()
                : SpacetimeDB.Internal.IndexBase<global::BTreeMultiColumn>(
                    "BTreeMultiColumn_X_Y_Z_idx_btree"
                )
            {
                public IEnumerable<global::BTreeMultiColumn> Filter(uint X) =>
                    DoFilter(
                        new SpacetimeDB.Internal.BTreeIndexBounds<uint, SpacetimeDB.BSATN.U32>(X)
                    );

                public ulong Delete(uint X) =>
                    DoDelete(
                        new SpacetimeDB.Internal.BTreeIndexBounds<uint, SpacetimeDB.BSATN.U32>(X)
                    );

                public IEnumerable<global::BTreeMultiColumn> Filter(Bound<uint> X) =>
                    DoFilter(
                        new SpacetimeDB.Internal.BTreeIndexBounds<uint, SpacetimeDB.BSATN.U32>(X)
                    );

                public ulong Delete(Bound<uint> X) =>
                    DoDelete(
                        new SpacetimeDB.Internal.BTreeIndexBounds<uint, SpacetimeDB.BSATN.U32>(X)
                    );

                public IEnumerable<global::BTreeMultiColumn> Filter((uint X, uint Y) f) =>
                    DoFilter(
                        new SpacetimeDB.Internal.BTreeIndexBounds<
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32
                        >(f)
                    );

                public ulong Delete((uint X, uint Y) f) =>
                    DoDelete(
                        new SpacetimeDB.Internal.BTreeIndexBounds<
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32
                        >(f)
                    );

                public IEnumerable<global::BTreeMultiColumn> Filter((uint X, Bound<uint> Y) f) =>
                    DoFilter(
                        new SpacetimeDB.Internal.BTreeIndexBounds<
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32
                        >(f)
                    );

                public ulong Delete((uint X, Bound<uint> Y) f) =>
                    DoDelete(
                        new SpacetimeDB.Internal.BTreeIndexBounds<
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32
                        >(f)
                    );

                public IEnumerable<global::BTreeMultiColumn> Filter((uint X, uint Y, uint Z) f) =>
                    DoFilter(
                        new SpacetimeDB.Internal.BTreeIndexBounds<
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32
                        >(f)
                    );

                public ulong Delete((uint X, uint Y, uint Z) f) =>
                    DoDelete(
                        new SpacetimeDB.Internal.BTreeIndexBounds<
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32
                        >(f)
                    );

                public IEnumerable<global::BTreeMultiColumn> Filter(
                    (uint X, uint Y, Bound<uint> Z) f
                ) =>
                    DoFilter(
                        new SpacetimeDB.Internal.BTreeIndexBounds<
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32
                        >(f)
                    );

                public ulong Delete((uint X, uint Y, Bound<uint> Z) f) =>
                    DoDelete(
                        new SpacetimeDB.Internal.BTreeIndexBounds<
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32
                        >(f)
                    );
            }

            internal LocationIndex Location => new();
        }

        internal readonly struct BTreeViews
            : SpacetimeDB.Internal.ITableView<BTreeViews, global::BTreeViews>
        {
            static global::BTreeViews SpacetimeDB.Internal.ITableView<
                BTreeViews,
                global::BTreeViews
            >.ReadGenFields(System.IO.BinaryReader reader, global::BTreeViews row)
            {
                return row;
            }

            public ulong Count =>
                SpacetimeDB.Internal.ITableView<BTreeViews, global::BTreeViews>.DoCount();

            public IEnumerable<global::BTreeViews> Iter() =>
                SpacetimeDB.Internal.ITableView<BTreeViews, global::BTreeViews>.DoIter();

            public global::BTreeViews Insert(global::BTreeViews row) =>
                SpacetimeDB.Internal.ITableView<BTreeViews, global::BTreeViews>.DoInsert(row);

            public bool Delete(global::BTreeViews row) =>
                SpacetimeDB.Internal.ITableView<BTreeViews, global::BTreeViews>.DoDelete(row);

            internal sealed class BTreeViewsUniqueIndex
                : UniqueIndex<
                    BTreeViews,
                    global::BTreeViews,
                    SpacetimeDB.Identity,
                    SpacetimeDB.Identity.BSATN
                >
            {
                internal BTreeViewsUniqueIndex(BTreeViews handle)
                    : base(handle, "BTreeViews_Id_idx_btree") { }

                // Important: don't move this to the base class.
                // C# generics don't play well with nullable types and can't accept both struct-type-based and class-type-based
                // `globalName` in one generic definition, leading to buggy `Row?` expansion for either one or another.
                public global::BTreeViews? Find(SpacetimeDB.Identity key) =>
                    DoFilter(key).Cast<global::BTreeViews?>().SingleOrDefault();

                public bool Update(global::BTreeViews row) => DoUpdate(row.Id, row);
            }

            internal BTreeViewsUniqueIndex Id => new(this);

            internal sealed class LocationIndex()
                : SpacetimeDB.Internal.IndexBase<global::BTreeViews>("BTreeViews_X_Y_idx_btree")
            {
                public IEnumerable<global::BTreeViews> Filter(uint X) =>
                    DoFilter(
                        new SpacetimeDB.Internal.BTreeIndexBounds<uint, SpacetimeDB.BSATN.U32>(X)
                    );

                public ulong Delete(uint X) =>
                    DoDelete(
                        new SpacetimeDB.Internal.BTreeIndexBounds<uint, SpacetimeDB.BSATN.U32>(X)
                    );

                public IEnumerable<global::BTreeViews> Filter(Bound<uint> X) =>
                    DoFilter(
                        new SpacetimeDB.Internal.BTreeIndexBounds<uint, SpacetimeDB.BSATN.U32>(X)
                    );

                public ulong Delete(Bound<uint> X) =>
                    DoDelete(
                        new SpacetimeDB.Internal.BTreeIndexBounds<uint, SpacetimeDB.BSATN.U32>(X)
                    );

                public IEnumerable<global::BTreeViews> Filter((uint X, uint Y) f) =>
                    DoFilter(
                        new SpacetimeDB.Internal.BTreeIndexBounds<
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32
                        >(f)
                    );

                public ulong Delete((uint X, uint Y) f) =>
                    DoDelete(
                        new SpacetimeDB.Internal.BTreeIndexBounds<
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32
                        >(f)
                    );

                public IEnumerable<global::BTreeViews> Filter((uint X, Bound<uint> Y) f) =>
                    DoFilter(
                        new SpacetimeDB.Internal.BTreeIndexBounds<
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32
                        >(f)
                    );

                public ulong Delete((uint X, Bound<uint> Y) f) =>
                    DoDelete(
                        new SpacetimeDB.Internal.BTreeIndexBounds<
                            uint,
                            SpacetimeDB.BSATN.U32,
                            uint,
                            SpacetimeDB.BSATN.U32
                        >(f)
                    );
            }

            internal LocationIndex Location => new();

            internal sealed class FactionIndex()
                : SpacetimeDB.Internal.IndexBase<global::BTreeViews>("BTreeViews_Faction_idx_btree")
            {
                public IEnumerable<global::BTreeViews> Filter(string Faction) =>
                    DoFilter(
                        new SpacetimeDB.Internal.BTreeIndexBounds<string, SpacetimeDB.BSATN.String>(
                            Faction
                        )
                    );

                public ulong Delete(string Faction) =>
                    DoDelete(
                        new SpacetimeDB.Internal.BTreeIndexBounds<string, SpacetimeDB.BSATN.String>(
                            Faction
                        )
                    );

                public IEnumerable<global::BTreeViews> Filter(Bound<string> Faction) =>
                    DoFilter(
                        new SpacetimeDB.Internal.BTreeIndexBounds<string, SpacetimeDB.BSATN.String>(
                            Faction
                        )
                    );

                public ulong Delete(Bound<string> Faction) =>
                    DoDelete(
                        new SpacetimeDB.Internal.BTreeIndexBounds<string, SpacetimeDB.BSATN.String>(
                            Faction
                        )
                    );
            }

            internal FactionIndex Faction => new();
        }

        public readonly struct MultiTable1
            : SpacetimeDB.Internal.ITableView<MultiTable1, global::MultiTableRow>
        {
            static global::MultiTableRow SpacetimeDB.Internal.ITableView<
                MultiTable1,
                global::MultiTableRow
            >.ReadGenFields(System.IO.BinaryReader reader, global::MultiTableRow row)
            {
                if (row.Foo == default)
                {
                    row.Foo = global::MultiTableRow.BSATN.Foo.Read(reader);
                }
                return row;
            }

            public ulong Count =>
                SpacetimeDB.Internal.ITableView<MultiTable1, global::MultiTableRow>.DoCount();

            public IEnumerable<global::MultiTableRow> Iter() =>
                SpacetimeDB.Internal.ITableView<MultiTable1, global::MultiTableRow>.DoIter();

            public global::MultiTableRow Insert(global::MultiTableRow row) =>
                SpacetimeDB.Internal.ITableView<MultiTable1, global::MultiTableRow>.DoInsert(row);

            public bool Delete(global::MultiTableRow row) =>
                SpacetimeDB.Internal.ITableView<MultiTable1, global::MultiTableRow>.DoDelete(row);

            public sealed class MultiTable1UniqueIndex
                : UniqueIndex<MultiTable1, global::MultiTableRow, uint, SpacetimeDB.BSATN.U32>
            {
                internal MultiTable1UniqueIndex(MultiTable1 handle)
                    : base(handle, "MultiTable1_Foo_idx_btree") { }

                // Important: don't move this to the base class.
                // C# generics don't play well with nullable types and can't accept both struct-type-based and class-type-based
                // `globalName` in one generic definition, leading to buggy `Row?` expansion for either one or another.
                public global::MultiTableRow? Find(uint key) =>
                    DoFilter(key).Cast<global::MultiTableRow?>().SingleOrDefault();

                public bool Update(global::MultiTableRow row) => DoUpdate(row.Foo, row);
            }

            public MultiTable1UniqueIndex Foo => new(this);

            public sealed class NameIndex()
                : SpacetimeDB.Internal.IndexBase<global::MultiTableRow>(
                    "MultiTable1_Name_idx_btree"
                )
            {
                public IEnumerable<global::MultiTableRow> Filter(string Name) =>
                    DoFilter(
                        new SpacetimeDB.Internal.BTreeIndexBounds<string, SpacetimeDB.BSATN.String>(
                            Name
                        )
                    );

                public ulong Delete(string Name) =>
                    DoDelete(
                        new SpacetimeDB.Internal.BTreeIndexBounds<string, SpacetimeDB.BSATN.String>(
                            Name
                        )
                    );

                public IEnumerable<global::MultiTableRow> Filter(Bound<string> Name) =>
                    DoFilter(
                        new SpacetimeDB.Internal.BTreeIndexBounds<string, SpacetimeDB.BSATN.String>(
                            Name
                        )
                    );

                public ulong Delete(Bound<string> Name) =>
                    DoDelete(
                        new SpacetimeDB.Internal.BTreeIndexBounds<string, SpacetimeDB.BSATN.String>(
                            Name
                        )
                    );
            }

            public NameIndex Name => new();
        }

        public readonly struct MultiTable2
            : SpacetimeDB.Internal.ITableView<MultiTable2, global::MultiTableRow>
        {
            static global::MultiTableRow SpacetimeDB.Internal.ITableView<
                MultiTable2,
                global::MultiTableRow
            >.ReadGenFields(System.IO.BinaryReader reader, global::MultiTableRow row)
            {
                if (row.Foo == default)
                {
                    row.Foo = global::MultiTableRow.BSATN.Foo.Read(reader);
                }
                return row;
            }

            public ulong Count =>
                SpacetimeDB.Internal.ITableView<MultiTable2, global::MultiTableRow>.DoCount();

            public IEnumerable<global::MultiTableRow> Iter() =>
                SpacetimeDB.Internal.ITableView<MultiTable2, global::MultiTableRow>.DoIter();

            public global::MultiTableRow Insert(global::MultiTableRow row) =>
                SpacetimeDB.Internal.ITableView<MultiTable2, global::MultiTableRow>.DoInsert(row);

            public bool Delete(global::MultiTableRow row) =>
                SpacetimeDB.Internal.ITableView<MultiTable2, global::MultiTableRow>.DoDelete(row);

            public sealed class MultiTable2UniqueIndex
                : UniqueIndex<MultiTable2, global::MultiTableRow, uint, SpacetimeDB.BSATN.U32>
            {
                internal MultiTable2UniqueIndex(MultiTable2 handle)
                    : base(handle, "MultiTable2_Bar_idx_btree") { }

                // Important: don't move this to the base class.
                // C# generics don't play well with nullable types and can't accept both struct-type-based and class-type-based
                // `globalName` in one generic definition, leading to buggy `Row?` expansion for either one or another.
                public global::MultiTableRow? Find(uint key) =>
                    DoFilter(key).Cast<global::MultiTableRow?>().SingleOrDefault();

                public bool Update(global::MultiTableRow row) => DoUpdate(row.Bar, row);
            }

            public MultiTable2UniqueIndex Bar => new(this);
        }

        public readonly struct PrivateTable
            : SpacetimeDB.Internal.ITableView<PrivateTable, global::PrivateTable>
        {
            static global::PrivateTable SpacetimeDB.Internal.ITableView<
                PrivateTable,
                global::PrivateTable
            >.ReadGenFields(System.IO.BinaryReader reader, global::PrivateTable row)
            {
                return row;
            }

            public ulong Count =>
                SpacetimeDB.Internal.ITableView<PrivateTable, global::PrivateTable>.DoCount();

            public IEnumerable<global::PrivateTable> Iter() =>
                SpacetimeDB.Internal.ITableView<PrivateTable, global::PrivateTable>.DoIter();

            public global::PrivateTable Insert(global::PrivateTable row) =>
                SpacetimeDB.Internal.ITableView<PrivateTable, global::PrivateTable>.DoInsert(row);

            public bool Delete(global::PrivateTable row) =>
                SpacetimeDB.Internal.ITableView<PrivateTable, global::PrivateTable>.DoDelete(row);
        }

        public readonly struct PublicTable
            : SpacetimeDB.Internal.ITableView<PublicTable, global::PublicTable>
        {
            static global::PublicTable SpacetimeDB.Internal.ITableView<
                PublicTable,
                global::PublicTable
            >.ReadGenFields(System.IO.BinaryReader reader, global::PublicTable row)
            {
                if (row.Id == default)
                {
                    row.Id = global::PublicTable.BSATN.Id.Read(reader);
                }
                return row;
            }

            public ulong Count =>
                SpacetimeDB.Internal.ITableView<PublicTable, global::PublicTable>.DoCount();

            public IEnumerable<global::PublicTable> Iter() =>
                SpacetimeDB.Internal.ITableView<PublicTable, global::PublicTable>.DoIter();

            public global::PublicTable Insert(global::PublicTable row) =>
                SpacetimeDB.Internal.ITableView<PublicTable, global::PublicTable>.DoInsert(row);

            public bool Delete(global::PublicTable row) =>
                SpacetimeDB.Internal.ITableView<PublicTable, global::PublicTable>.DoDelete(row);

            public sealed class PublicTableUniqueIndex
                : UniqueIndex<PublicTable, global::PublicTable, int, SpacetimeDB.BSATN.I32>
            {
                internal PublicTableUniqueIndex(PublicTable handle)
                    : base(handle, "PublicTable_Id_idx_btree") { }

                // Important: don't move this to the base class.
                // C# generics don't play well with nullable types and can't accept both struct-type-based and class-type-based
                // `globalName` in one generic definition, leading to buggy `Row?` expansion for either one or another.
                public global::PublicTable? Find(int key) =>
                    DoFilter(key).Cast<global::PublicTable?>().SingleOrDefault();

                public bool Update(global::PublicTable row) => DoUpdate(row.Id, row);
            }

            public PublicTableUniqueIndex Id => new(this);
        }

        public readonly struct SendMessageTimer
            : SpacetimeDB.Internal.ITableView<SendMessageTimer, global::Timers.SendMessageTimer>
        {
            static global::Timers.SendMessageTimer SpacetimeDB.Internal.ITableView<
                SendMessageTimer,
                global::Timers.SendMessageTimer
            >.ReadGenFields(System.IO.BinaryReader reader, global::Timers.SendMessageTimer row)
            {
                if (row.ScheduledId == default)
                {
                    row.ScheduledId = global::Timers.SendMessageTimer.BSATN.ScheduledId.Read(
                        reader
                    );
                }
                return row;
            }

            public ulong Count =>
                SpacetimeDB.Internal.ITableView<
                    SendMessageTimer,
                    global::Timers.SendMessageTimer
                >.DoCount();

            public IEnumerable<global::Timers.SendMessageTimer> Iter() =>
                SpacetimeDB.Internal.ITableView<
                    SendMessageTimer,
                    global::Timers.SendMessageTimer
                >.DoIter();

            public global::Timers.SendMessageTimer Insert(global::Timers.SendMessageTimer row) =>
                SpacetimeDB.Internal.ITableView<
                    SendMessageTimer,
                    global::Timers.SendMessageTimer
                >.DoInsert(row);

            public bool Delete(global::Timers.SendMessageTimer row) =>
                SpacetimeDB.Internal.ITableView<
                    SendMessageTimer,
                    global::Timers.SendMessageTimer
                >.DoDelete(row);

            public sealed class SendMessageTimerUniqueIndex
                : UniqueIndex<
                    SendMessageTimer,
                    global::Timers.SendMessageTimer,
                    ulong,
                    SpacetimeDB.BSATN.U64
                >
            {
                internal SendMessageTimerUniqueIndex(SendMessageTimer handle)
                    : base(handle, "SendMessageTimer_ScheduledId_idx_btree") { }

                // Important: don't move this to the base class.
                // C# generics don't play well with nullable types and can't accept both struct-type-based and class-type-based
                // `globalName` in one generic definition, leading to buggy `Row?` expansion for either one or another.
                public global::Timers.SendMessageTimer? Find(ulong key) =>
                    DoFilter(key).Cast<global::Timers.SendMessageTimer?>().SingleOrDefault();

                public bool Update(global::Timers.SendMessageTimer row) =>
                    DoUpdate(row.ScheduledId, row);
            }

            public SendMessageTimerUniqueIndex ScheduledId => new(this);
        }
    }

    public sealed class Local
    {
        internal Internal.TableHandles.BTreeMultiColumn BTreeMultiColumn => new();
        internal Internal.TableHandles.BTreeViews BTreeViews => new();
        public Internal.TableHandles.MultiTable1 MultiTable1 => new();
        public Internal.TableHandles.MultiTable2 MultiTable2 => new();
        public Internal.TableHandles.PrivateTable PrivateTable => new();
        public Internal.TableHandles.PublicTable PublicTable => new();
        public Internal.TableHandles.SendMessageTimer SendMessageTimer => new();
    }
}

static class ModuleRegistration
{
    class Init : SpacetimeDB.Internal.IReducer
    {
        public SpacetimeDB.Internal.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => new("__init__", []);

        public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx)
        {
            Timers.Init((SpacetimeDB.ReducerContext)ctx);
        }
    }

    class InsertData : SpacetimeDB.Internal.IReducer
    {
        private static readonly PublicTable.BSATN data = new();

        public SpacetimeDB.Internal.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => new("InsertData", [new(nameof(data), data.GetAlgebraicType(registrar))]);

        public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx)
        {
            Reducers.InsertData((SpacetimeDB.ReducerContext)ctx, data.Read(reader));
        }
    }

    class InsertData2 : SpacetimeDB.Internal.IReducer
    {
        private static readonly PublicTable.BSATN data = new();

        public SpacetimeDB.Internal.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => new("InsertData2", [new(nameof(data), data.GetAlgebraicType(registrar))]);

        public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx)
        {
            Test.NestingNamespaces.AndClasses.InsertData2(
                (SpacetimeDB.ReducerContext)ctx,
                data.Read(reader)
            );
        }
    }

    class InsertMultiData : SpacetimeDB.Internal.IReducer
    {
        private static readonly MultiTableRow.BSATN data = new();

        public SpacetimeDB.Internal.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => new("InsertMultiData", [new(nameof(data), data.GetAlgebraicType(registrar))]);

        public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx)
        {
            MultiTableRow.InsertMultiData((SpacetimeDB.ReducerContext)ctx, data.Read(reader));
        }
    }

    class ScheduleImmediate : SpacetimeDB.Internal.IReducer
    {
        private static readonly PublicTable.BSATN data = new();

        public SpacetimeDB.Internal.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => new("ScheduleImmediate", [new(nameof(data), data.GetAlgebraicType(registrar))]);

        public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx)
        {
            Reducers.ScheduleImmediate((SpacetimeDB.ReducerContext)ctx, data.Read(reader));
        }
    }

    class SendScheduledMessage : SpacetimeDB.Internal.IReducer
    {
        private static readonly Timers.SendMessageTimer.BSATN arg = new();

        public SpacetimeDB.Internal.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => new("SendScheduledMessage", [new(nameof(arg), arg.GetAlgebraicType(registrar))]);

        public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx)
        {
            Timers.SendScheduledMessage((SpacetimeDB.ReducerContext)ctx, arg.Read(reader));
        }
    }

#if EXPERIMENTAL_WASM_AOT
    // In AOT mode we're building a library.
    // Main method won't be called automatically, so we need to export it as a preinit function.
    [UnmanagedCallersOnly(EntryPoint = "__preinit__10_init_csharp")]
#else
    // Prevent trimming of FFI exports that are invoked from C and not visible to C# trimmer.
    [DynamicDependency(
        DynamicallyAccessedMemberTypes.PublicMethods,
        typeof(SpacetimeDB.Internal.Module)
    )]
#endif
    public static void Main()
    {
        SpacetimeDB.Internal.Module.SetReducerContextConstructor(
            (identity, address, random, time) =>
                new SpacetimeDB.ReducerContext(identity, address, random, time)
        );

        SpacetimeDB.Internal.Module.RegisterReducer<Init>();
        SpacetimeDB.Internal.Module.RegisterReducer<InsertData>();
        SpacetimeDB.Internal.Module.RegisterReducer<InsertData2>();
        SpacetimeDB.Internal.Module.RegisterReducer<InsertMultiData>();
        SpacetimeDB.Internal.Module.RegisterReducer<ScheduleImmediate>();
        SpacetimeDB.Internal.Module.RegisterReducer<SendScheduledMessage>();
        SpacetimeDB.Internal.Module.RegisterTable<global::BTreeMultiColumn>();
        SpacetimeDB.Internal.Module.RegisterTable<global::BTreeViews>();
        SpacetimeDB.Internal.Module.RegisterTable<global::MultiTableRow>();
        SpacetimeDB.Internal.Module.RegisterTable<global::PrivateTable>();
        SpacetimeDB.Internal.Module.RegisterTable<global::PublicTable>();
        SpacetimeDB.Internal.Module.RegisterTable<global::Timers.SendMessageTimer>();
    }

    // Exports only work from the main assembly, so we need to generate forwarding methods.
#if EXPERIMENTAL_WASM_AOT
    [UnmanagedCallersOnly(EntryPoint = "__describe_module__")]
    public static void __describe_module__(SpacetimeDB.Internal.BytesSink d) =>
        SpacetimeDB.Internal.Module.__describe_module__(d);

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
    ) =>
        SpacetimeDB.Internal.Module.__call_reducer__(
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
