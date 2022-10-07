use std::collections::HashMap;

use super::*;

mod sealed {
    pub trait Sealed {}
}
pub trait RefKind: Clone + sealed::Sealed {
    fn resolve(&self, ctx: &mut ResolveContext<'_>) -> TypeDef;
    fn as_typeref(&self) -> &TypeRef;
    fn from_typeref(r: TypeRef) -> Option<Self>;
}

#[derive(Clone, Copy, Debug)]
pub enum Void {}
impl sealed::Sealed for Void {}
impl RefKind for Void {
    fn resolve(&self, _: &mut ResolveContext<'_>) -> TypeDef {
        match *self {}
    }

    fn as_typeref(&self) -> &TypeRef {
        match *self {}
    }

    fn from_typeref(_: TypeRef) -> Option<Self> {
        None
    }
}

#[derive(Clone, Debug)]
pub struct TypeRef {
    pub name: String,
}
impl sealed::Sealed for TypeRef {}
impl RefKind for TypeRef {
    fn resolve(&self, ctx: &mut ResolveContext<'_>) -> TypeDef {
        ctx.resolve_name(&self.name)
    }

    fn as_typeref(&self) -> &TypeRef {
        self
    }

    fn from_typeref(r: TypeRef) -> Option<Self> {
        Some(r)
    }
}

impl TypeRef {
    pub(super) fn decode(b: impl AsRef<[u8]>) -> (Result<Self, String>, usize) {
        let b = b.as_ref();
        let len: usize = b[0].into();
        match std::str::from_utf8(&b[1..][..len]) {
            Ok(s) => (Ok(Self { name: s.to_owned() }), 1 + len),
            Err(_) => (Err("invalid utf8".into()), 0),
        }
    }
    pub(super) fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.push(0xff);
        bytes.push(self.name.len().try_into().unwrap());
        bytes.extend_from_slice(self.name.as_bytes());
    }
}

pub struct ResolveContext<'a> {
    types: &'a HashMap<String, TypeDef<TypeRef>>,
    stack: Vec<String>,
}
impl<'a> ResolveContext<'a> {
    pub fn new(types: &'a HashMap<String, TypeDef<TypeRef>>) -> Self {
        ResolveContext {
            types,
            stack: Vec::new(),
        }
    }
    pub fn resolve_name(&mut self, name: &str) -> TypeDef {
        let string = name.to_owned();
        if self.stack.contains(&string) {
            panic!("circular types: {} contains {}", self.stack.join(" contains "), name);
        }
        self.stack.push(string);
        let ret = self
            .types
            .get(name)
            .unwrap_or_else(|| panic!("couldn't resolve type {}", name))
            .resolve(self);
        self.stack.pop();
        ret
    }
    pub fn get_type(&self, name: &str) -> Option<&TypeDef<TypeRef>> {
        self.types.get(name)
    }
}

impl TypeDef<TypeRef> {
    pub fn resolve_refs(&self, types: &HashMap<String, Self>) -> TypeDef {
        self.resolve(&mut ResolveContext::new(types))
    }
    fn resolve(&self, ctx: &mut ResolveContext) -> TypeDef {
        self.map_refkind_ref(&mut |r| ctx.resolve_name(&r.name))
    }
}
impl TypeDef {
    pub fn refify(self) -> TypeDef<TypeRef> {
        self.map_refkind(&mut |x| match x {})
    }
}

impl<Ref: RefKind> ElementDef<Ref> {
    pub fn map_refkind<U: RefKind>(self, f: &mut impl FnMut(Ref) -> TypeDef<U>) -> ElementDef<U> {
        ElementDef {
            tag: self.tag,
            name: self.name,
            element_type: self.element_type.map_refkind(f),
        }
    }
    pub fn map_refkind_ref<U: RefKind>(&self, f: &mut impl FnMut(&Ref) -> TypeDef<U>) -> ElementDef<U> {
        ElementDef {
            tag: self.tag,
            name: self.name.clone(),
            element_type: self.element_type.map_refkind_ref(f),
        }
    }
}
impl<Ref: RefKind> TupleDef<Ref> {
    pub fn map_refkind<U: RefKind>(self, f: &mut impl FnMut(Ref) -> TypeDef<U>) -> TupleDef<U> {
        TupleDef {
            elements: self.elements.into_iter().map(|elem| elem.map_refkind(f)).collect(),
        }
    }
    pub fn map_refkind_ref<U: RefKind>(&self, f: &mut impl FnMut(&Ref) -> TypeDef<U>) -> TupleDef<U> {
        TupleDef {
            elements: self.elements.iter().map(|elem| elem.map_refkind_ref(f)).collect(),
        }
    }
}
impl<Ref: RefKind> EnumDef<Ref> {
    pub fn map_refkind<U: RefKind>(self, f: &mut impl FnMut(Ref) -> TypeDef<U>) -> EnumDef<U> {
        EnumDef {
            elements: self.elements.into_iter().map(|elem| elem.map_refkind(f)).collect(),
        }
    }
    pub fn map_refkind_ref<U: RefKind>(&self, f: &mut impl FnMut(&Ref) -> TypeDef<U>) -> EnumDef<U> {
        EnumDef {
            elements: self.elements.iter().map(|elem| elem.map_refkind_ref(f)).collect(),
        }
    }
}
impl<Ref: RefKind> TypeDef<Ref> {
    pub fn map_refkind<U: RefKind>(self, f: &mut impl FnMut(Ref) -> TypeDef<U>) -> TypeDef<U> {
        match self {
            TypeDef::Tuple(tup) => TypeDef::Tuple(tup.map_refkind(f)),
            TypeDef::Enum(enu) => TypeDef::Enum(enu.map_refkind(f)),
            TypeDef::Vec { element_type } => TypeDef::Vec {
                element_type: Box::new(element_type.map_refkind(f)),
            },
            TypeDef::Primitive(prim) => TypeDef::Primitive(prim),
            TypeDef::Ref(r) => f(r),
        }
    }
    pub fn map_refkind_ref<U: RefKind>(&self, f: &mut impl FnMut(&Ref) -> TypeDef<U>) -> TypeDef<U> {
        match self {
            TypeDef::Tuple(tup) => TypeDef::Tuple(tup.map_refkind_ref(f)),
            TypeDef::Enum(enu) => TypeDef::Enum(enu.map_refkind_ref(f)),
            TypeDef::Vec { element_type } => TypeDef::Vec {
                element_type: Box::new(element_type.map_refkind_ref(f)),
            },
            TypeDef::Primitive(prim) => TypeDef::Primitive(*prim),
            TypeDef::Ref(r) => f(r),
        }
    }
}
