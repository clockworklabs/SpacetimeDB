use spacetimedb_sats::AlgebraicType;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Param {
    pub(crate) name: String,
    pub(crate) kind: AlgebraicType,
}

impl Param {
    pub fn new(name: &str, kind: AlgebraicType) -> Self {
        Self {
            name: name.to_string(),
            kind,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FunDef {
    pub name: String,
    pub params: Vec<Param>,
    pub result: AlgebraicType,
}

impl FunDef {
    pub fn new(name: &str, params: &[Param], result: AlgebraicType) -> Self {
        Self {
            name: name.into(),
            params: params.into(),
            result,
        }
    }
}
