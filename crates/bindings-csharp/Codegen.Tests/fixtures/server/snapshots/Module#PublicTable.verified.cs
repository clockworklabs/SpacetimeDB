﻿//HintName: PublicTable.cs
// <auto-generated />
#nullable enable

partial struct PublicTable : SpacetimeDB.Internal.ITable<PublicTable>
{
    public void ReadFields(System.IO.BinaryReader reader)
    {
        Id = BSATN.Id.Read(reader);
        ByteField = BSATN.ByteField.Read(reader);
        UshortField = BSATN.UshortField.Read(reader);
        UintField = BSATN.UintField.Read(reader);
        UlongField = BSATN.UlongField.Read(reader);
        UInt128Field = BSATN.UInt128Field.Read(reader);
        U128Field = BSATN.U128Field.Read(reader);
        U256Field = BSATN.U256Field.Read(reader);
        SbyteField = BSATN.SbyteField.Read(reader);
        ShortField = BSATN.ShortField.Read(reader);
        IntField = BSATN.IntField.Read(reader);
        LongField = BSATN.LongField.Read(reader);
        Int128Field = BSATN.Int128Field.Read(reader);
        I128Field = BSATN.I128Field.Read(reader);
        I256Field = BSATN.I256Field.Read(reader);
        BoolField = BSATN.BoolField.Read(reader);
        FloatField = BSATN.FloatField.Read(reader);
        DoubleField = BSATN.DoubleField.Read(reader);
        StringField = BSATN.StringField.Read(reader);
        IdentityField = BSATN.IdentityField.Read(reader);
        AddressField = BSATN.AddressField.Read(reader);
        CustomStructField = BSATN.CustomStructField.Read(reader);
        CustomClassField = BSATN.CustomClassField.Read(reader);
        CustomEnumField = BSATN.CustomEnumField.Read(reader);
        CustomTaggedEnumField = BSATN.CustomTaggedEnumField.Read(reader);
        ListField = BSATN.ListField.Read(reader);
        DictionaryField = BSATN.DictionaryField.Read(reader);
        NullableValueField = BSATN.NullableValueField.Read(reader);
        NullableReferenceField = BSATN.NullableReferenceField.Read(reader);
        ComplexNestedField = BSATN.ComplexNestedField.Read(reader);
    }

    public void WriteFields(System.IO.BinaryWriter writer)
    {
        BSATN.Id.Write(writer, Id);
        BSATN.ByteField.Write(writer, ByteField);
        BSATN.UshortField.Write(writer, UshortField);
        BSATN.UintField.Write(writer, UintField);
        BSATN.UlongField.Write(writer, UlongField);
        BSATN.UInt128Field.Write(writer, UInt128Field);
        BSATN.U128Field.Write(writer, U128Field);
        BSATN.U256Field.Write(writer, U256Field);
        BSATN.SbyteField.Write(writer, SbyteField);
        BSATN.ShortField.Write(writer, ShortField);
        BSATN.IntField.Write(writer, IntField);
        BSATN.LongField.Write(writer, LongField);
        BSATN.Int128Field.Write(writer, Int128Field);
        BSATN.I128Field.Write(writer, I128Field);
        BSATN.I256Field.Write(writer, I256Field);
        BSATN.BoolField.Write(writer, BoolField);
        BSATN.FloatField.Write(writer, FloatField);
        BSATN.DoubleField.Write(writer, DoubleField);
        BSATN.StringField.Write(writer, StringField);
        BSATN.IdentityField.Write(writer, IdentityField);
        BSATN.AddressField.Write(writer, AddressField);
        BSATN.CustomStructField.Write(writer, CustomStructField);
        BSATN.CustomClassField.Write(writer, CustomClassField);
        BSATN.CustomEnumField.Write(writer, CustomEnumField);
        BSATN.CustomTaggedEnumField.Write(writer, CustomTaggedEnumField);
        BSATN.ListField.Write(writer, ListField);
        BSATN.DictionaryField.Write(writer, DictionaryField);
        BSATN.NullableValueField.Write(writer, NullableValueField);
        BSATN.NullableReferenceField.Write(writer, NullableReferenceField);
        BSATN.ComplexNestedField.Write(writer, ComplexNestedField);
    }

    public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<PublicTable>
    {
        internal static readonly SpacetimeDB.BSATN.I32 Id = new();
        internal static readonly SpacetimeDB.BSATN.U8 ByteField = new();
        internal static readonly SpacetimeDB.BSATN.U16 UshortField = new();
        internal static readonly SpacetimeDB.BSATN.U32 UintField = new();
        internal static readonly SpacetimeDB.BSATN.U64 UlongField = new();
        internal static readonly SpacetimeDB.BSATN.U128 UInt128Field = new();
        internal static readonly SpacetimeDB.BSATN.U128Stdb U128Field = new();
        internal static readonly SpacetimeDB.BSATN.U256 U256Field = new();
        internal static readonly SpacetimeDB.BSATN.I8 SbyteField = new();
        internal static readonly SpacetimeDB.BSATN.I16 ShortField = new();
        internal static readonly SpacetimeDB.BSATN.I32 IntField = new();
        internal static readonly SpacetimeDB.BSATN.I64 LongField = new();
        internal static readonly SpacetimeDB.BSATN.I128 Int128Field = new();
        internal static readonly SpacetimeDB.BSATN.I128Stdb I128Field = new();
        internal static readonly SpacetimeDB.BSATN.I256 I256Field = new();
        internal static readonly SpacetimeDB.BSATN.Bool BoolField = new();
        internal static readonly SpacetimeDB.BSATN.F32 FloatField = new();
        internal static readonly SpacetimeDB.BSATN.F64 DoubleField = new();
        internal static readonly SpacetimeDB.BSATN.String StringField = new();
        internal static readonly SpacetimeDB.Identity.BSATN IdentityField = new();
        internal static readonly SpacetimeDB.Address.BSATN AddressField = new();
        internal static readonly CustomStruct.BSATN CustomStructField = new();
        internal static readonly CustomClass.BSATN CustomClassField = new();
        internal static readonly SpacetimeDB.BSATN.Enum<CustomEnum> CustomEnumField = new();
        internal static readonly CustomTaggedEnum.BSATN CustomTaggedEnumField = new();
        internal static readonly SpacetimeDB.BSATN.List<int, SpacetimeDB.BSATN.I32> ListField =
            new();
        internal static readonly SpacetimeDB.BSATN.Dictionary<
            string,
            int,
            SpacetimeDB.BSATN.String,
            SpacetimeDB.BSATN.I32
        > DictionaryField = new();
        internal static readonly SpacetimeDB.BSATN.ValueOption<
            int,
            SpacetimeDB.BSATN.I32
        > NullableValueField = new();
        internal static readonly SpacetimeDB.BSATN.RefOption<
            string,
            SpacetimeDB.BSATN.String
        > NullableReferenceField = new();
        internal static readonly SpacetimeDB.BSATN.RefOption<
            System.Collections.Generic.Dictionary<
                CustomEnum,
                System.Collections.Generic.List<int?>?
            >,
            SpacetimeDB.BSATN.Dictionary<
                CustomEnum,
                System.Collections.Generic.List<int?>?,
                SpacetimeDB.BSATN.Enum<CustomEnum>,
                SpacetimeDB.BSATN.RefOption<
                    System.Collections.Generic.List<int?>,
                    SpacetimeDB.BSATN.List<
                        int?,
                        SpacetimeDB.BSATN.ValueOption<int, SpacetimeDB.BSATN.I32>
                    >
                >
            >
        > ComplexNestedField = new();

        public PublicTable Read(System.IO.BinaryReader reader) =>
            SpacetimeDB.BSATN.IStructuralReadWrite.Read<PublicTable>(reader);

        public void Write(System.IO.BinaryWriter writer, PublicTable value)
        {
            value.WriteFields(writer);
        }

        public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            registrar.RegisterType<PublicTable>(_ => new SpacetimeDB.BSATN.AlgebraicType.Product(
                new SpacetimeDB.BSATN.AggregateElement[]
                {
                    new(nameof(Id), Id.GetAlgebraicType(registrar)),
                    new(nameof(ByteField), ByteField.GetAlgebraicType(registrar)),
                    new(nameof(UshortField), UshortField.GetAlgebraicType(registrar)),
                    new(nameof(UintField), UintField.GetAlgebraicType(registrar)),
                    new(nameof(UlongField), UlongField.GetAlgebraicType(registrar)),
                    new(nameof(UInt128Field), UInt128Field.GetAlgebraicType(registrar)),
                    new(nameof(U128Field), U128Field.GetAlgebraicType(registrar)),
                    new(nameof(U256Field), U256Field.GetAlgebraicType(registrar)),
                    new(nameof(SbyteField), SbyteField.GetAlgebraicType(registrar)),
                    new(nameof(ShortField), ShortField.GetAlgebraicType(registrar)),
                    new(nameof(IntField), IntField.GetAlgebraicType(registrar)),
                    new(nameof(LongField), LongField.GetAlgebraicType(registrar)),
                    new(nameof(Int128Field), Int128Field.GetAlgebraicType(registrar)),
                    new(nameof(I128Field), I128Field.GetAlgebraicType(registrar)),
                    new(nameof(I256Field), I256Field.GetAlgebraicType(registrar)),
                    new(nameof(BoolField), BoolField.GetAlgebraicType(registrar)),
                    new(nameof(FloatField), FloatField.GetAlgebraicType(registrar)),
                    new(nameof(DoubleField), DoubleField.GetAlgebraicType(registrar)),
                    new(nameof(StringField), StringField.GetAlgebraicType(registrar)),
                    new(nameof(IdentityField), IdentityField.GetAlgebraicType(registrar)),
                    new(nameof(AddressField), AddressField.GetAlgebraicType(registrar)),
                    new(nameof(CustomStructField), CustomStructField.GetAlgebraicType(registrar)),
                    new(nameof(CustomClassField), CustomClassField.GetAlgebraicType(registrar)),
                    new(nameof(CustomEnumField), CustomEnumField.GetAlgebraicType(registrar)),
                    new(
                        nameof(CustomTaggedEnumField),
                        CustomTaggedEnumField.GetAlgebraicType(registrar)
                    ),
                    new(nameof(ListField), ListField.GetAlgebraicType(registrar)),
                    new(nameof(DictionaryField), DictionaryField.GetAlgebraicType(registrar)),
                    new(nameof(NullableValueField), NullableValueField.GetAlgebraicType(registrar)),
                    new(
                        nameof(NullableReferenceField),
                        NullableReferenceField.GetAlgebraicType(registrar)
                    ),
                    new(nameof(ComplexNestedField), ComplexNestedField.GetAlgebraicType(registrar))
                }
            ));
    }

    static IEnumerable<SpacetimeDB.Internal.TableDesc> SpacetimeDB.Internal.ITable<PublicTable>.MakeTableDesc(
        SpacetimeDB.BSATN.ITypeRegistrar registrar
    ) =>
        [
            new(
                new(
                    TableName: nameof(PublicTable),
                    Columns:
                    [
                        new(nameof(Id), BSATN.Id.GetAlgebraicType(registrar)),
                        new(nameof(ByteField), BSATN.ByteField.GetAlgebraicType(registrar)),
                        new(nameof(UshortField), BSATN.UshortField.GetAlgebraicType(registrar)),
                        new(nameof(UintField), BSATN.UintField.GetAlgebraicType(registrar)),
                        new(nameof(UlongField), BSATN.UlongField.GetAlgebraicType(registrar)),
                        new(nameof(UInt128Field), BSATN.UInt128Field.GetAlgebraicType(registrar)),
                        new(nameof(U128Field), BSATN.U128Field.GetAlgebraicType(registrar)),
                        new(nameof(U256Field), BSATN.U256Field.GetAlgebraicType(registrar)),
                        new(nameof(SbyteField), BSATN.SbyteField.GetAlgebraicType(registrar)),
                        new(nameof(ShortField), BSATN.ShortField.GetAlgebraicType(registrar)),
                        new(nameof(IntField), BSATN.IntField.GetAlgebraicType(registrar)),
                        new(nameof(LongField), BSATN.LongField.GetAlgebraicType(registrar)),
                        new(nameof(Int128Field), BSATN.Int128Field.GetAlgebraicType(registrar)),
                        new(nameof(I128Field), BSATN.I128Field.GetAlgebraicType(registrar)),
                        new(nameof(I256Field), BSATN.I256Field.GetAlgebraicType(registrar)),
                        new(nameof(BoolField), BSATN.BoolField.GetAlgebraicType(registrar)),
                        new(nameof(FloatField), BSATN.FloatField.GetAlgebraicType(registrar)),
                        new(nameof(DoubleField), BSATN.DoubleField.GetAlgebraicType(registrar)),
                        new(nameof(StringField), BSATN.StringField.GetAlgebraicType(registrar)),
                        new(nameof(IdentityField), BSATN.IdentityField.GetAlgebraicType(registrar)),
                        new(nameof(AddressField), BSATN.AddressField.GetAlgebraicType(registrar)),
                        new(
                            nameof(CustomStructField),
                            BSATN.CustomStructField.GetAlgebraicType(registrar)
                        ),
                        new(
                            nameof(CustomClassField),
                            BSATN.CustomClassField.GetAlgebraicType(registrar)
                        ),
                        new(
                            nameof(CustomEnumField),
                            BSATN.CustomEnumField.GetAlgebraicType(registrar)
                        ),
                        new(
                            nameof(CustomTaggedEnumField),
                            BSATN.CustomTaggedEnumField.GetAlgebraicType(registrar)
                        ),
                        new(nameof(ListField), BSATN.ListField.GetAlgebraicType(registrar)),
                        new(
                            nameof(DictionaryField),
                            BSATN.DictionaryField.GetAlgebraicType(registrar)
                        ),
                        new(
                            nameof(NullableValueField),
                            BSATN.NullableValueField.GetAlgebraicType(registrar)
                        ),
                        new(
                            nameof(NullableReferenceField),
                            BSATN.NullableReferenceField.GetAlgebraicType(registrar)
                        ),
                        new(
                            nameof(ComplexNestedField),
                            BSATN.ComplexNestedField.GetAlgebraicType(registrar)
                        )
                    ],
                    Indexes: [],
                    Constraints:
                    [
                        new(
                            nameof(PublicTable),
                            0,
                            nameof(Id),
                            SpacetimeDB.Internal.ColumnAttrs.PrimaryKeyAuto
                        )
                    ],
                    Sequences: [],
                    // "system" | "user"
                    TableType: "user",
                    // "public" | "private"
                    TableAccess: "private",
                    Scheduled: null
                ),
                (uint)
                    (
                        (SpacetimeDB.BSATN.AlgebraicType.Ref)new BSATN().GetAlgebraicType(registrar)
                    ).Ref_
            ),
        ];

    static SpacetimeDB.Internal.Filter SpacetimeDB.Internal.ITable<PublicTable>.CreateFilter() =>
        new(
            [
                new(nameof(Id), (w, v) => BSATN.Id.Write(w, (int)v!)),
                new(nameof(ByteField), (w, v) => BSATN.ByteField.Write(w, (byte)v!)),
                new(nameof(UshortField), (w, v) => BSATN.UshortField.Write(w, (ushort)v!)),
                new(nameof(UintField), (w, v) => BSATN.UintField.Write(w, (uint)v!)),
                new(nameof(UlongField), (w, v) => BSATN.UlongField.Write(w, (ulong)v!)),
                new(
                    nameof(UInt128Field),
                    (w, v) => BSATN.UInt128Field.Write(w, (System.UInt128)v!)
                ),
                new(nameof(U128Field), (w, v) => BSATN.U128Field.Write(w, (SpacetimeDB.U128)v!)),
                new(nameof(U256Field), (w, v) => BSATN.U256Field.Write(w, (SpacetimeDB.U256)v!)),
                new(nameof(SbyteField), (w, v) => BSATN.SbyteField.Write(w, (sbyte)v!)),
                new(nameof(ShortField), (w, v) => BSATN.ShortField.Write(w, (short)v!)),
                new(nameof(IntField), (w, v) => BSATN.IntField.Write(w, (int)v!)),
                new(nameof(LongField), (w, v) => BSATN.LongField.Write(w, (long)v!)),
                new(nameof(Int128Field), (w, v) => BSATN.Int128Field.Write(w, (System.Int128)v!)),
                new(nameof(I128Field), (w, v) => BSATN.I128Field.Write(w, (SpacetimeDB.I128)v!)),
                new(nameof(I256Field), (w, v) => BSATN.I256Field.Write(w, (SpacetimeDB.I256)v!)),
                new(nameof(BoolField), (w, v) => BSATN.BoolField.Write(w, (bool)v!)),
                new(nameof(FloatField), (w, v) => BSATN.FloatField.Write(w, (float)v!)),
                new(nameof(DoubleField), (w, v) => BSATN.DoubleField.Write(w, (double)v!)),
                new(nameof(StringField), (w, v) => BSATN.StringField.Write(w, (string)v!)),
                new(
                    nameof(IdentityField),
                    (w, v) => BSATN.IdentityField.Write(w, (SpacetimeDB.Identity)v!)
                ),
                new(
                    nameof(AddressField),
                    (w, v) => BSATN.AddressField.Write(w, (SpacetimeDB.Address)v!)
                ),
                new(
                    nameof(CustomStructField),
                    (w, v) => BSATN.CustomStructField.Write(w, (CustomStruct)v!)
                ),
                new(
                    nameof(CustomClassField),
                    (w, v) => BSATN.CustomClassField.Write(w, (CustomClass)v!)
                ),
                new(
                    nameof(CustomEnumField),
                    (w, v) => BSATN.CustomEnumField.Write(w, (CustomEnum)v!)
                ),
                new(
                    nameof(CustomTaggedEnumField),
                    (w, v) => BSATN.CustomTaggedEnumField.Write(w, (CustomTaggedEnum)v!)
                ),
                new(
                    nameof(ListField),
                    (w, v) => BSATN.ListField.Write(w, (System.Collections.Generic.List<int>)v!)
                ),
                new(
                    nameof(DictionaryField),
                    (w, v) =>
                        BSATN.DictionaryField.Write(
                            w,
                            (System.Collections.Generic.Dictionary<string, int>)v!
                        )
                ),
                new(
                    nameof(NullableValueField),
                    (w, v) => BSATN.NullableValueField.Write(w, (int?)v!)
                ),
                new(
                    nameof(NullableReferenceField),
                    (w, v) => BSATN.NullableReferenceField.Write(w, (string?)v!)
                ),
                new(
                    nameof(ComplexNestedField),
                    (w, v) =>
                        BSATN.ComplexNestedField.Write(
                            w,
                            (System.Collections.Generic.Dictionary<
                                CustomEnum,
                                System.Collections.Generic.List<int?>?
                            >?)
                                v!
                        )
                )
            ]
        );
} // PublicTable
