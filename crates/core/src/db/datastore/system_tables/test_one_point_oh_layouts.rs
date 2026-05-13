//! Test module storing the layouts of the system tables as of 1.0.
//! Split out from the main module to avoid cluttering it with large constants.

use spacetimedb_lib::SpacetimeType;
use spacetimedb_sats::AlgebraicType;
use spacetimedb_table::layout::{
    AlgebraicTypeLayout::{self, *},
    Layout,
    PrimitiveType::*,
    ProductTypeElementLayout, ProductTypeLayout, SumTypeLayout, SumTypeVariantLayout,
    VarLenType::*,
};

use super::{
    StClientRow, StColumnRow, StConstraintRow, StIndexRow, StModuleRow, StScheduledRow, StSequenceRow, StTableRow,
    StVarRow,
};

pub(super) trait HasOnePointOhLayout: SpacetimeType {
    /// Get the original layout of the type as of 1.0.
    /// Note: if the logic for computing layouts ever changes, you'll need to think VERY CAREFULLY
    /// about how to update this function.
    fn get_original_layout() -> AlgebraicTypeLayout;
}

macro_rules! boxed {
    ($($var:expr),* $(,)?) => {
        vec![$($var),*].into()
    };
}

fn col_list_layout() -> AlgebraicTypeLayout {
    VarLen(Array(Box::new(AlgebraicType::array(AlgebraicType::U16))))
}

fn bytes_layout() -> AlgebraicTypeLayout {
    VarLen(Array(Box::new(AlgebraicType::array(AlgebraicType::U8))))
}

impl HasOnePointOhLayout for StTableRow {
    fn get_original_layout() -> AlgebraicTypeLayout {
        Product(ProductTypeLayout {
            layout: Layout { size: 24, align: 4 },
            elements: boxed![
                ProductTypeElementLayout {
                    offset: 0,
                    ty: Primitive(U32),
                    name: Some("table_id".into()),
                },
                ProductTypeElementLayout {
                    offset: 4,
                    ty: VarLen(String),
                    name: Some("table_name".into()),
                },
                ProductTypeElementLayout {
                    offset: 8,
                    ty: VarLen(String),
                    name: Some("table_type".into()),
                },
                ProductTypeElementLayout {
                    offset: 12,
                    ty: VarLen(String),
                    name: Some("table_access".into()),
                },
                ProductTypeElementLayout {
                    offset: 16,
                    ty: Sum(SumTypeLayout {
                        layout: Layout { size: 6, align: 2 },
                        variants: boxed![
                            SumTypeVariantLayout {
                                ty: col_list_layout(),
                                name: Some("some".into()),
                            },
                            SumTypeVariantLayout {
                                ty: Product(ProductTypeLayout {
                                    layout: Layout { size: 0, align: 1 },
                                    elements: Box::new([]),
                                }),
                                name: Some("none".into()),
                            },
                        ],
                        payload_offset: 2,
                    }),
                    name: Some("table_primary_key".into()),
                },
            ],
        })
    }
}
impl HasOnePointOhLayout for StColumnRow {
    fn get_original_layout() -> AlgebraicTypeLayout {
        Product(ProductTypeLayout {
            layout: Layout { size: 16, align: 4 },
            elements: boxed![
                ProductTypeElementLayout {
                    offset: 0,
                    ty: Primitive(U32),
                    name: Some("table_id".into()),
                },
                ProductTypeElementLayout {
                    offset: 4,
                    ty: Primitive(U16),
                    name: Some("col_pos".into()),
                },
                ProductTypeElementLayout {
                    offset: 6,
                    ty: VarLen(String),
                    name: Some("col_name".into()),
                },
                ProductTypeElementLayout {
                    offset: 10,
                    ty: bytes_layout(),
                    name: Some("col_type".into()),
                },
            ],
        })
    }
}
impl HasOnePointOhLayout for StIndexRow {
    fn get_original_layout() -> AlgebraicTypeLayout {
        Product(ProductTypeLayout {
            layout: Layout { size: 48, align: 16 },
            elements: boxed![
                ProductTypeElementLayout {
                    offset: 0,
                    ty: Primitive(U32),
                    name: Some("index_id".into()),
                },
                ProductTypeElementLayout {
                    offset: 4,
                    ty: Primitive(U32),
                    name: Some("table_id".into()),
                },
                ProductTypeElementLayout {
                    offset: 8,
                    ty: VarLen(String),
                    name: Some("index_name".into()),
                },
                ProductTypeElementLayout {
                    offset: 16,
                    ty: Sum(SumTypeLayout {
                        layout: Layout { size: 32, align: 16 },
                        variants: boxed![
                            SumTypeVariantLayout {
                                ty: Primitive(U128),
                                name: Some("Unused".into()),
                            },
                            SumTypeVariantLayout {
                                ty: col_list_layout(),
                                name: Some("BTree".into()),
                            },
                        ],
                        payload_offset: 16,
                    }),
                    name: Some("index_algorithm".into()),
                },
            ],
        })
    }
}
impl HasOnePointOhLayout for StSequenceRow {
    fn get_original_layout() -> AlgebraicTypeLayout {
        Product(ProductTypeLayout {
            layout: Layout { size: 96, align: 16 },
            elements: boxed![
                ProductTypeElementLayout {
                    offset: 0,
                    ty: Primitive(U32),
                    name: Some("sequence_id".into()),
                },
                ProductTypeElementLayout {
                    offset: 4,
                    ty: VarLen(String),
                    name: Some("sequence_name".into()),
                },
                ProductTypeElementLayout {
                    offset: 8,
                    ty: Primitive(U32),
                    name: Some("table_id".into()),
                },
                ProductTypeElementLayout {
                    offset: 12,
                    ty: Primitive(U16),
                    name: Some("col_pos".into()),
                },
                ProductTypeElementLayout {
                    offset: 16,
                    ty: Primitive(I128),
                    name: Some("increment".into()),
                },
                ProductTypeElementLayout {
                    offset: 32,
                    ty: Primitive(I128),
                    name: Some("start".into()),
                },
                ProductTypeElementLayout {
                    offset: 48,
                    ty: Primitive(I128),
                    name: Some("min_value".into()),
                },
                ProductTypeElementLayout {
                    offset: 64,
                    ty: Primitive(I128),
                    name: Some("max_value".into()),
                },
                ProductTypeElementLayout {
                    offset: 80,
                    ty: Primitive(I128),
                    name: Some("allocated".into()),
                },
            ],
        })
    }
}
impl HasOnePointOhLayout for StConstraintRow {
    fn get_original_layout() -> AlgebraicTypeLayout {
        Product(ProductTypeLayout {
            layout: Layout { size: 48, align: 16 },
            elements: boxed![
                ProductTypeElementLayout {
                    offset: 0,
                    ty: Primitive(U32),
                    name: Some("constraint_id".into()),
                },
                ProductTypeElementLayout {
                    offset: 4,
                    ty: VarLen(String),
                    name: Some("constraint_name".into()),
                },
                ProductTypeElementLayout {
                    offset: 8,
                    ty: Primitive(U32),
                    name: Some("table_id".into()),
                },
                ProductTypeElementLayout {
                    offset: 16,
                    ty: Sum(SumTypeLayout {
                        layout: Layout { size: 32, align: 16 },
                        variants: boxed![
                            SumTypeVariantLayout {
                                ty: Primitive(U128),
                                name: Some("Unused".into()),
                            },
                            SumTypeVariantLayout {
                                ty: col_list_layout(),
                                name: Some("Unique".into()),
                            },
                        ],
                        payload_offset: 16,
                    }),
                    name: Some("constraint_data".into()),
                },
            ],
        })
    }
}
impl HasOnePointOhLayout for StModuleRow {
    fn get_original_layout() -> AlgebraicTypeLayout {
        Product(ProductTypeLayout {
            layout: Layout { size: 22, align: 2 },
            elements: boxed![
                ProductTypeElementLayout {
                    offset: 0,
                    ty: bytes_layout(),
                    name: Some("database_address".into()),
                },
                ProductTypeElementLayout {
                    offset: 4,
                    ty: bytes_layout(),
                    name: Some("owner_identity".into()),
                },
                ProductTypeElementLayout {
                    offset: 8,
                    ty: Primitive(U8),
                    name: Some("program_kind".into()),
                },
                ProductTypeElementLayout {
                    offset: 10,
                    ty: bytes_layout(),
                    name: Some("program_hash".into()),
                },
                ProductTypeElementLayout {
                    offset: 14,
                    ty: bytes_layout(),
                    name: Some("program_bytes".into()),
                },
                ProductTypeElementLayout {
                    offset: 18,
                    ty: VarLen(String),
                    name: Some("module_version".into()),
                },
            ],
        })
    }
}
impl HasOnePointOhLayout for StClientRow {
    fn get_original_layout() -> AlgebraicTypeLayout {
        Product(ProductTypeLayout {
            layout: Layout { size: 8, align: 2 },
            elements: boxed![
                ProductTypeElementLayout {
                    offset: 0,
                    ty: bytes_layout(),
                    name: Some("identity".into()),
                },
                ProductTypeElementLayout {
                    offset: 4,
                    ty: bytes_layout(),
                    name: Some("address".into()),
                },
            ],
        })
    }
}
impl HasOnePointOhLayout for StVarRow {
    fn get_original_layout() -> AlgebraicTypeLayout {
        Product(ProductTypeLayout {
            layout: Layout { size: 48, align: 16 },
            elements: boxed![
                ProductTypeElementLayout {
                    offset: 0,
                    ty: VarLen(String),
                    name: Some("name".into()),
                },
                ProductTypeElementLayout {
                    offset: 16,
                    ty: Sum(SumTypeLayout {
                        layout: Layout { size: 32, align: 16 },
                        variants: boxed![
                            SumTypeVariantLayout {
                                ty: Primitive(Bool),
                                name: Some("Bool".into()),
                            },
                            SumTypeVariantLayout {
                                ty: Primitive(I8),
                                name: Some("I8".into()),
                            },
                            SumTypeVariantLayout {
                                ty: Primitive(U8),
                                name: Some("U8".into()),
                            },
                            SumTypeVariantLayout {
                                ty: Primitive(I16),
                                name: Some("I16".into()),
                            },
                            SumTypeVariantLayout {
                                ty: Primitive(U16),
                                name: Some("U16".into()),
                            },
                            SumTypeVariantLayout {
                                ty: Primitive(I32),
                                name: Some("I32".into()),
                            },
                            SumTypeVariantLayout {
                                ty: Primitive(U32),
                                name: Some("U32".into()),
                            },
                            SumTypeVariantLayout {
                                ty: Primitive(I64),
                                name: Some("I64".into()),
                            },
                            SumTypeVariantLayout {
                                ty: Primitive(U64),
                                name: Some("U64".into()),
                            },
                            SumTypeVariantLayout {
                                ty: Primitive(I128),
                                name: Some("I128".into()),
                            },
                            SumTypeVariantLayout {
                                ty: Primitive(U128),
                                name: Some("U128".into()),
                            },
                            SumTypeVariantLayout {
                                ty: Primitive(F32),
                                name: Some("F32".into()),
                            },
                            SumTypeVariantLayout {
                                ty: Primitive(F64),
                                name: Some("F64".into()),
                            },
                            SumTypeVariantLayout {
                                ty: VarLen(String),
                                name: Some("String".into()),
                            },
                        ],
                        payload_offset: 16,
                    }),
                    name: Some("value".into()),
                },
            ],
        })
    }
}
impl HasOnePointOhLayout for StScheduledRow {
    fn get_original_layout() -> AlgebraicTypeLayout {
        Product(ProductTypeLayout {
            layout: Layout { size: 16, align: 4 },
            elements: boxed![
                ProductTypeElementLayout {
                    offset: 0,
                    ty: Primitive(U32),
                    name: Some("schedule_id".into()),
                },
                ProductTypeElementLayout {
                    offset: 4,
                    ty: Primitive(U32),
                    name: Some("table_id".into()),
                },
                ProductTypeElementLayout {
                    offset: 8,
                    ty: VarLen(String),
                    name: Some("reducer_name".into()),
                },
                ProductTypeElementLayout {
                    offset: 12,
                    ty: VarLen(String),
                    name: Some("schedule_name".into()),
                },
            ],
        })
    }
}
