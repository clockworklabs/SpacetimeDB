﻿//HintName: PublicTable.cs
// <auto-generated />
#nullable enable

partial struct PublicTable : System.IEquatable<PublicTable>, SpacetimeDB.BSATN.IStructuralReadWrite
{
    public void ReadFields(System.IO.BinaryReader reader)
    {
        ByteField = BSATN.ByteField.Read(reader);
        UshortField = BSATN.UshortField.Read(reader);
        UintField = BSATN.UintField.Read(reader);
        UlongField = BSATN.UlongField.Read(reader);
        U128Field = BSATN.U128Field.Read(reader);
        U256Field = BSATN.U256Field.Read(reader);
        SbyteField = BSATN.SbyteField.Read(reader);
        ShortField = BSATN.ShortField.Read(reader);
        IntField = BSATN.IntField.Read(reader);
        LongField = BSATN.LongField.Read(reader);
        I128Field = BSATN.I128Field.Read(reader);
        I256Field = BSATN.I256Field.Read(reader);
        BoolField = BSATN.BoolField.Read(reader);
        FloatField = BSATN.FloatField.Read(reader);
        DoubleField = BSATN.DoubleField.Read(reader);
        StringField = BSATN.StringField.Read(reader);
        IdentityField = BSATN.IdentityField.Read(reader);
        ConnectionIdField = BSATN.ConnectionIdField.Read(reader);
        CustomStructField = BSATN.CustomStructField.Read(reader);
        CustomClassField = BSATN.CustomClassField.Read(reader);
        CustomEnumField = BSATN.CustomEnumField.Read(reader);
        CustomTaggedEnumField = BSATN.CustomTaggedEnumField.Read(reader);
        ListField = BSATN.ListField.Read(reader);
        NullableValueField = BSATN.NullableValueField.Read(reader);
        NullableReferenceField = BSATN.NullableReferenceField.Read(reader);
    }

    public void WriteFields(System.IO.BinaryWriter writer)
    {
        BSATN.ByteField.Write(writer, ByteField);
        BSATN.UshortField.Write(writer, UshortField);
        BSATN.UintField.Write(writer, UintField);
        BSATN.UlongField.Write(writer, UlongField);
        BSATN.U128Field.Write(writer, U128Field);
        BSATN.U256Field.Write(writer, U256Field);
        BSATN.SbyteField.Write(writer, SbyteField);
        BSATN.ShortField.Write(writer, ShortField);
        BSATN.IntField.Write(writer, IntField);
        BSATN.LongField.Write(writer, LongField);
        BSATN.I128Field.Write(writer, I128Field);
        BSATN.I256Field.Write(writer, I256Field);
        BSATN.BoolField.Write(writer, BoolField);
        BSATN.FloatField.Write(writer, FloatField);
        BSATN.DoubleField.Write(writer, DoubleField);
        BSATN.StringField.Write(writer, StringField);
        BSATN.IdentityField.Write(writer, IdentityField);
        BSATN.ConnectionIdField.Write(writer, ConnectionIdField);
        BSATN.CustomStructField.Write(writer, CustomStructField);
        BSATN.CustomClassField.Write(writer, CustomClassField);
        BSATN.CustomEnumField.Write(writer, CustomEnumField);
        BSATN.CustomTaggedEnumField.Write(writer, CustomTaggedEnumField);
        BSATN.ListField.Write(writer, ListField);
        BSATN.NullableValueField.Write(writer, NullableValueField);
        BSATN.NullableReferenceField.Write(writer, NullableReferenceField);
    }

    public override string ToString() =>
        $"PublicTable {{ ByteField = {SpacetimeDB.BSATN.StringUtil.GenericToString(ByteField)}, UshortField = {SpacetimeDB.BSATN.StringUtil.GenericToString(UshortField)}, UintField = {SpacetimeDB.BSATN.StringUtil.GenericToString(UintField)}, UlongField = {SpacetimeDB.BSATN.StringUtil.GenericToString(UlongField)}, U128Field = {SpacetimeDB.BSATN.StringUtil.GenericToString(U128Field)}, U256Field = {SpacetimeDB.BSATN.StringUtil.GenericToString(U256Field)}, SbyteField = {SpacetimeDB.BSATN.StringUtil.GenericToString(SbyteField)}, ShortField = {SpacetimeDB.BSATN.StringUtil.GenericToString(ShortField)}, IntField = {SpacetimeDB.BSATN.StringUtil.GenericToString(IntField)}, LongField = {SpacetimeDB.BSATN.StringUtil.GenericToString(LongField)}, I128Field = {SpacetimeDB.BSATN.StringUtil.GenericToString(I128Field)}, I256Field = {SpacetimeDB.BSATN.StringUtil.GenericToString(I256Field)}, BoolField = {SpacetimeDB.BSATN.StringUtil.GenericToString(BoolField)}, FloatField = {SpacetimeDB.BSATN.StringUtil.GenericToString(FloatField)}, DoubleField = {SpacetimeDB.BSATN.StringUtil.GenericToString(DoubleField)}, StringField = {SpacetimeDB.BSATN.StringUtil.GenericToString(StringField)}, IdentityField = {SpacetimeDB.BSATN.StringUtil.GenericToString(IdentityField)}, ConnectionIdField = {SpacetimeDB.BSATN.StringUtil.GenericToString(ConnectionIdField)}, CustomStructField = {SpacetimeDB.BSATN.StringUtil.GenericToString(CustomStructField)}, CustomClassField = {SpacetimeDB.BSATN.StringUtil.GenericToString(CustomClassField)}, CustomEnumField = {SpacetimeDB.BSATN.StringUtil.GenericToString(CustomEnumField)}, CustomTaggedEnumField = {SpacetimeDB.BSATN.StringUtil.GenericToString(CustomTaggedEnumField)}, ListField = {SpacetimeDB.BSATN.StringUtil.GenericToString(ListField)}, NullableValueField = {SpacetimeDB.BSATN.StringUtil.GenericToString(NullableValueField)}, NullableReferenceField = {SpacetimeDB.BSATN.StringUtil.GenericToString(NullableReferenceField)} }}";

    public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<PublicTable>
    {
        internal static readonly SpacetimeDB.BSATN.U8 ByteField = new();
        internal static readonly SpacetimeDB.BSATN.U16 UshortField = new();
        internal static readonly SpacetimeDB.BSATN.U32 UintField = new();
        internal static readonly SpacetimeDB.BSATN.U64 UlongField = new();
        internal static readonly SpacetimeDB.BSATN.U128Stdb U128Field = new();
        internal static readonly SpacetimeDB.BSATN.U256 U256Field = new();
        internal static readonly SpacetimeDB.BSATN.I8 SbyteField = new();
        internal static readonly SpacetimeDB.BSATN.I16 ShortField = new();
        internal static readonly SpacetimeDB.BSATN.I32 IntField = new();
        internal static readonly SpacetimeDB.BSATN.I64 LongField = new();
        internal static readonly SpacetimeDB.BSATN.I128Stdb I128Field = new();
        internal static readonly SpacetimeDB.BSATN.I256 I256Field = new();
        internal static readonly SpacetimeDB.BSATN.Bool BoolField = new();
        internal static readonly SpacetimeDB.BSATN.F32 FloatField = new();
        internal static readonly SpacetimeDB.BSATN.F64 DoubleField = new();
        internal static readonly SpacetimeDB.BSATN.String StringField = new();
        internal static readonly SpacetimeDB.Identity.BSATN IdentityField = new();
        internal static readonly SpacetimeDB.ConnectionId.BSATN ConnectionIdField = new();
        internal static readonly CustomStruct.BSATN CustomStructField = new();
        internal static readonly CustomClass.BSATN CustomClassField = new();
        internal static readonly SpacetimeDB.BSATN.Enum<CustomEnum> CustomEnumField = new();
        internal static readonly CustomTaggedEnum.BSATN CustomTaggedEnumField = new();
        internal static readonly SpacetimeDB.BSATN.List<int, SpacetimeDB.BSATN.I32> ListField =
            new();
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

        public SpacetimeDB.BSATN.AlgebraicType.Ref GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            registrar.RegisterType<PublicTable>(_ => new SpacetimeDB.BSATN.AlgebraicType.Product(
                new SpacetimeDB.BSATN.AggregateElement[]
                {
                    new(nameof(ByteField), ByteField.GetAlgebraicType(registrar)),
                    new(nameof(UshortField), UshortField.GetAlgebraicType(registrar)),
                    new(nameof(UintField), UintField.GetAlgebraicType(registrar)),
                    new(nameof(UlongField), UlongField.GetAlgebraicType(registrar)),
                    new(nameof(U128Field), U128Field.GetAlgebraicType(registrar)),
                    new(nameof(U256Field), U256Field.GetAlgebraicType(registrar)),
                    new(nameof(SbyteField), SbyteField.GetAlgebraicType(registrar)),
                    new(nameof(ShortField), ShortField.GetAlgebraicType(registrar)),
                    new(nameof(IntField), IntField.GetAlgebraicType(registrar)),
                    new(nameof(LongField), LongField.GetAlgebraicType(registrar)),
                    new(nameof(I128Field), I128Field.GetAlgebraicType(registrar)),
                    new(nameof(I256Field), I256Field.GetAlgebraicType(registrar)),
                    new(nameof(BoolField), BoolField.GetAlgebraicType(registrar)),
                    new(nameof(FloatField), FloatField.GetAlgebraicType(registrar)),
                    new(nameof(DoubleField), DoubleField.GetAlgebraicType(registrar)),
                    new(nameof(StringField), StringField.GetAlgebraicType(registrar)),
                    new(nameof(IdentityField), IdentityField.GetAlgebraicType(registrar)),
                    new(nameof(ConnectionIdField), ConnectionIdField.GetAlgebraicType(registrar)),
                    new(nameof(CustomStructField), CustomStructField.GetAlgebraicType(registrar)),
                    new(nameof(CustomClassField), CustomClassField.GetAlgebraicType(registrar)),
                    new(nameof(CustomEnumField), CustomEnumField.GetAlgebraicType(registrar)),
                    new(
                        nameof(CustomTaggedEnumField),
                        CustomTaggedEnumField.GetAlgebraicType(registrar)
                    ),
                    new(nameof(ListField), ListField.GetAlgebraicType(registrar)),
                    new(nameof(NullableValueField), NullableValueField.GetAlgebraicType(registrar)),
                    new(
                        nameof(NullableReferenceField),
                        NullableReferenceField.GetAlgebraicType(registrar)
                    )
                }
            ));

        SpacetimeDB.BSATN.AlgebraicType SpacetimeDB.BSATN.IReadWrite<PublicTable>.GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => GetAlgebraicType(registrar);
    }

    public override int GetHashCode()
    {
        return ByteField.GetHashCode()
            ^ UshortField.GetHashCode()
            ^ UintField.GetHashCode()
            ^ UlongField.GetHashCode()
            ^ U128Field.GetHashCode()
            ^ U256Field.GetHashCode()
            ^ SbyteField.GetHashCode()
            ^ ShortField.GetHashCode()
            ^ IntField.GetHashCode()
            ^ LongField.GetHashCode()
            ^ I128Field.GetHashCode()
            ^ I256Field.GetHashCode()
            ^ BoolField.GetHashCode()
            ^ FloatField.GetHashCode()
            ^ DoubleField.GetHashCode()
            ^ StringField.GetHashCode()
            ^ IdentityField.GetHashCode()
            ^ ConnectionIdField.GetHashCode()
            ^ CustomStructField.GetHashCode()
            ^ CustomClassField.GetHashCode()
            ^ CustomEnumField.GetHashCode()
            ^ CustomTaggedEnumField.GetHashCode()
            ^ ListField.GetHashCode()
            ^ NullableValueField.GetHashCode()
            ^ (NullableReferenceField == null ? 0 : NullableReferenceField.GetHashCode());
    }

#nullable enable
    public bool Equals(PublicTable that)
    {
        return ByteField.Equals(that.ByteField)
            && UshortField.Equals(that.UshortField)
            && UintField.Equals(that.UintField)
            && UlongField.Equals(that.UlongField)
            && U128Field.Equals(that.U128Field)
            && U256Field.Equals(that.U256Field)
            && SbyteField.Equals(that.SbyteField)
            && ShortField.Equals(that.ShortField)
            && IntField.Equals(that.IntField)
            && LongField.Equals(that.LongField)
            && I128Field.Equals(that.I128Field)
            && I256Field.Equals(that.I256Field)
            && BoolField.Equals(that.BoolField)
            && FloatField.Equals(that.FloatField)
            && DoubleField.Equals(that.DoubleField)
            && StringField.Equals(that.StringField)
            && IdentityField.Equals(that.IdentityField)
            && ConnectionIdField.Equals(that.ConnectionIdField)
            && CustomStructField.Equals(that.CustomStructField)
            && CustomClassField.Equals(that.CustomClassField)
            && CustomEnumField.Equals(that.CustomEnumField)
            && CustomTaggedEnumField.Equals(that.CustomTaggedEnumField)
            && ListField.Equals(that.ListField)
            && NullableValueField.Equals(that.NullableValueField)
            && (
                NullableReferenceField == null
                    ? that.NullableReferenceField == null
                    : NullableReferenceField.Equals(that.NullableReferenceField)
            );
    }

    public override bool Equals(object? that)
    {
        if (that == null)
        {
            return false;
        }
        var that_ = that as PublicTable?;
        if (((object?)that_) == null)
        {
            return false;
        }
        return Equals(that_);
    }

    public static bool operator ==(PublicTable this_, PublicTable that)
    {
        if (((object?)this_) == null || ((object?)that) == null)
        {
            return object.Equals(this_, that);
        }
        return this_.Equals(that);
    }

    public static bool operator !=(PublicTable this_, PublicTable that)
    {
        if (((object?)this_) == null || ((object?)that) == null)
        {
            return !object.Equals(this_, that);
        }
        return !this_.Equals(that);
    }
#nullable restore
} // PublicTable
