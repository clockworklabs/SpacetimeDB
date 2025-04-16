﻿//HintName: CustomClass.cs
// <auto-generated />
#nullable enable

partial struct CustomClass : System.IEquatable<CustomClass>, SpacetimeDB.BSATN.IStructuralReadWrite
{
    public void ReadFields(System.IO.BinaryReader reader)
    {
        IntField = BSATN.IntFieldRW.Read(reader);
        StringField = BSATN.StringFieldRW.Read(reader);
    }

    public void WriteFields(System.IO.BinaryWriter writer)
    {
        BSATN.IntFieldRW.Write(writer, IntField);
        BSATN.StringFieldRW.Write(writer, StringField);
    }

    public override string ToString() =>
        $"CustomClass {{ IntField = {SpacetimeDB.BSATN.StringUtil.GenericToString(IntField)}, StringField = {SpacetimeDB.BSATN.StringUtil.GenericToString(StringField)} }}";

    public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<CustomClass>
    {
        internal static readonly SpacetimeDB.BSATN.I32 IntFieldRW = new();
        internal static readonly SpacetimeDB.BSATN.String StringFieldRW = new();

        public CustomClass Read(System.IO.BinaryReader reader) =>
            SpacetimeDB.BSATN.IStructuralReadWrite.Read<CustomClass>(reader);

        public void Write(System.IO.BinaryWriter writer, CustomClass value)
        {
            value.WriteFields(writer);
        }

        public SpacetimeDB.BSATN.AlgebraicType.Ref GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            registrar.RegisterType<CustomClass>(_ => new SpacetimeDB.BSATN.AlgebraicType.Product(
                new SpacetimeDB.BSATN.AggregateElement[]
                {
                    new("IntField", IntFieldRW.GetAlgebraicType(registrar)),
                    new("StringField", StringFieldRW.GetAlgebraicType(registrar))
                }
            ));

        SpacetimeDB.BSATN.AlgebraicType SpacetimeDB.BSATN.IReadWrite<CustomClass>.GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) => GetAlgebraicType(registrar);
    }

    public override int GetHashCode()
    {
        return IntField.GetHashCode() ^ StringField.GetHashCode();
    }

#nullable enable
    public bool Equals(CustomClass that)
    {
        return IntField.Equals(that.IntField) && StringField.Equals(that.StringField);
    }

    public override bool Equals(object? that)
    {
        if (that == null)
        {
            return false;
        }
        var that_ = that as CustomClass?;
        if (((object?)that_) == null)
        {
            return false;
        }
        return Equals(that_);
    }

    public static bool operator ==(CustomClass this_, CustomClass that)
    {
        if (((object?)this_) == null || ((object?)that) == null)
        {
            return object.Equals(this_, that);
        }
        return this_.Equals(that);
    }

    public static bool operator !=(CustomClass this_, CustomClass that)
    {
        if (((object?)this_) == null || ((object?)that) == null)
        {
            return !object.Equals(this_, that);
        }
        return !this_.Equals(that);
    }
#nullable restore
} // CustomClass
