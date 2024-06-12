﻿//HintName: PublicTable.cs

// <auto-generated />
#nullable enable

partial class PublicTable : SpacetimeDB.BSATN.IStructuralReadWrite
{
    public void ReadFields(System.IO.BinaryReader reader)
    {
        Id = BSATN.Id.Read(reader);
        ByteField = BSATN.ByteField.Read(reader);
        UshortField = BSATN.UshortField.Read(reader);
        UintField = BSATN.UintField.Read(reader);
        UlongField = BSATN.UlongField.Read(reader);
        Uint128Field = BSATN.Uint128Field.Read(reader);
        SbyteField = BSATN.SbyteField.Read(reader);
        ShortField = BSATN.ShortField.Read(reader);
        IntField = BSATN.IntField.Read(reader);
        LongField = BSATN.LongField.Read(reader);
        Int128Field = BSATN.Int128Field.Read(reader);
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
    }

    public void WriteFields(System.IO.BinaryWriter writer)
    {
        BSATN.Id.Write(writer, Id);
        BSATN.ByteField.Write(writer, ByteField);
        BSATN.UshortField.Write(writer, UshortField);
        BSATN.UintField.Write(writer, UintField);
        BSATN.UlongField.Write(writer, UlongField);
        BSATN.Uint128Field.Write(writer, Uint128Field);
        BSATN.SbyteField.Write(writer, SbyteField);
        BSATN.ShortField.Write(writer, ShortField);
        BSATN.IntField.Write(writer, IntField);
        BSATN.LongField.Write(writer, LongField);
        BSATN.Int128Field.Write(writer, Int128Field);
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
    }

    public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<PublicTable>
    {
        internal static readonly SpacetimeDB.BSATN.I32 Id = new();
        internal static readonly SpacetimeDB.BSATN.U8 ByteField = new();
        internal static readonly SpacetimeDB.BSATN.U16 UshortField = new();
        internal static readonly SpacetimeDB.BSATN.U32 UintField = new();
        internal static readonly SpacetimeDB.BSATN.U64 UlongField = new();
        internal static readonly SpacetimeDB.BSATN.U128 Uint128Field = new();
        internal static readonly SpacetimeDB.BSATN.I8 SbyteField = new();
        internal static readonly SpacetimeDB.BSATN.I16 ShortField = new();
        internal static readonly SpacetimeDB.BSATN.I32 IntField = new();
        internal static readonly SpacetimeDB.BSATN.I64 LongField = new();
        internal static readonly SpacetimeDB.BSATN.I128 Int128Field = new();
        internal static readonly SpacetimeDB.BSATN.Bool BoolField = new();
        internal static readonly SpacetimeDB.BSATN.F32 FloatField = new();
        internal static readonly SpacetimeDB.BSATN.F64 DoubleField = new();
        internal static readonly SpacetimeDB.BSATN.String StringField = new();
        internal static readonly SpacetimeDB.Runtime.Identity.BSATN IdentityField = new();
        internal static readonly SpacetimeDB.Runtime.Address.BSATN AddressField = new();
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

        public PublicTable Read(System.IO.BinaryReader reader) =>
            SpacetimeDB.BSATN.IStructuralReadWrite.Read<PublicTable>(reader);

        public void Write(System.IO.BinaryWriter writer, PublicTable value)
        {
            value.WriteFields(writer);
        }

        public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            registrar.RegisterType<PublicTable>(
                typeRef => new SpacetimeDB.BSATN.AlgebraicType.Product(
                    new SpacetimeDB.BSATN.AggregateElement[]
                    {
                        new(nameof(Id), Id.GetAlgebraicType(registrar)),
                        new(nameof(ByteField), ByteField.GetAlgebraicType(registrar)),
                        new(nameof(UshortField), UshortField.GetAlgebraicType(registrar)),
                        new(nameof(UintField), UintField.GetAlgebraicType(registrar)),
                        new(nameof(UlongField), UlongField.GetAlgebraicType(registrar)),
                        new(nameof(Uint128Field), Uint128Field.GetAlgebraicType(registrar)),
                        new(nameof(SbyteField), SbyteField.GetAlgebraicType(registrar)),
                        new(nameof(ShortField), ShortField.GetAlgebraicType(registrar)),
                        new(nameof(IntField), IntField.GetAlgebraicType(registrar)),
                        new(nameof(LongField), LongField.GetAlgebraicType(registrar)),
                        new(nameof(Int128Field), Int128Field.GetAlgebraicType(registrar)),
                        new(nameof(BoolField), BoolField.GetAlgebraicType(registrar)),
                        new(nameof(FloatField), FloatField.GetAlgebraicType(registrar)),
                        new(nameof(DoubleField), DoubleField.GetAlgebraicType(registrar)),
                        new(nameof(StringField), StringField.GetAlgebraicType(registrar)),
                        new(nameof(IdentityField), IdentityField.GetAlgebraicType(registrar)),
                        new(nameof(AddressField), AddressField.GetAlgebraicType(registrar)),
                        new(
                            nameof(CustomStructField),
                            CustomStructField.GetAlgebraicType(registrar)
                        ),
                        new(nameof(CustomClassField), CustomClassField.GetAlgebraicType(registrar)),
                        new(nameof(CustomEnumField), CustomEnumField.GetAlgebraicType(registrar)),
                        new(
                            nameof(CustomTaggedEnumField),
                            CustomTaggedEnumField.GetAlgebraicType(registrar)
                        ),
                        new(nameof(ListField), ListField.GetAlgebraicType(registrar)),
                        new(nameof(DictionaryField), DictionaryField.GetAlgebraicType(registrar)),
                        new(
                            nameof(NullableValueField),
                            NullableValueField.GetAlgebraicType(registrar)
                        ),
                        new(
                            nameof(NullableReferenceField),
                            NullableReferenceField.GetAlgebraicType(registrar)
                        )
                    }
                )
            );
    }
} // PublicTable
