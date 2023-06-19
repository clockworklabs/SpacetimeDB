use crate::{
    AlgebraicType, AlgebraicTypeRef, ArrayType, BuiltinType, MapType, ProductType, ProductTypeElement, SumType,
    SumTypeVariant, TypeInSpace,
};

#[derive(Default)]
pub struct ResolveRefState {
    stack: Vec<AlgebraicTypeRef>,
}

pub trait ResolveRefs {
    type Output;
    fn resolve_refs(this: TypeInSpace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output>;
}

impl ResolveRefs for AlgebraicType {
    type Output = Self;
    fn resolve_refs(this: TypeInSpace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        match this.ty() {
            AlgebraicType::Sum(sum) => this.with(sum)._resolve_refs(state).map(Self::Sum),
            AlgebraicType::Product(prod) => this.with(prod)._resolve_refs(state).map(Self::Product),
            AlgebraicType::Builtin(b) => this.with(b)._resolve_refs(state).map(Self::Builtin),
            AlgebraicType::Ref(r) => this.with(r)._resolve_refs(state),
        }
    }
}
impl ResolveRefs for BuiltinType {
    type Output = Self;
    fn resolve_refs(this: TypeInSpace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        match this.ty() {
            BuiltinType::Array(ty) => this.with(ty)._resolve_refs(state).map(Self::Array),
            BuiltinType::Map(m) => this.with(m)._resolve_refs(state).map(Self::Map),
            x => Some(x.clone()),
        }
    }
}
impl ResolveRefs for ArrayType {
    type Output = ArrayType;
    fn resolve_refs(this: TypeInSpace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        Some(ArrayType {
            elem_ty: Box::new(this.map(|m| &*m.elem_ty)._resolve_refs(state)?),
        })
    }
}
impl ResolveRefs for MapType {
    type Output = MapType;
    fn resolve_refs(this: TypeInSpace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        Some(MapType {
            key_ty: Box::new(this.map(|m| &*m.key_ty)._resolve_refs(state)?),
            ty: Box::new(this.map(|m| &*m.ty)._resolve_refs(state)?),
        })
    }
}
impl ResolveRefs for ProductType {
    type Output = Self;
    fn resolve_refs(this: TypeInSpace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        let elements = this
            .ty()
            .elements
            .iter()
            .map(|el| this.with(el)._resolve_refs(state))
            .collect::<Option<_>>()?;
        Some(ProductType { elements })
    }
}
impl ResolveRefs for ProductTypeElement {
    type Output = Self;
    fn resolve_refs(this: TypeInSpace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        Some(ProductTypeElement {
            algebraic_type: this.map(|e| &e.algebraic_type)._resolve_refs(state)?,
            name: this.ty().name.clone(),
        })
    }
}
impl ResolveRefs for SumType {
    type Output = Self;
    fn resolve_refs(this: TypeInSpace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        let variants = this
            .ty()
            .variants
            .iter()
            .map(|v| this.with(v)._resolve_refs(state))
            .collect::<Option<_>>()?;
        Some(SumType { variants })
    }
}
impl ResolveRefs for SumTypeVariant {
    type Output = Self;
    fn resolve_refs(this: TypeInSpace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        Some(SumTypeVariant {
            algebraic_type: this.map(|v| &v.algebraic_type)._resolve_refs(state)?,
            name: this.ty().name.clone(),
        })
    }
}
impl ResolveRefs for AlgebraicTypeRef {
    type Output = AlgebraicType;
    fn resolve_refs(this: TypeInSpace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        if state.stack.contains(this.ty()) {
            return None;
        }
        state.stack.push(*this.ty());
        let ret = this
            .typespace()
            .get(*this.ty())
            .and_then(|ty| this.with(ty)._resolve_refs(state));
        state.stack.pop();
        ret
    }
}

impl<T: ResolveRefs> TypeInSpace<'_, T> {
    pub fn resolve_refs(self) -> Option<T::Output> {
        T::resolve_refs(self, &mut ResolveRefState::default())
    }
    fn _resolve_refs(self, state: &mut ResolveRefState) -> Option<T::Output> {
        T::resolve_refs(self, state)
    }
}
