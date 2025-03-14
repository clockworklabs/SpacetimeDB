﻿//HintName: CustomStruct.cs
// <auto-generated />
#nullable enable

partial struct CustomStruct
    : System.IEquatable<CustomStruct>,
        SpacetimeDB.BSATN.IStructuralReadWrite
{
    public void ReadFields(System.IO.BinaryReader reader)
    {
        IntField = BSATN.IntField.Read(reader);
        StringField = BSATN.StringField.Read(reader);
    }

    public void WriteFields(System.IO.BinaryWriter writer)
    {
        BSATN.IntField.Write(writer, IntField);
        BSATN.StringField.Write(writer, StringField);
    }

    public override string ToString() =>
        $"CustomStruct {{ IntField = {SpacetimeDB.BSATN.StringUtil.GenericToString(IntField)}, StringField = {SpacetimeDB.BSATN.StringUtil.GenericToString(StringField)} }}";

    public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<CustomStruct>
    {
        internal static readonly SpacetimeDB.BSATN.I32 IntField = new();
        internal static readonly SpacetimeDB.BSATN.String StringField = new();

        public CustomStruct Read(System.IO.BinaryReader reader) =>
            SpacetimeDB.BSATN.IStructuralReadWrite.Read<CustomStruct>(reader);

        public void Write(System.IO.BinaryWriter writer, CustomStruct value)
        {
            value.WriteFields(writer);
        }

        public SpacetimeDB.BSATN.AlgebraicType.Ref GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            registrar.RegisterType<CustomStruct>(_ => new SpacetimeDB.BSATN.AlgebraicType.Product(
                new SpacetimeDB.BSATN.AggregateElement[]
                {
                    new(nameof(IntField), IntField.GetAlgebraicType(registrar)),
                    new(nameof(StringField), StringField.GetAlgebraicType(registrar))
                }
            ));

        SpacetimeDB.BSATN.AlgebraicType SpacetimeDB.BSATN.IReadWrite<CustomStruct>.GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => GetAlgebraicType(registrar);
    }

    public override int GetHashCode()
    {
        return IntField.GetHashCode() ^ StringField.GetHashCode();
    }

#nullable enable
    public bool Equals(CustomStruct that)
    {
        return IntField.Equals(that.IntField) && StringField.Equals(that.StringField);
    }

    public override bool Equals(object? that)
    {
        if (that == null)
        {
            return false;
        }
        var that_ = that as CustomStruct?;
        if (((object?)that_) == null)
        {
            return false;
        }
        return Equals(that_);
    }

    public static bool operator ==(CustomStruct this_, CustomStruct that)
    {
        if (((object?)this_) == null || ((object?)that) == null)
        {
            return object.Equals(this_, that);
        }
        return this_.Equals(that);
    }

    public static bool operator !=(CustomStruct this_, CustomStruct that)
    {
        if (((object?)this_) == null || ((object?)that) == null)
        {
            return !object.Equals(this_, that);
        }
        return !this_.Equals(that);
    }
#nullable restore
} // CustomStruct
