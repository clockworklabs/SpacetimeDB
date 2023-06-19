use crate::errors::{ErrorKind, ErrorLang};
use crate::expr::{Code, FunctionId};
use crate::program::{ProgramRef, ProgramVm};
use spacetimedb_sats::algebraic_type::AlgebraicType;
use spacetimedb_sats::algebraic_value::AlgebraicValue;
use std::fmt;

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

pub trait FunVM: for<'a> Fn(ProgramRef<'a>, Args<'a>) -> Code {
    fn clone_object(&self) -> Box<dyn FunVM>;
}

impl<F> FunVM for F
where
    F: 'static + Clone + for<'a> Fn(ProgramRef<'a>, Args<'a>) -> Code,
{
    fn clone_object(&self) -> Box<dyn FunVM> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn FunVM> {
    fn clone(&self) -> Self {
        self.clone_object()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Args<'a> {
    Unary(&'a AlgebraicValue),
    Binary(&'a AlgebraicValue, &'a AlgebraicValue),
    Splat(&'a [AlgebraicValue]),
}

impl<'a> Args<'a> {
    pub fn param(&self, pos: usize) -> Option<&'a AlgebraicValue> {
        match self {
            Args::Unary(x) => {
                if pos == 0 {
                    Some(x)
                } else {
                    None
                }
            }
            Args::Binary(a, b) => match pos {
                0 => Some(a),
                1 => Some(b),
                _ => None,
            },
            Args::Splat(x) => x.get(pos),
        }
    }

    pub fn param_extract<F, T>(&self, pos: usize, f: F) -> Result<&'a T, ErrorLang>
    where
        F: Fn(&'a AlgebraicValue) -> Option<&'a T>,
    {
        if let Some(x) = self.param(pos) {
            if let Some(x) = f(x) {
                Ok(x)
            } else {
                Err(ErrorLang::new(ErrorKind::Params, Some("Param {0} invalid type.")))
            }
        } else {
            Err(ErrorLang::new(ErrorKind::Params, Some("Param {0} not found.")))
        }
    }
}

#[derive(Clone)]
pub struct FunVm {
    pub(crate) name: String,
    pub(crate) idx: FunctionId,
    pub(crate) fun: Box<dyn FunVM>,
}

impl FunVm {
    pub fn new(name: &str, idx: FunctionId, fun: Box<dyn FunVM>) -> Self {
        Self {
            name: name.to_string(),
            idx,
            fun,
        }
    }
    pub fn call<'a, P: ProgramVm>(&self, p: &'a P, args: Args<'a>) -> Code {
        (self.fun)(p.as_program_ref(), args)
    }
}

impl fmt::Debug for FunVm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "fun {}({})", self.name, self.idx)
    }
}

#[derive(Debug, Clone)]
pub struct Lambda {
    #[allow(dead_code)]
    pub(crate) head: FunDef,
    pub(crate) body: Code,
}
