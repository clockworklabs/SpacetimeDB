use crate::{
    AlgebraicType, AlgebraicTypeRef, ArrayType, BuiltinType, MapType, ProductType, ProductTypeElement, SumType,
    SumTypeVariant, WithTypespace,
};

/// Resolver for [`AlgebraicTypeRef`]s within a structure.
#[derive(Default)]
pub struct ResolveRefState {
    /// The stack used to handle cycle detection for [recursive types] (`μα. T`).
    ///
    /// [recursive types]: https://en.wikipedia.org/wiki/Recursive_data_type#Theory
    stack: Vec<AlgebraicTypeRef>,
}

/// A trait for types that know how to resolve their [`AlgebraicTypeRef`]s
/// provided a typing context and the resolver `state`.
pub trait ResolveRefs {
    /// Output type after type references have been resolved.
    type Output;

    /// Returns, if possible, an output with all [`AlgebraicTypeRef`]s
    /// within `this` (typing context carried) resolved
    /// using the provided resolver `state`.
    ///
    /// `None` is only returned if there were cycles in the precense of recursive μ-types.
    fn resolve_refs(this: WithTypespace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output>;
}

// -----------------------------------------------------------------------------
// The interesting logic:
// -----------------------------------------------------------------------------

impl ResolveRefs for AlgebraicTypeRef {
    type Output = AlgebraicType;
    fn resolve_refs(this: WithTypespace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        // Suppose we have `&0 = { Nil, Cons({ elem: U8, tail: &0 }) }`.
        // This is our standard cons-list type.
        // In this setup, when getting to `tail`,
        // we would recurse back to expanding `tail` again, and so or...
        // So we will never halt. This check breaks that cycle.
        if state.stack.contains(this.ty()) {
            return None;
        }

        // Push ourselves to the stack.
        state.stack.push(*this.ty());

        // Extract the `at: AlgebraicType` pointed to by `this` and then resolve `at`.
        let ret = this
            .typespace()
            .get(*this.ty())
            .and_then(|at| this.with(at)._resolve_refs(state));

        // Remove ourselves.
        state.stack.pop();
        ret
    }
}

// -----------------------------------------------------------------------------
// All the below is just plumbing:
// -----------------------------------------------------------------------------

impl ResolveRefs for AlgebraicType {
    type Output = Self;
    fn resolve_refs(this: WithTypespace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        match this.ty() {
            Self::Ref(r) => this.with(r)._resolve_refs(state),
            Self::Sum(sum) => this.with(sum)._resolve_refs(state).map(Into::into),
            Self::Product(prod) => this.with(prod)._resolve_refs(state).map(Into::into),
            Self::Builtin(BuiltinType::Array(ty)) => this.with(ty)._resolve_refs(state).map(Into::into),
            Self::Builtin(BuiltinType::Map(m)) => this.with(&**m)._resolve_refs(state).map(Into::into),
            // These types are plain and cannot have refs in them.
            x => Some(x.clone()),
        }
    }
}

impl ResolveRefs for ArrayType {
    type Output = Self;
    fn resolve_refs(this: WithTypespace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        Some(Self {
            elem_ty: Box::new(this.map(|m| &*m.elem_ty)._resolve_refs(state)?),
        })
    }
}

impl ResolveRefs for MapType {
    type Output = Self;
    fn resolve_refs(this: WithTypespace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        Some(Self {
            key_ty: this.map(|m| &m.key_ty)._resolve_refs(state)?,
            ty: this.map(|m| &m.ty)._resolve_refs(state)?,
        })
    }
}

impl ResolveRefs for ProductType {
    type Output = Self;
    fn resolve_refs(this: WithTypespace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
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
    fn resolve_refs(this: WithTypespace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        Some(Self {
            algebraic_type: this.map(|e| &e.algebraic_type)._resolve_refs(state)?,
            name: this.ty().name.clone(),
        })
    }
}

impl ResolveRefs for SumType {
    type Output = Self;
    fn resolve_refs(this: WithTypespace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        let variants = this
            .ty()
            .variants
            .iter()
            .map(|v| this.with(v)._resolve_refs(state))
            .collect::<Option<Vec<_>>>()?;
        Some(Self { variants })
    }
}

impl ResolveRefs for SumTypeVariant {
    type Output = Self;
    fn resolve_refs(this: WithTypespace<'_, Self>, state: &mut ResolveRefState) -> Option<Self::Output> {
        Some(Self {
            algebraic_type: this.map(|v| &v.algebraic_type)._resolve_refs(state)?,
            name: this.ty().name.clone(),
        })
    }
}

impl<T: ResolveRefs> WithTypespace<'_, T> {
    pub fn resolve_refs(self) -> Option<T::Output> {
        T::resolve_refs(self, &mut ResolveRefState::default())
    }
    fn _resolve_refs(self, state: &mut ResolveRefState) -> Option<T::Output> {
        T::resolve_refs(self, state)
    }
}
