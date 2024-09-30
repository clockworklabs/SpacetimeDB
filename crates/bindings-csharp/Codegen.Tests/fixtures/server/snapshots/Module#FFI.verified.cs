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
        public readonly Identity Sender;
        public readonly Address? Address;
        public readonly Random Random;
        public readonly DateTimeOffset Time;

        internal ReducerContext(
            Identity sender,
            Address? address,
            Random random,
            DateTimeOffset time
        )
        {
            Sender = sender;
            Address = address;
            Random = random;
            Time = time;
        }
    }

    namespace Internal.TableHandles
    {
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

            public IEnumerable<global::MultiTableRow> Iter() =>
                SpacetimeDB.Internal.ITableView<MultiTable1, global::MultiTableRow>.Iter();

            public IEnumerable<global::MultiTableRow> Query(
                System.Linq.Expressions.Expression<Func<global::MultiTableRow, bool>> predicate
            ) =>
                SpacetimeDB.Internal.ITableView<MultiTable1, global::MultiTableRow>.Query(
                    predicate
                );

            public global::MultiTableRow Insert(global::MultiTableRow row) =>
                SpacetimeDB.Internal.ITableView<MultiTable1, global::MultiTableRow>.Insert(row);

            public IEnumerable<global::MultiTableRow> FilterByName(string Name) =>
                SpacetimeDB
                    .Internal.ITableView<MultiTable1, global::MultiTableRow>.ColEq.Where(
                        0,
                        Name,
                        global::MultiTableRow.BSATN.Name
                    )
                    .Iter();

            public IEnumerable<global::MultiTableRow> FilterByFoo(uint Foo) =>
                SpacetimeDB
                    .Internal.ITableView<MultiTable1, global::MultiTableRow>.ColEq.Where(
                        1,
                        Foo,
                        global::MultiTableRow.BSATN.Foo
                    )
                    .Iter();

            public global::MultiTableRow? FindByFoo(uint Foo) =>
                FilterByFoo(Foo).Cast<global::MultiTableRow?>().SingleOrDefault();

            public bool DeleteByFoo(uint Foo) =>
                SpacetimeDB
                    .Internal.ITableView<MultiTable1, global::MultiTableRow>.ColEq.Where(
                        1,
                        Foo,
                        global::MultiTableRow.BSATN.Foo
                    )
                    .Delete();

            public bool UpdateByFoo(uint Foo, global::MultiTableRow @this) =>
                SpacetimeDB
                    .Internal.ITableView<MultiTable1, global::MultiTableRow>.ColEq.Where(
                        1,
                        Foo,
                        global::MultiTableRow.BSATN.Foo
                    )
                    .Update(@this);

            public IEnumerable<global::MultiTableRow> FilterByBar(uint Bar) =>
                SpacetimeDB
                    .Internal.ITableView<MultiTable1, global::MultiTableRow>.ColEq.Where(
                        2,
                        Bar,
                        global::MultiTableRow.BSATN.Bar
                    )
                    .Iter();
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

            public IEnumerable<global::MultiTableRow> Iter() =>
                SpacetimeDB.Internal.ITableView<MultiTable2, global::MultiTableRow>.Iter();

            public IEnumerable<global::MultiTableRow> Query(
                System.Linq.Expressions.Expression<Func<global::MultiTableRow, bool>> predicate
            ) =>
                SpacetimeDB.Internal.ITableView<MultiTable2, global::MultiTableRow>.Query(
                    predicate
                );

            public global::MultiTableRow Insert(global::MultiTableRow row) =>
                SpacetimeDB.Internal.ITableView<MultiTable2, global::MultiTableRow>.Insert(row);

            public IEnumerable<global::MultiTableRow> FilterByName(string Name) =>
                SpacetimeDB
                    .Internal.ITableView<MultiTable2, global::MultiTableRow>.ColEq.Where(
                        0,
                        Name,
                        global::MultiTableRow.BSATN.Name
                    )
                    .Iter();

            public IEnumerable<global::MultiTableRow> FilterByFoo(uint Foo) =>
                SpacetimeDB
                    .Internal.ITableView<MultiTable2, global::MultiTableRow>.ColEq.Where(
                        1,
                        Foo,
                        global::MultiTableRow.BSATN.Foo
                    )
                    .Iter();

            public IEnumerable<global::MultiTableRow> FilterByBar(uint Bar) =>
                SpacetimeDB
                    .Internal.ITableView<MultiTable2, global::MultiTableRow>.ColEq.Where(
                        2,
                        Bar,
                        global::MultiTableRow.BSATN.Bar
                    )
                    .Iter();

            public global::MultiTableRow? FindByBar(uint Bar) =>
                FilterByBar(Bar).Cast<global::MultiTableRow?>().SingleOrDefault();

            public bool DeleteByBar(uint Bar) =>
                SpacetimeDB
                    .Internal.ITableView<MultiTable2, global::MultiTableRow>.ColEq.Where(
                        2,
                        Bar,
                        global::MultiTableRow.BSATN.Bar
                    )
                    .Delete();

            public bool UpdateByBar(uint Bar, global::MultiTableRow @this) =>
                SpacetimeDB
                    .Internal.ITableView<MultiTable2, global::MultiTableRow>.ColEq.Where(
                        2,
                        Bar,
                        global::MultiTableRow.BSATN.Bar
                    )
                    .Update(@this);
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

            public IEnumerable<global::PrivateTable> Iter() =>
                SpacetimeDB.Internal.ITableView<PrivateTable, global::PrivateTable>.Iter();

            public IEnumerable<global::PrivateTable> Query(
                System.Linq.Expressions.Expression<Func<global::PrivateTable, bool>> predicate
            ) =>
                SpacetimeDB.Internal.ITableView<PrivateTable, global::PrivateTable>.Query(
                    predicate
                );

            public global::PrivateTable Insert(global::PrivateTable row) =>
                SpacetimeDB.Internal.ITableView<PrivateTable, global::PrivateTable>.Insert(row);
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

            public IEnumerable<global::PublicTable> Iter() =>
                SpacetimeDB.Internal.ITableView<PublicTable, global::PublicTable>.Iter();

            public IEnumerable<global::PublicTable> Query(
                System.Linq.Expressions.Expression<Func<global::PublicTable, bool>> predicate
            ) => SpacetimeDB.Internal.ITableView<PublicTable, global::PublicTable>.Query(predicate);

            public global::PublicTable Insert(global::PublicTable row) =>
                SpacetimeDB.Internal.ITableView<PublicTable, global::PublicTable>.Insert(row);

            public IEnumerable<global::PublicTable> FilterById(int Id) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        0,
                        Id,
                        global::PublicTable.BSATN.Id
                    )
                    .Iter();

            public global::PublicTable? FindById(int Id) =>
                FilterById(Id).Cast<global::PublicTable?>().SingleOrDefault();

            public bool DeleteById(int Id) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        0,
                        Id,
                        global::PublicTable.BSATN.Id
                    )
                    .Delete();

            public bool UpdateById(int Id, global::PublicTable @this) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        0,
                        Id,
                        global::PublicTable.BSATN.Id
                    )
                    .Update(@this);

            public IEnumerable<global::PublicTable> FilterByByteField(byte ByteField) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        1,
                        ByteField,
                        global::PublicTable.BSATN.ByteField
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByUshortField(ushort UshortField) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        2,
                        UshortField,
                        global::PublicTable.BSATN.UshortField
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByUintField(uint UintField) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        3,
                        UintField,
                        global::PublicTable.BSATN.UintField
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByUlongField(ulong UlongField) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        4,
                        UlongField,
                        global::PublicTable.BSATN.UlongField
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByUInt128Field(
                System.UInt128 UInt128Field
            ) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        5,
                        UInt128Field,
                        global::PublicTable.BSATN.UInt128Field
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByU128Field(SpacetimeDB.U128 U128Field) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        6,
                        U128Field,
                        global::PublicTable.BSATN.U128Field
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByU256Field(SpacetimeDB.U256 U256Field) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        7,
                        U256Field,
                        global::PublicTable.BSATN.U256Field
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterBySbyteField(sbyte SbyteField) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        8,
                        SbyteField,
                        global::PublicTable.BSATN.SbyteField
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByShortField(short ShortField) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        9,
                        ShortField,
                        global::PublicTable.BSATN.ShortField
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByIntField(int IntField) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        10,
                        IntField,
                        global::PublicTable.BSATN.IntField
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByLongField(long LongField) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        11,
                        LongField,
                        global::PublicTable.BSATN.LongField
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByInt128Field(
                System.Int128 Int128Field
            ) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        12,
                        Int128Field,
                        global::PublicTable.BSATN.Int128Field
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByI128Field(SpacetimeDB.I128 I128Field) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        13,
                        I128Field,
                        global::PublicTable.BSATN.I128Field
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByI256Field(SpacetimeDB.I256 I256Field) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        14,
                        I256Field,
                        global::PublicTable.BSATN.I256Field
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByBoolField(bool BoolField) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        15,
                        BoolField,
                        global::PublicTable.BSATN.BoolField
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByStringField(string StringField) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        18,
                        StringField,
                        global::PublicTable.BSATN.StringField
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByIdentityField(
                SpacetimeDB.Identity IdentityField
            ) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        19,
                        IdentityField,
                        global::PublicTable.BSATN.IdentityField
                    )
                    .Iter();

            public IEnumerable<global::PublicTable> FilterByAddressField(
                SpacetimeDB.Address AddressField
            ) =>
                SpacetimeDB
                    .Internal.ITableView<PublicTable, global::PublicTable>.ColEq.Where(
                        20,
                        AddressField,
                        global::PublicTable.BSATN.AddressField
                    )
                    .Iter();
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

            public IEnumerable<global::Timers.SendMessageTimer> Iter() =>
                SpacetimeDB.Internal.ITableView<
                    SendMessageTimer,
                    global::Timers.SendMessageTimer
                >.Iter();

            public IEnumerable<global::Timers.SendMessageTimer> Query(
                System.Linq.Expressions.Expression<
                    Func<global::Timers.SendMessageTimer, bool>
                > predicate
            ) =>
                SpacetimeDB.Internal.ITableView<
                    SendMessageTimer,
                    global::Timers.SendMessageTimer
                >.Query(predicate);

            public global::Timers.SendMessageTimer Insert(global::Timers.SendMessageTimer row) =>
                SpacetimeDB.Internal.ITableView<
                    SendMessageTimer,
                    global::Timers.SendMessageTimer
                >.Insert(row);

            public IEnumerable<global::Timers.SendMessageTimer> FilterByText(string Text) =>
                SpacetimeDB
                    .Internal.ITableView<
                        SendMessageTimer,
                        global::Timers.SendMessageTimer
                    >.ColEq.Where(0, Text, global::Timers.SendMessageTimer.BSATN.Text)
                    .Iter();

            public IEnumerable<global::Timers.SendMessageTimer> FilterByScheduledId(
                ulong ScheduledId
            ) =>
                SpacetimeDB
                    .Internal.ITableView<
                        SendMessageTimer,
                        global::Timers.SendMessageTimer
                    >.ColEq.Where(1, ScheduledId, global::Timers.SendMessageTimer.BSATN.ScheduledId)
                    .Iter();

            public global::Timers.SendMessageTimer? FindByScheduledId(ulong ScheduledId) =>
                FilterByScheduledId(ScheduledId)
                    .Cast<global::Timers.SendMessageTimer?>()
                    .SingleOrDefault();

            public bool DeleteByScheduledId(ulong ScheduledId) =>
                SpacetimeDB
                    .Internal.ITableView<
                        SendMessageTimer,
                        global::Timers.SendMessageTimer
                    >.ColEq.Where(1, ScheduledId, global::Timers.SendMessageTimer.BSATN.ScheduledId)
                    .Delete();

            public bool UpdateByScheduledId(
                ulong ScheduledId,
                global::Timers.SendMessageTimer @this
            ) =>
                SpacetimeDB
                    .Internal.ITableView<
                        SendMessageTimer,
                        global::Timers.SendMessageTimer
                    >.ColEq.Where(1, ScheduledId, global::Timers.SendMessageTimer.BSATN.ScheduledId)
                    .Update(@this);
        }
    }

    public sealed class Local
    {
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
        ) =>
            new(
                "test_custom_name_and_reducer_ctx",
                [new(nameof(data), data.GetAlgebraicType(registrar))]
            );

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
