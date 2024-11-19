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
        public readonly struct TestAutoIncNotInteger
            : SpacetimeDB.Internal.ITableView<TestAutoIncNotInteger, global::TestAutoIncNotInteger>
        {
            static global::TestAutoIncNotInteger SpacetimeDB.Internal.ITableView<
                TestAutoIncNotInteger,
                global::TestAutoIncNotInteger
            >.ReadGenFields(System.IO.BinaryReader reader, global::TestAutoIncNotInteger row)
            {
                if (row.AutoIncField == default)
                {
                    row.AutoIncField = global::TestAutoIncNotInteger.BSATN.AutoIncField.Read(
                        reader
                    );
                }
                if (row.IdentityField == default)
                {
                    row.IdentityField = global::TestAutoIncNotInteger.BSATN.IdentityField.Read(
                        reader
                    );
                }
                return row;
            }

            public ulong Count =>
                SpacetimeDB.Internal.ITableView<
                    TestAutoIncNotInteger,
                    global::TestAutoIncNotInteger
                >.DoCount();

            public IEnumerable<global::TestAutoIncNotInteger> Iter() =>
                SpacetimeDB.Internal.ITableView<
                    TestAutoIncNotInteger,
                    global::TestAutoIncNotInteger
                >.DoIter();

            public global::TestAutoIncNotInteger Insert(global::TestAutoIncNotInteger row) =>
                SpacetimeDB.Internal.ITableView<
                    TestAutoIncNotInteger,
                    global::TestAutoIncNotInteger
                >.DoInsert(row);

            public bool Delete(global::TestAutoIncNotInteger row) =>
                SpacetimeDB.Internal.ITableView<
                    TestAutoIncNotInteger,
                    global::TestAutoIncNotInteger
                >.DoDelete(row);

            public sealed class TestAutoIncNotIntegerUniqueIndex
                : UniqueIndex<
                    TestAutoIncNotInteger,
                    global::TestAutoIncNotInteger,
                    string,
                    SpacetimeDB.BSATN.String
                >
            {
                internal TestAutoIncNotIntegerUniqueIndex(TestAutoIncNotInteger handle)
                    : base(
                        handle,
                        "idx_TestAutoIncNotInteger_TestAutoIncNotInteger_IdentityField_unique"
                    ) { }

                // Important: don't move this to the base class.
                // C# generics don't play well with nullable types and can't accept both struct-type-based and class-type-based
                // `globalName` in one generic definition, leading to buggy `Row?` expansion for either one or another.
                public global::TestAutoIncNotInteger? Find(string key) =>
                    DoFilter(key).Cast<global::TestAutoIncNotInteger?>().SingleOrDefault();

                public bool Update(global::TestAutoIncNotInteger row) =>
                    DoUpdate(row.IdentityField, row);
            }

            public TestAutoIncNotIntegerUniqueIndex IdentityField => new(this);
        }

        public readonly struct TestDuplicateTableName
            : SpacetimeDB.Internal.ITableView<
                TestDuplicateTableName,
                global::TestDuplicateTableName
            >
        {
            static global::TestDuplicateTableName SpacetimeDB.Internal.ITableView<
                TestDuplicateTableName,
                global::TestDuplicateTableName
            >.ReadGenFields(System.IO.BinaryReader reader, global::TestDuplicateTableName row)
            {
                return row;
            }

            public ulong Count =>
                SpacetimeDB.Internal.ITableView<
                    TestDuplicateTableName,
                    global::TestDuplicateTableName
                >.DoCount();

            public IEnumerable<global::TestDuplicateTableName> Iter() =>
                SpacetimeDB.Internal.ITableView<
                    TestDuplicateTableName,
                    global::TestDuplicateTableName
                >.DoIter();

            public global::TestDuplicateTableName Insert(global::TestDuplicateTableName row) =>
                SpacetimeDB.Internal.ITableView<
                    TestDuplicateTableName,
                    global::TestDuplicateTableName
                >.DoInsert(row);

            public bool Delete(global::TestDuplicateTableName row) =>
                SpacetimeDB.Internal.ITableView<
                    TestDuplicateTableName,
                    global::TestDuplicateTableName
                >.DoDelete(row);
        }

        public readonly struct TestIncompatibleSchedule1
            : SpacetimeDB.Internal.ITableView<
                TestIncompatibleSchedule1,
                global::TestIncompatibleSchedule
            >
        {
            static global::TestIncompatibleSchedule SpacetimeDB.Internal.ITableView<
                TestIncompatibleSchedule1,
                global::TestIncompatibleSchedule
            >.ReadGenFields(System.IO.BinaryReader reader, global::TestIncompatibleSchedule row)
            {
                if (row.ScheduledId == default)
                {
                    row.ScheduledId = global::TestIncompatibleSchedule.BSATN.ScheduledId.Read(
                        reader
                    );
                }
                return row;
            }

            public ulong Count =>
                SpacetimeDB.Internal.ITableView<
                    TestIncompatibleSchedule1,
                    global::TestIncompatibleSchedule
                >.DoCount();

            public IEnumerable<global::TestIncompatibleSchedule> Iter() =>
                SpacetimeDB.Internal.ITableView<
                    TestIncompatibleSchedule1,
                    global::TestIncompatibleSchedule
                >.DoIter();

            public global::TestIncompatibleSchedule Insert(global::TestIncompatibleSchedule row) =>
                SpacetimeDB.Internal.ITableView<
                    TestIncompatibleSchedule1,
                    global::TestIncompatibleSchedule
                >.DoInsert(row);

            public bool Delete(global::TestIncompatibleSchedule row) =>
                SpacetimeDB.Internal.ITableView<
                    TestIncompatibleSchedule1,
                    global::TestIncompatibleSchedule
                >.DoDelete(row);

            public sealed class TestIncompatibleSchedule1UniqueIndex
                : UniqueIndex<
                    TestIncompatibleSchedule1,
                    global::TestIncompatibleSchedule,
                    ulong,
                    SpacetimeDB.BSATN.U64
                >
            {
                internal TestIncompatibleSchedule1UniqueIndex(TestIncompatibleSchedule1 handle)
                    : base(
                        handle,
                        "idx_TestIncompatibleSchedule1_TestIncompatibleSchedule1_ScheduledId_unique"
                    ) { }

                // Important: don't move this to the base class.
                // C# generics don't play well with nullable types and can't accept both struct-type-based and class-type-based
                // `globalName` in one generic definition, leading to buggy `Row?` expansion for either one or another.
                public global::TestIncompatibleSchedule? Find(ulong key) =>
                    DoFilter(key).Cast<global::TestIncompatibleSchedule?>().SingleOrDefault();

                public bool Update(global::TestIncompatibleSchedule row) =>
                    DoUpdate(row.ScheduledId, row);
            }

            public TestIncompatibleSchedule1UniqueIndex ScheduledId => new(this);
        }

        public readonly struct TestIncompatibleSchedule2
            : SpacetimeDB.Internal.ITableView<
                TestIncompatibleSchedule2,
                global::TestIncompatibleSchedule
            >
        {
            static global::TestIncompatibleSchedule SpacetimeDB.Internal.ITableView<
                TestIncompatibleSchedule2,
                global::TestIncompatibleSchedule
            >.ReadGenFields(System.IO.BinaryReader reader, global::TestIncompatibleSchedule row)
            {
                if (row.ScheduledId == default)
                {
                    row.ScheduledId = global::TestIncompatibleSchedule.BSATN.ScheduledId.Read(
                        reader
                    );
                }
                return row;
            }

            public ulong Count =>
                SpacetimeDB.Internal.ITableView<
                    TestIncompatibleSchedule2,
                    global::TestIncompatibleSchedule
                >.DoCount();

            public IEnumerable<global::TestIncompatibleSchedule> Iter() =>
                SpacetimeDB.Internal.ITableView<
                    TestIncompatibleSchedule2,
                    global::TestIncompatibleSchedule
                >.DoIter();

            public global::TestIncompatibleSchedule Insert(global::TestIncompatibleSchedule row) =>
                SpacetimeDB.Internal.ITableView<
                    TestIncompatibleSchedule2,
                    global::TestIncompatibleSchedule
                >.DoInsert(row);

            public bool Delete(global::TestIncompatibleSchedule row) =>
                SpacetimeDB.Internal.ITableView<
                    TestIncompatibleSchedule2,
                    global::TestIncompatibleSchedule
                >.DoDelete(row);

            public sealed class TestIncompatibleSchedule2UniqueIndex
                : UniqueIndex<
                    TestIncompatibleSchedule2,
                    global::TestIncompatibleSchedule,
                    ulong,
                    SpacetimeDB.BSATN.U64
                >
            {
                internal TestIncompatibleSchedule2UniqueIndex(TestIncompatibleSchedule2 handle)
                    : base(
                        handle,
                        "idx_TestIncompatibleSchedule2_TestIncompatibleSchedule2_ScheduledId_unique"
                    ) { }

                // Important: don't move this to the base class.
                // C# generics don't play well with nullable types and can't accept both struct-type-based and class-type-based
                // `globalName` in one generic definition, leading to buggy `Row?` expansion for either one or another.
                public global::TestIncompatibleSchedule? Find(ulong key) =>
                    DoFilter(key).Cast<global::TestIncompatibleSchedule?>().SingleOrDefault();

                public bool Update(global::TestIncompatibleSchedule row) =>
                    DoUpdate(row.ScheduledId, row);
            }

            public TestIncompatibleSchedule2UniqueIndex ScheduledId => new(this);
        }

        public readonly struct TestTableTaggedEnum
            : SpacetimeDB.Internal.ITableView<TestTableTaggedEnum, global::TestTableTaggedEnum>
        {
            static global::TestTableTaggedEnum SpacetimeDB.Internal.ITableView<
                TestTableTaggedEnum,
                global::TestTableTaggedEnum
            >.ReadGenFields(System.IO.BinaryReader reader, global::TestTableTaggedEnum row)
            {
                return row;
            }

            public ulong Count =>
                SpacetimeDB.Internal.ITableView<
                    TestTableTaggedEnum,
                    global::TestTableTaggedEnum
                >.DoCount();

            public IEnumerable<global::TestTableTaggedEnum> Iter() =>
                SpacetimeDB.Internal.ITableView<
                    TestTableTaggedEnum,
                    global::TestTableTaggedEnum
                >.DoIter();

            public global::TestTableTaggedEnum Insert(global::TestTableTaggedEnum row) =>
                SpacetimeDB.Internal.ITableView<
                    TestTableTaggedEnum,
                    global::TestTableTaggedEnum
                >.DoInsert(row);

            public bool Delete(global::TestTableTaggedEnum row) =>
                SpacetimeDB.Internal.ITableView<
                    TestTableTaggedEnum,
                    global::TestTableTaggedEnum
                >.DoDelete(row);
        }

        public readonly struct TestUniqueNotEquatable
            : SpacetimeDB.Internal.ITableView<
                TestUniqueNotEquatable,
                global::TestUniqueNotEquatable
            >
        {
            static global::TestUniqueNotEquatable SpacetimeDB.Internal.ITableView<
                TestUniqueNotEquatable,
                global::TestUniqueNotEquatable
            >.ReadGenFields(System.IO.BinaryReader reader, global::TestUniqueNotEquatable row)
            {
                return row;
            }

            public ulong Count =>
                SpacetimeDB.Internal.ITableView<
                    TestUniqueNotEquatable,
                    global::TestUniqueNotEquatable
                >.DoCount();

            public IEnumerable<global::TestUniqueNotEquatable> Iter() =>
                SpacetimeDB.Internal.ITableView<
                    TestUniqueNotEquatable,
                    global::TestUniqueNotEquatable
                >.DoIter();

            public global::TestUniqueNotEquatable Insert(global::TestUniqueNotEquatable row) =>
                SpacetimeDB.Internal.ITableView<
                    TestUniqueNotEquatable,
                    global::TestUniqueNotEquatable
                >.DoInsert(row);

            public bool Delete(global::TestUniqueNotEquatable row) =>
                SpacetimeDB.Internal.ITableView<
                    TestUniqueNotEquatable,
                    global::TestUniqueNotEquatable
                >.DoDelete(row);

            public sealed class TestUniqueNotEquatableUniqueIndex
                : UniqueIndex<
                    TestUniqueNotEquatable,
                    global::TestUniqueNotEquatable,
                    int?,
                    SpacetimeDB.BSATN.ValueOption<int, SpacetimeDB.BSATN.I32>
                >
            {
                internal TestUniqueNotEquatableUniqueIndex(TestUniqueNotEquatable handle)
                    : base(
                        handle,
                        "idx_TestUniqueNotEquatable_TestUniqueNotEquatable_UniqueField_unique"
                    ) { }

                // Important: don't move this to the base class.
                // C# generics don't play well with nullable types and can't accept both struct-type-based and class-type-based
                // `globalName` in one generic definition, leading to buggy `Row?` expansion for either one or another.
                public global::TestUniqueNotEquatable? Find(int? key) =>
                    DoFilter(key).Cast<global::TestUniqueNotEquatable?>().SingleOrDefault();

                public bool Update(global::TestUniqueNotEquatable row) =>
                    DoUpdate(row.UniqueField, row);
            }

            public TestUniqueNotEquatableUniqueIndex UniqueField => new(this);

            public sealed class TestUniqueNotEquatableUniqueIndex
                : UniqueIndex<
                    TestUniqueNotEquatable,
                    global::TestUniqueNotEquatable,
                    TestEnumWithExplicitValues,
                    SpacetimeDB.BSATN.Enum<TestEnumWithExplicitValues>
                >
            {
                internal TestUniqueNotEquatableUniqueIndex(TestUniqueNotEquatable handle)
                    : base(
                        handle,
                        "idx_TestUniqueNotEquatable_TestUniqueNotEquatable_PrimaryKeyField_unique"
                    ) { }

                // Important: don't move this to the base class.
                // C# generics don't play well with nullable types and can't accept both struct-type-based and class-type-based
                // `globalName` in one generic definition, leading to buggy `Row?` expansion for either one or another.
                public global::TestUniqueNotEquatable? Find(TestEnumWithExplicitValues key) =>
                    DoFilter(key).Cast<global::TestUniqueNotEquatable?>().SingleOrDefault();

                public bool Update(global::TestUniqueNotEquatable row) =>
                    DoUpdate(row.PrimaryKeyField, row);
            }

            public TestUniqueNotEquatableUniqueIndex PrimaryKeyField => new(this);
        }
    }

    public sealed class Local
    {
        public Internal.TableHandles.TestAutoIncNotInteger TestAutoIncNotInteger => new();
        public Internal.TableHandles.TestDuplicateTableName TestDuplicateTableName => new();
        public Internal.TableHandles.TestIncompatibleSchedule1 TestIncompatibleSchedule1 => new();
        public Internal.TableHandles.TestIncompatibleSchedule2 TestIncompatibleSchedule2 => new();
        public Internal.TableHandles.TestTableTaggedEnum TestTableTaggedEnum => new();
        public Internal.TableHandles.TestUniqueNotEquatable TestUniqueNotEquatable => new();
    }
}

static class ModuleRegistration
{
    class TestDuplicateReducerKind1 : SpacetimeDB.Internal.IReducer
    {
        public SpacetimeDB.Internal.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => new("__init__", []);

        public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx)
        {
            Reducers.TestDuplicateReducerKind1((SpacetimeDB.ReducerContext)ctx);
        }
    }

    class __ReducerWithReservedPrefix : SpacetimeDB.Internal.IReducer
    {
        public SpacetimeDB.Internal.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => new("__ReducerWithReservedPrefix", []);

        public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx)
        {
            Reducers.__ReducerWithReservedPrefix((SpacetimeDB.ReducerContext)ctx);
        }
    }

    class OnReducerWithReservedPrefix : SpacetimeDB.Internal.IReducer
    {
        public SpacetimeDB.Internal.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => new("OnReducerWithReservedPrefix", []);

        public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx)
        {
            Reducers.OnReducerWithReservedPrefix((SpacetimeDB.ReducerContext)ctx);
        }
    }

    class TestDuplicateReducerName : SpacetimeDB.Internal.IReducer
    {
        public SpacetimeDB.Internal.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => new("TestDuplicateReducerName", []);

        public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx)
        {
            Reducers.TestDuplicateReducerName((SpacetimeDB.ReducerContext)ctx);
        }
    }

    class TestIncompatibleScheduleReducer : SpacetimeDB.Internal.IReducer
    {
        private static readonly TestIncompatibleSchedule.BSATN table = new();

        public SpacetimeDB.Internal.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            new(
                "TestIncompatibleScheduleReducer",
                [new(nameof(table), table.GetAlgebraicType(registrar))]
            );

        public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx)
        {
            TestIncompatibleSchedule.TestIncompatibleScheduleReducer(
                (SpacetimeDB.ReducerContext)ctx,
                table.Read(reader)
            );
        }
    }

    class TestReducerReturnType : SpacetimeDB.Internal.IReducer
    {
        public SpacetimeDB.Internal.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => new("TestReducerReturnType", []);

        public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx)
        {
            Reducers.TestReducerReturnType((SpacetimeDB.ReducerContext)ctx);
        }
    }

    class TestReducerWithoutContext : SpacetimeDB.Internal.IReducer
    {
        public SpacetimeDB.Internal.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => new("TestReducerWithoutContext", []);

        public void Invoke(BinaryReader reader, SpacetimeDB.Internal.IReducerContext ctx)
        {
            Reducers.TestReducerWithoutContext((SpacetimeDB.ReducerContext)ctx);
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

        SpacetimeDB.Internal.Module.RegisterReducer<TestDuplicateReducerKind1>();
        SpacetimeDB.Internal.Module.RegisterReducer<__ReducerWithReservedPrefix>();
        SpacetimeDB.Internal.Module.RegisterReducer<OnReducerWithReservedPrefix>();
        SpacetimeDB.Internal.Module.RegisterReducer<TestDuplicateReducerName>();
        SpacetimeDB.Internal.Module.RegisterReducer<TestIncompatibleScheduleReducer>();
        SpacetimeDB.Internal.Module.RegisterReducer<TestReducerReturnType>();
        SpacetimeDB.Internal.Module.RegisterReducer<TestReducerWithoutContext>();
        SpacetimeDB.Internal.Module.RegisterTable<global::TestAutoIncNotInteger>();
        SpacetimeDB.Internal.Module.RegisterTable<global::TestDuplicateTableName>();
        SpacetimeDB.Internal.Module.RegisterTable<global::TestIncompatibleSchedule>();
        SpacetimeDB.Internal.Module.RegisterTable<global::TestTableTaggedEnum>();
        SpacetimeDB.Internal.Module.RegisterTable<global::TestUniqueNotEquatable>();
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
