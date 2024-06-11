﻿//HintName: CustomStruct.cs

// <auto-generated />
#nullable enable

partial struct CustomStruct : SpacetimeDB.BSATN.IStructuralReadWrite
{
    public void ReadFields(System.IO.BinaryReader reader)
    {
        intField = BSATN.intField.Read(reader);
        stringField = BSATN.stringField.Read(reader);
    }

    public void WriteFields(System.IO.BinaryWriter writer)
    {
        BSATN.intField.Write(writer, intField);
        BSATN.stringField.Write(writer, stringField);
    }

    public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<CustomStruct>
    {
        internal static readonly SpacetimeDB.BSATN.I32 intField = new();
        internal static readonly SpacetimeDB.BSATN.String stringField = new();

        public CustomStruct Read(System.IO.BinaryReader reader) =>
            SpacetimeDB.BSATN.IStructuralReadWrite.Read<CustomStruct>(reader);

        public void Write(System.IO.BinaryWriter writer, CustomStruct value)
        {
            value.WriteFields(writer);
        }

        public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            registrar.RegisterType<CustomStruct>(
                typeRef => new SpacetimeDB.BSATN.AlgebraicType.Product(
                    new SpacetimeDB.BSATN.AggregateElement[]
                    {
                        new(nameof(intField), intField.GetAlgebraicType(registrar)),
                        new(nameof(stringField), stringField.GetAlgebraicType(registrar))
                    }
                )
            );
    }
} // CustomStruct