// () -> Tuple or enum?
// (0: 1) -> Tuple or enum?
// (0: 1, x: (1: 2 | 0: 2))
// (0: 1 | 1: 2)

// Types
// () -> 0-tuple or void?
// (0: u32) -> 1-tuple or 1-enum or monuple?
// (0: u32, 1: (0: 1 | 0: 2)) -> 2-tuple with enum for second type
// (0: 1 | 0: 2) -> 2-enum

// Proposed Types?
// () -> 0-tuple (either + or * operator)
// (1: u32) -> 1-tuple (either + or * operator)
// (1: u32, 2: u32) -> 2-tuple (* operator)
// (1: u32 | 2: u32) -> 2-tuple (+ operator)

#[derive(Debug, Clone)]
pub struct ElementDef {
    // In the case of tuples, this is the id of the column
    // In the case of enums, this is the id of the variant
    pub tag: u8,
    // TODO: Allow named elements? Probably need for SQL and nice for JSON
    // slow though so need to be careful
    // pub name: Option<String>,
    pub element_type: TypeDef,
}

impl ElementDef {
    pub fn decode(bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        let tag = bytes[num_read];
        num_read += 1;

        let (element_type, nr) = TypeDef::decode(&bytes[num_read..]);
        num_read += nr;

        (ElementDef { tag, element_type }, num_read)
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(self.tag);
        self.element_type.encode(bytes);
    }
}

#[derive(Debug, Clone)]
pub struct TupleDef {
    pub elements: Vec<ElementDef>,
}

impl TupleDef {
    pub fn decode(bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        let len = bytes[num_read];
        num_read += 1;

        let mut elements = Vec::new();
        for _ in 0..len {
            let (element, nr) = ElementDef::decode(&bytes[num_read..]);
            elements.push(element);
            num_read += nr;
        }
        (TupleDef { elements }, num_read)
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(self.elements.len() as u8);
        for item in &self.elements {
            item.encode(bytes);
        }
    }
}

// TODO: probably implement this with a tuple but store whether the tuple
// is a sum tuple or a product tuple, then we have uniformity over types
#[derive(Debug, Clone)]
pub struct EnumDef {
    pub elements: Vec<ElementDef>,
}

impl EnumDef {
    pub fn decode(bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let mut num_read = 0;
        let bytes = bytes.as_ref();
        let len = bytes[num_read];
        num_read += 1;

        let mut items = Vec::new();
        for _ in 0..len {
            let (item, nr) = ElementDef::decode(&bytes[num_read..]);
            items.push(item);
            num_read += nr;
        }
        (EnumDef { elements: items }, num_read)
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(self.elements.len() as u8);
        for item in &self.elements {
            item.encode(bytes);
        }
    }
}

#[derive(Debug, Clone)]
pub enum TypeDef {
    Tuple(TupleDef),
    Enum(EnumDef),

    // base types
    Vec { element_type: Box<TypeDef> },
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
    F32,
    F64,
    String,
    Bytes,
    Unit,
}

impl TypeDef {
    pub fn decode(bytes: impl AsRef<[u8]>) -> (Self, usize) {
        let bytes = bytes.as_ref();
        match bytes[0] {
            0 => {
                let (tuple_def, bytes_read) = TupleDef::decode(&bytes[1..]);
                (TypeDef::Tuple(tuple_def), bytes_read + 1)
            }
            1 => {
                let (enum_def, bytes_read) = EnumDef::decode(&bytes[1..]);
                (TypeDef::Enum(enum_def), bytes_read + 1)
            }
            2 => {
                let (type_def, bytes_read) = TypeDef::decode(&bytes[1..]);
                (
                    TypeDef::Vec {
                        element_type: Box::new(type_def),
                    },
                    bytes_read + 1,
                )
            }
            3 => (TypeDef::U8, 1),
            4 => (TypeDef::U16, 1),
            5 => (TypeDef::U32, 1),
            6 => (TypeDef::U64, 1),
            7 => (TypeDef::U128, 1),
            8 => (TypeDef::I8, 1),
            9 => (TypeDef::I16, 1),
            10 => (TypeDef::I32, 1),
            11 => (TypeDef::I64, 1),
            12 => (TypeDef::I128, 1),
            13 => (TypeDef::Bool, 1),
            14 => (TypeDef::F32, 1),
            15 => (TypeDef::F64, 1),
            16 => (TypeDef::String, 1),
            17 => (TypeDef::Bytes, 1),
            18 => (TypeDef::Bytes, 1),
            b => panic!("Unknown {}", b),
        }
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        match self {
            TypeDef::Tuple(t) => {
                bytes.push(0);
                t.encode(bytes);
            }
            TypeDef::Enum(e) => {
                bytes.push(1);
                e.encode(bytes);
            }
            TypeDef::Vec { element_type } => {
                bytes.push(2);
                element_type.encode(bytes);
            }
            TypeDef::U8 => bytes.push(3),
            TypeDef::U16 => bytes.push(4),
            TypeDef::U32 => bytes.push(5),
            TypeDef::U64 => bytes.push(6),
            TypeDef::U128 => bytes.push(7),
            TypeDef::I8 => bytes.push(8),
            TypeDef::I16 => bytes.push(9),
            TypeDef::I32 => bytes.push(10),
            TypeDef::I64 => bytes.push(11),
            TypeDef::I128 => bytes.push(12),
            TypeDef::Bool => bytes.push(13),
            TypeDef::F32 => bytes.push(14),
            TypeDef::F64 => bytes.push(15),
            TypeDef::String => bytes.push(16),
            TypeDef::Bytes => bytes.push(17),
            TypeDef::Unit => bytes.push(18),
        }
    }
}
