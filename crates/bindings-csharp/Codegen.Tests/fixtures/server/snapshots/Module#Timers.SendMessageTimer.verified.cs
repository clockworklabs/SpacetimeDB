﻿//HintName: Timers.SendMessageTimer.cs
// <auto-generated />
#nullable enable

partial class Timers
{
    [System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Auto)]
    partial struct SendMessageTimer : SpacetimeDB.Internal.ITable<SendMessageTimer>
    {
        public void ReadFields(System.IO.BinaryReader reader)
        {
            Text = BSATN.Text.Read(reader);
            ScheduledId = BSATN.ScheduledId.Read(reader);
            ScheduledAt = BSATN.ScheduledAt.Read(reader);
        }

        public void WriteFields(System.IO.BinaryWriter writer)
        {
            BSATN.Text.Write(writer, Text);
            BSATN.ScheduledId.Write(writer, ScheduledId);
            BSATN.ScheduledAt.Write(writer, ScheduledAt);
        }

        public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<SendMessageTimer>
        {
            internal static readonly SpacetimeDB.BSATN.String Text = new();
            internal static readonly SpacetimeDB.BSATN.U64 ScheduledId = new();
            internal static readonly SpacetimeDB.ScheduleAt.BSATN ScheduledAt = new();

            public SendMessageTimer Read(System.IO.BinaryReader reader) =>
                SpacetimeDB.BSATN.IStructuralReadWrite.Read<SendMessageTimer>(reader);

            public void Write(System.IO.BinaryWriter writer, SendMessageTimer value)
            {
                value.WriteFields(writer);
            }

            public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(
                SpacetimeDB.BSATN.ITypeRegistrar registrar
            ) =>
                registrar.RegisterType<SendMessageTimer>(
                    _ => new SpacetimeDB.BSATN.AlgebraicType.Product(
                        new SpacetimeDB.BSATN.AggregateElement[]
                        {
                            new(nameof(Text), Text.GetAlgebraicType(registrar)),
                            new(nameof(ScheduledId), ScheduledId.GetAlgebraicType(registrar)),
                            new(nameof(ScheduledAt), ScheduledAt.GetAlgebraicType(registrar))
                        }
                    )
                );
        }

        public ulong ScheduledId;
        public SpacetimeDB.ScheduleAt ScheduledAt;

        void SpacetimeDB.Internal.ITable<SendMessageTimer>.ReadGenFields(
            System.IO.BinaryReader reader
        )
        {
            if (ScheduledId == default)
            {
                ScheduledId = BSATN.ScheduledId.Read(reader);
            }
        }

        static SpacetimeDB.Internal.TableDesc SpacetimeDB.Internal.ITable<SendMessageTimer>.MakeTableDesc(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            new(
                new(
                    TableName: nameof(SendMessageTimer),
                    Columns:
                    [
                        new(nameof(Text), BSATN.Text.GetAlgebraicType(registrar)),
                        new(nameof(ScheduledId), BSATN.ScheduledId.GetAlgebraicType(registrar)),
                        new(nameof(ScheduledAt), BSATN.ScheduledAt.GetAlgebraicType(registrar))
                    ],
                    Indexes: [],
                    Constraints:
                    [
                        new(
                            nameof(SendMessageTimer),
                            1,
                            nameof(ScheduledId),
                            SpacetimeDB.ColumnAttrs.PrimaryKeyAuto
                        )
                    ],
                    Sequences: [],
                    // "system" | "user"
                    TableType: "user",
                    // "public" | "private"
                    TableAccess: "private",
                    Scheduled: nameof(SendScheduledMessage)
                ),
                (uint)
                    (
                        (SpacetimeDB.BSATN.AlgebraicType.Ref)new BSATN().GetAlgebraicType(registrar)
                    ).Ref_
            );

        static SpacetimeDB.Internal.Filter SpacetimeDB.Internal.ITable<SendMessageTimer>.CreateFilter() =>
            new(
                [
                    new(nameof(Text), (w, v) => BSATN.Text.Write(w, (string)v!)),
                    new(nameof(ScheduledId), (w, v) => BSATN.ScheduledId.Write(w, (ulong)v!)),
                    new(
                        nameof(ScheduledAt),
                        (w, v) => BSATN.ScheduledAt.Write(w, (SpacetimeDB.ScheduleAt)v!)
                    )
                ]
            );

        public static IEnumerable<SendMessageTimer> Iter() =>
            SpacetimeDB.Internal.ITable<SendMessageTimer>.Iter();

        public static IEnumerable<SendMessageTimer> Query(
            System.Linq.Expressions.Expression<Func<SendMessageTimer, bool>> predicate
        ) => SpacetimeDB.Internal.ITable<SendMessageTimer>.Query(predicate);

        public void Insert() => SpacetimeDB.Internal.ITable<SendMessageTimer>.Insert(this);

        public static IEnumerable<SendMessageTimer> FilterByText(string Text) =>
            SpacetimeDB.Internal.ITable<SendMessageTimer>.ColEq.Where(0, Text, BSATN.Text).Iter();

        public static IEnumerable<SendMessageTimer> FilterByScheduledId(ulong ScheduledId) =>
            SpacetimeDB
                .Internal.ITable<SendMessageTimer>.ColEq.Where(1, ScheduledId, BSATN.ScheduledId)
                .Iter();

        public static SendMessageTimer? FindByScheduledId(ulong ScheduledId) =>
            FilterByScheduledId(ScheduledId).Cast<SendMessageTimer?>().SingleOrDefault();

        public static bool DeleteByScheduledId(ulong ScheduledId) =>
            SpacetimeDB
                .Internal.ITable<SendMessageTimer>.ColEq.Where(1, ScheduledId, BSATN.ScheduledId)
                .Delete();

        public static bool UpdateByScheduledId(ulong ScheduledId, SendMessageTimer @this) =>
            SpacetimeDB
                .Internal.ITable<SendMessageTimer>.ColEq.Where(1, ScheduledId, BSATN.ScheduledId)
                .Update(@this);
    } // SendMessageTimer
} // Timers
