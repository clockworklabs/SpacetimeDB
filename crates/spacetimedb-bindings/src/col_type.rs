#[derive(PartialEq, Debug)]
pub enum ColType {
    U8,
    U16,
    U32,
    U64,
    U128,
    I8,
    I16,
    I32,
    I64,
    I128,
    Bool,
    // F32,
    // F64,
}

impl ColType {
    pub fn size(&self) -> u8 {
        match self {
            ColType::U8 => 1,
            ColType::U16 => 2,
            ColType::U32 => 4,
            ColType::U64 => 8,
            ColType::U128 => 16,
            ColType::I8 => 1,
            ColType::I16 => 2,
            ColType::I32 => 4,
            ColType::I64 => 8,
            ColType::I128 => 16,
            ColType::Bool => 1,
            // ColType::F32 => 4,
            // ColType::F64 => 8,
        }
    }

    pub fn to_u32(&self) -> u32 {
        match self {
            ColType::U8 => 1,
            ColType::U16 => 2,
            ColType::U32 => 3,
            ColType::U64 => 4,
            ColType::U128 => 5,
            ColType::I8 => 6,
            ColType::I16 => 7,
            ColType::I32 => 8,
            ColType::I64 => 9,
            ColType::I128 => 10,
            ColType::Bool => 11,
            // ColType::F32 => 4,
            // ColType::F64 => 8,
        }
    }

    pub fn from_u32(value: u32) -> Self {
        match value {
            1 => Self::U8,
            2 => Self::U16,
            3 => Self::U32,
            4 => Self::U64,
            5 => Self::U128,
            6 => Self::I8,
            7 => Self::I16,
            8 => Self::I32,
            9 => Self::I64,
            10 => Self::I128,
            11 => Self::Bool,
            _ => panic!("Unknown value: {}", value),
        }
    }
}
