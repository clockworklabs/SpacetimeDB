﻿//HintName: FFI.cs
// <auto-generated />
#nullable enable

using System.Diagnostics.CodeAnalysis;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

static class ModuleRegistration
{
    class Init : SpacetimeDB.Internal.IReducer
    {
        public SpacetimeDB.Internal.Module.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        )
        {
            return new("__init__");
        }

        public void Invoke(BinaryReader reader, SpacetimeDB.ReducerContext ctx)
        {
            Timers.Init(ctx);
        }
    }

    class InsertData : SpacetimeDB.Internal.IReducer
    {
        private static PublicTable.BSATN data = new();

        public SpacetimeDB.Internal.Module.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        )
        {
            return new(
                "InsertData",
                new SpacetimeDB.BSATN.AggregateElement(
                    nameof(data),
                    data.GetAlgebraicType(registrar)
                )
            );
        }

        public void Invoke(BinaryReader reader, SpacetimeDB.ReducerContext ctx)
        {
            Reducers.InsertData(data.Read(reader));
        }
    }

    class InsertData2 : SpacetimeDB.Internal.IReducer
    {
        private static PublicTable.BSATN data = new();

        public SpacetimeDB.Internal.Module.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        )
        {
            return new(
                "test_custom_name_and_reducer_ctx",
                new SpacetimeDB.BSATN.AggregateElement(
                    nameof(data),
                    data.GetAlgebraicType(registrar)
                )
            );
        }

        public void Invoke(BinaryReader reader, SpacetimeDB.ReducerContext ctx)
        {
            Test.NestingNamespaces.AndClasses.InsertData2(ctx, data.Read(reader));
        }
    }

    class ScheduleImmediate : SpacetimeDB.Internal.IReducer
    {
        private static PublicTable.BSATN data = new();

        public SpacetimeDB.Internal.Module.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        )
        {
            return new(
                "ScheduleImmediate",
                new SpacetimeDB.BSATN.AggregateElement(
                    nameof(data),
                    data.GetAlgebraicType(registrar)
                )
            );
        }

        public void Invoke(BinaryReader reader, SpacetimeDB.ReducerContext ctx)
        {
            Reducers.ScheduleImmediate(data.Read(reader));
        }
    }

    class SendScheduledMessage : SpacetimeDB.Internal.IReducer
    {
        private static Timers.SendMessageTimer.BSATN arg = new();

        public SpacetimeDB.Internal.Module.ReducerDef MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        )
        {
            return new(
                "SendScheduledMessage",
                new SpacetimeDB.BSATN.AggregateElement(nameof(arg), arg.GetAlgebraicType(registrar))
            );
        }

        public void Invoke(BinaryReader reader, SpacetimeDB.ReducerContext ctx)
        {
            Timers.SendScheduledMessage(arg.Read(reader));
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
        SpacetimeDB.Internal.Module.RegisterReducer<Init>();
        SpacetimeDB.Internal.Module.RegisterReducer<InsertData>();
        SpacetimeDB.Internal.Module.RegisterReducer<InsertData2>();
        SpacetimeDB.Internal.Module.RegisterReducer<ScheduleImmediate>();
        SpacetimeDB.Internal.Module.RegisterReducer<SendScheduledMessage>();
        SpacetimeDB.Internal.Module.RegisterTable<PrivateTable>();
        SpacetimeDB.Internal.Module.RegisterTable<PublicTable>();
        SpacetimeDB.Internal.Module.RegisterTable<Timers.SendMessageTimer>();
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
