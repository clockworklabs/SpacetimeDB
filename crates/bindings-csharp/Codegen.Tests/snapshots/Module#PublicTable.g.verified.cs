//HintName: PublicTable.g.cs
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
        ComplexNestedField = BSATN.ComplexNestedField.Read(reader);
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
        BSATN.ComplexNestedField.Write(writer, ComplexNestedField);
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
        internal static readonly SpacetimeDB.BSATN.RefOption<
            System.Collections.Generic.Dictionary<
                CustomEnum?,
                System.Collections.Generic.List<int?>?
            >,
            SpacetimeDB.BSATN.Dictionary<
                CustomEnum?,
                System.Collections.Generic.List<int?>?,
                SpacetimeDB.BSATN.ValueOption<CustomEnum, SpacetimeDB.BSATN.Enum<CustomEnum>>,
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
                        ),
                        new(
                            nameof(ComplexNestedField),
                            ComplexNestedField.GetAlgebraicType(registrar)
                        )
                    }
                )
            );
    }

    private static readonly Lazy<SpacetimeDB.RawBindings.TableId> tableId =
        new(() => SpacetimeDB.Runtime.GetTableId(nameof(PublicTable)));

    public static IEnumerable<PublicTable> Iter() =>
        new SpacetimeDB.Runtime.RawTableIter(tableId.Value).Parse<PublicTable>();

    public static SpacetimeDB.Module.TableDesc MakeTableDesc(
        SpacetimeDB.BSATN.ITypeRegistrar registrar
    ) =>
        new(
            new(
                nameof(PublicTable),
                new SpacetimeDB.Module.ColumnDefWithAttrs[]
                {
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(Id),
                            BSATN.Id.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.PrimaryKeyAuto
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(ByteField),
                            BSATN.ByteField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(UshortField),
                            BSATN.UshortField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(UintField),
                            BSATN.UintField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(UlongField),
                            BSATN.UlongField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(Uint128Field),
                            BSATN.Uint128Field.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(SbyteField),
                            BSATN.SbyteField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(ShortField),
                            BSATN.ShortField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(IntField),
                            BSATN.IntField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(LongField),
                            BSATN.LongField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(Int128Field),
                            BSATN.Int128Field.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(BoolField),
                            BSATN.BoolField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(FloatField),
                            BSATN.FloatField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(DoubleField),
                            BSATN.DoubleField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(StringField),
                            BSATN.StringField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(IdentityField),
                            BSATN.IdentityField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(AddressField),
                            BSATN.AddressField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(CustomStructField),
                            BSATN.CustomStructField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(CustomClassField),
                            BSATN.CustomClassField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(CustomEnumField),
                            BSATN.CustomEnumField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(CustomTaggedEnumField),
                            BSATN.CustomTaggedEnumField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(ListField),
                            BSATN.ListField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(DictionaryField),
                            BSATN.DictionaryField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(NullableValueField),
                            BSATN.NullableValueField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(NullableReferenceField),
                            BSATN.NullableReferenceField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    ),
                    new(
                        new SpacetimeDB.Module.ColumnDef(
                            nameof(ComplexNestedField),
                            BSATN.ComplexNestedField.GetAlgebraicType(registrar)
                        ),
                        SpacetimeDB.Module.ColumnAttrs.UnSet
                    )
                },
                false
            ),
            (SpacetimeDB.BSATN.AlgebraicType.Ref)new BSATN().GetAlgebraicType(registrar)
        );

    private static readonly Lazy<KeyValuePair<
        string,
        Action<BinaryWriter, object?>
    >[]> fieldTypeInfos =
        new(
            () =>
                new KeyValuePair<string, Action<BinaryWriter, object?>>[]
                {
                    new(nameof(Id), (w, v) => BSATN.Id.Write(w, (int)v!)),
                    new(nameof(ByteField), (w, v) => BSATN.ByteField.Write(w, (byte)v!)),
                    new(nameof(UshortField), (w, v) => BSATN.UshortField.Write(w, (ushort)v!)),
                    new(nameof(UintField), (w, v) => BSATN.UintField.Write(w, (uint)v!)),
                    new(nameof(UlongField), (w, v) => BSATN.UlongField.Write(w, (ulong)v!)),
                    new(
                        nameof(Uint128Field),
                        (w, v) => BSATN.Uint128Field.Write(w, (System.UInt128)v!)
                    ),
                    new(nameof(SbyteField), (w, v) => BSATN.SbyteField.Write(w, (sbyte)v!)),
                    new(nameof(ShortField), (w, v) => BSATN.ShortField.Write(w, (short)v!)),
                    new(nameof(IntField), (w, v) => BSATN.IntField.Write(w, (int)v!)),
                    new(nameof(LongField), (w, v) => BSATN.LongField.Write(w, (long)v!)),
                    new(
                        nameof(Int128Field),
                        (w, v) => BSATN.Int128Field.Write(w, (System.Int128)v!)
                    ),
                    new(nameof(BoolField), (w, v) => BSATN.BoolField.Write(w, (bool)v!)),
                    new(nameof(FloatField), (w, v) => BSATN.FloatField.Write(w, (float)v!)),
                    new(nameof(DoubleField), (w, v) => BSATN.DoubleField.Write(w, (double)v!)),
                    new(nameof(StringField), (w, v) => BSATN.StringField.Write(w, (string)v!)),
                    new(
                        nameof(IdentityField),
                        (w, v) => BSATN.IdentityField.Write(w, (SpacetimeDB.Runtime.Identity)v!)
                    ),
                    new(
                        nameof(AddressField),
                        (w, v) => BSATN.AddressField.Write(w, (SpacetimeDB.Runtime.Address)v!)
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
                                    CustomEnum?,
                                    System.Collections.Generic.List<int?>?
                                >?)
                                    v!
                            )
                    ),
                }
        );

    public static IEnumerable<PublicTable> Query(
        System.Linq.Expressions.Expression<Func<PublicTable, bool>> filter
    ) =>
        new SpacetimeDB.Runtime.RawTableIterFiltered(
            tableId.Value,
            SpacetimeDB.Filter.Filter.Compile<PublicTable>(fieldTypeInfos.Value, filter)
        ).Parse<PublicTable>();

    public void Insert()
    {
        var bytes = SpacetimeDB.Runtime.Insert(tableId.Value, this);
        // bytes should contain modified value now with autoinc fields updated
        using var stream = new System.IO.MemoryStream(bytes);
        using var reader = new System.IO.BinaryReader(stream);
        ReadFields(reader);
    }

    public static IEnumerable<PublicTable> FilterById(int Id) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(0),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.Id, Id)
        ).Parse<PublicTable>();

    public static PublicTable? FindById(int Id) =>
        FilterById(Id).Cast<PublicTable?>().SingleOrDefault();

    public static bool DeleteById(int Id) =>
        SpacetimeDB.Runtime.DeleteByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(0),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.Id, Id)
        ) > 0;

    public static bool UpdateById(int Id, PublicTable value) =>
        SpacetimeDB.Runtime.UpdateByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(0),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.Id, Id),
            value
        );

    public static IEnumerable<PublicTable> FilterByByteField(byte ByteField) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(1),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.ByteField, ByteField)
        ).Parse<PublicTable>();

    public static IEnumerable<PublicTable> FilterByUshortField(ushort UshortField) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(2),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.UshortField, UshortField)
        ).Parse<PublicTable>();

    public static IEnumerable<PublicTable> FilterByUintField(uint UintField) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(3),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.UintField, UintField)
        ).Parse<PublicTable>();

    public static IEnumerable<PublicTable> FilterByUlongField(ulong UlongField) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(4),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.UlongField, UlongField)
        ).Parse<PublicTable>();

    public static IEnumerable<PublicTable> FilterByUint128Field(System.UInt128 Uint128Field) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(5),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.Uint128Field, Uint128Field)
        ).Parse<PublicTable>();

    public static IEnumerable<PublicTable> FilterBySbyteField(sbyte SbyteField) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(6),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.SbyteField, SbyteField)
        ).Parse<PublicTable>();

    public static IEnumerable<PublicTable> FilterByShortField(short ShortField) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(7),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.ShortField, ShortField)
        ).Parse<PublicTable>();

    public static IEnumerable<PublicTable> FilterByIntField(int IntField) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(8),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.IntField, IntField)
        ).Parse<PublicTable>();

    public static IEnumerable<PublicTable> FilterByLongField(long LongField) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(9),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.LongField, LongField)
        ).Parse<PublicTable>();

    public static IEnumerable<PublicTable> FilterByInt128Field(System.Int128 Int128Field) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(10),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.Int128Field, Int128Field)
        ).Parse<PublicTable>();

    public static IEnumerable<PublicTable> FilterByBoolField(bool BoolField) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(11),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.BoolField, BoolField)
        ).Parse<PublicTable>();

    public static IEnumerable<PublicTable> FilterByStringField(string StringField) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(14),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.StringField, StringField)
        ).Parse<PublicTable>();

    public static IEnumerable<PublicTable> FilterByIdentityField(
        SpacetimeDB.Runtime.Identity IdentityField
    ) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(15),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.IdentityField, IdentityField)
        ).Parse<PublicTable>();

    public static IEnumerable<PublicTable> FilterByAddressField(
        SpacetimeDB.Runtime.Address AddressField
    ) =>
        new SpacetimeDB.Runtime.RawTableIterByColEq(
            tableId.Value,
            new SpacetimeDB.RawBindings.ColId(16),
            SpacetimeDB.BSATN.IStructuralReadWrite.ToBytes(BSATN.AddressField, AddressField)
        ).Parse<PublicTable>();
} // PublicTable
