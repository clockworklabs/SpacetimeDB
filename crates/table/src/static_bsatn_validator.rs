//! To efficiently implement a fast-path BSATN -> BFLATN,
//! we use a `StaticLayout` but in reverse of the read path.
//! This however leaves us with no way to validate
//! that the BSATN satisfies the row type of a given table.
//!
//! More specifically, we must validate that:
//! 1. The length of the BSATN-encoded row matches the expected length.
//! 2. All `bool`s in the row type only receive the values 0 or 1.
//! 3. All sum tags are valid.
//! 4. a sum's payload follows 2-3 recursively.
//!
//! That is where this module comes in,
//! which provides two functions:
//! - [`static_bsatn_validator`], which compiles a validator program given the table's row type.
//! - [`validate_bsatn`], which executes the validator program against a row encoded in BSATN.
//!
//! The compilation uses the same strategy as for row type visitors,
//! first simplifying to a rose-tree and then flattening that to
//! a simple forward-progress-only byte code instruction set.

#![allow(unused)]

use crate::layout::ProductTypeLayoutView;

use super::{
    layout::{AlgebraicTypeLayout, HasLayout as _, ProductTypeLayout, RowTypeLayout},
    static_layout::StaticLayout,
    MemoryUsage,
};
use itertools::{repeat_n, Itertools as _};
use spacetimedb_sats::bsatn::DecodeError;
use spacetimedb_schema::type_for_generate::PrimitiveType;
use std::sync::Arc;

/// Constructs a validator for a row encoded in BSATN
/// that checks that the row satisfies the type `ty`
/// when `ty` has `StaticLayout`.
///
/// This is a potentially expensive operation,
/// so the resulting `StaticBsatnValidator` should be stored and re-used.
pub(crate) fn static_bsatn_validator(ty: &RowTypeLayout) -> StaticBsatnValidator {
    let tree = row_type_to_tree(ty.product());
    let insns = tree_to_insns(&tree).into();
    StaticBsatnValidator { insns }
}

/// Construct a `Tree` from `ty`.
///
/// See [`extend_trees_for_algebraic_type`] for more details.
fn row_type_to_tree(ty: ProductTypeLayoutView<'_>) -> Tree {
    let mut sub_trees = Vec::new();
    extend_trees_for_product_type(ty, &mut 0, &mut sub_trees);
    sub_trees_to_tree(sub_trees)
}

/// Convert a list of `sub_trees` to one tree.
fn sub_trees_to_tree(mut sub_trees: Vec<Tree>) -> Tree {
    match sub_trees.len() {
        // No trees is `Empty`.
        0 => Tree::Empty,
        // A single subtree can be collapsed.
        // so prune the intermediate node.
        1 => sub_trees.pop().unwrap(),
        // For more than one children, sequence them doing one after the other.
        _ => Tree::Sequence { sub_trees },
    }
}

/// Extend `sub_trees` with checks for `ty`.
///
/// See [`extend_trees_for_algebraic_type`] for more details.
fn extend_trees_for_product_type(ty: ProductTypeLayoutView<'_>, current_offset: &mut usize, sub_trees: &mut Vec<Tree>) {
    for elem in ty.elements {
        extend_trees_for_algebraic_type(&elem.ty, current_offset, sub_trees);
    }
}

/// Extend `sub_trees` with checks for `ty`.
///
/// `current_offset` should be passed as `&mut 0` upon entry to the row-type,
/// and will be incremented as appropriate during recursive traversal
/// to track the offset in bytes of the member currently being visited.
fn extend_trees_for_algebraic_type(ty: &AlgebraicTypeLayout, current_offset: &mut usize, sub_trees: &mut Vec<Tree>) {
    match ty {
        AlgebraicTypeLayout::Primitive(PrimitiveType::Bool) => {
            // The `Bool` type is special, as it only allows a BSATN byte to be 0 or 1.
            let offset = *current_offset as u16;
            *current_offset += 1;
            sub_trees.push(Tree::CheckBool { offset });
        }
        AlgebraicTypeLayout::Primitive(prim_ty) => {
            // For primitive types, increment `current_offset` past this member.
            // Primitive types have no padding, so we can use `prim_ty.size()` for bsatn.
            *current_offset += prim_ty.size();
        }
        AlgebraicTypeLayout::Product(prod_ty) => {
            extend_trees_for_product_type(prod_ty.view(), current_offset, sub_trees)
        }
        AlgebraicTypeLayout::Sum(sum_ty) => {
            // Record the tag's offset and the number of variants.
            let num_variants = sum_ty.variants.len() as u8;
            let tag_offset = *current_offset as u16;
            *current_offset += 1;

            // For each variant, collect that variant's sub-tree.
            // All variants are stored overlapping at the offset of the sum
            // so we must reset `current_offset` each time to the before-variant value.
            // We also need to create a fresh `sub_tree` context.
            // Note that BSATN stores sums with tag first,
            // followed by data/payload.
            let mut child_offset = *current_offset;
            let mut variants = sum_ty
                .variants
                .iter()
                .map(|variant| {
                    let var_ty = &variant.ty;
                    let mut sub_trees = Vec::new();
                    child_offset = *current_offset;
                    extend_trees_for_algebraic_type(var_ty, &mut child_offset, &mut sub_trees);
                    sub_trees_to_tree(sub_trees)
                })
                .collect::<Vec<_>>();
            // Having dealt with all variants,
            // we must now move `current_offset` forward to the size of the payload
            // which we know to be same for all variants.
            *current_offset = child_offset;

            if variants.iter().all_equal() {
                // When all variants have the same set checks,
                // there's no need to switch on the tag, so prune the intermediate node.
                // A special case of this is single-variant sums.
                sub_trees.push(Tree::CheckTag {
                    tag_offset,
                    num_variants,
                });
                if let Some(tree) = variants.pop() {
                    sub_trees.push(tree);
                }
            } else {
                sub_trees.push(Tree::Sum {
                    tag_offset,
                    tag_data_processors: variants,
                });
            }
        }

        // There are no var-len members when there's a static fixed bsatn length.
        AlgebraicTypeLayout::VarLen(_) => unreachable!(),
    }
}

/// A [Rose Tree](https://en.wikipedia.org/wiki/Rose_tree)
/// containing information about validation steps for
/// decoding BSATN for statically known fixed size `AlgebraicType`s.
#[derive(Debug, PartialEq, Eq)]
enum Tree {
    /// Nothing to check.
    Empty,

    /// Do each sub-tree after each other.
    Sequence { sub_trees: Vec<Tree> },

    /// Check a byte at `start + N` bytes to be a valid `bool`.
    CheckBool { offset: u16 },

    /// Check a byte at `start + N` bytes to be `< num_variants`.
    CheckTag {
        /// The sum's tag is at `row + tag_offset` bytes.
        tag_offset: u16,
        /// The number of variants there are.
        /// The read tag must be `< num_variants`.
        num_variants: u8,
    },

    /// A choice between several variants.
    Sum {
        /// The sum's tag is at `row + tag_offset` bytes.
        tag_offset: u16,
        /// The checks for variant `N` are described in `tag_data_processors[N]`.
        tag_data_processors: Vec<Tree>,
    },
}

/// Compile the [`Tree`] to a list of [`Insn`].
fn tree_to_insns(tree: &Tree) -> Vec<Insn> {
    let mut program = Vec::new();

    fn compile_tree(tree: &Tree, into: &mut Vec<Insn>) {
        match tree {
            Tree::Empty => {}
            &Tree::CheckBool { offset } => into.push(Insn::CheckBool(offset)),
            Tree::Sequence { sub_trees } => {
                for tree in &**sub_trees {
                    compile_tree(tree, into);
                }
            }
            &Tree::CheckTag {
                tag_offset,
                num_variants,
            } => into.push(Insn::CheckTag(CheckTag {
                tag_offset,
                num_variants,
            })),
            Tree::Sum {
                tag_offset,
                tag_data_processors,
            } => {
                // Add the branching instruction itself.
                let num_variants = tag_data_processors.len();
                into.push(Insn::CheckReadTagRelBranch(CheckTag {
                    tag_offset: *tag_offset,
                    num_variants: num_variants as u8,
                }));
                // Add N slots for "to variant goto"s.
                let to_branches = into.len();
                into.extend(repeat_n(Insn::FIXUP, num_variants));
                // Compile the branches.
                let mut from_variant_gotos = Vec::with_capacity(num_variants);
                for (tag, branch) in tag_data_processors.iter().enumerate() {
                    // Fixup the to-variant jump address.
                    into[to_branches + tag] = Insn::Goto(into.len() as u16);
                    // Compile the branch.
                    compile_tree(branch, into);
                    // Add jump-out gotos that we'll fixup later to store the after-sum address.
                    from_variant_gotos.push(into.len());
                    into.push(Insn::FIXUP);
                }
                // Fixup the jump-out-from-variant addresses.
                let goto_addr = into.len();
                for idx in from_variant_gotos {
                    into[idx] = Insn::Goto(goto_addr as u16);
                }
            }
        }
    }

    compile_tree(tree, &mut program);
    remove_trailing_gotos(&mut program);
    program
}

/// Remove any trailing gotos.
///
/// They are not needed as they will only go towards the end,
/// so we can just cut them out.
fn remove_trailing_gotos(program: &mut Vec<Insn>) {
    for idx in (0..program.len()).rev() {
        match program[idx] {
            Insn::Goto(_) => program.pop(),
            _ => break,
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CheckTag {
    /// The tag to check is stored at `start + tag_offset`.
    tag_offset: u16,
    /// The number of variants there are.
    /// The read tag must be `< num_variants`.
    num_variants: u8,
}

/// The instruction set of a [`StaticBsatnValidator`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Insn {
    /// Visit the byte at offset `start + N`
    /// and assert that it is 0 or 1, i.e., a valid `bool`.
    CheckBool(u16),

    /// Read the `tag` at `start + tag_offset`
    /// and validate that `tag < num_variants`.
    CheckTag(CheckTag),

    /// Read the `tag` at `start + tag_offset`
    /// and validate that `tag < num_variants`.
    /// Then move the instruction pointer forward by `tag + 1`.
    /// The branch logic for the variant payload continues there.
    CheckReadTagRelBranch(CheckTag),

    /// Unconditionally branch to the instruction at `program[N]`
    /// where `N > instruction pointer`.
    Goto(u16),
}

impl Insn {
    const FIXUP: Self = Self::Goto(u16::MAX);
}

impl MemoryUsage for Insn {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaticBsatnValidator {
    /// The list of instructions that make up this program.
    insns: Arc<[Insn]>,
}

impl MemoryUsage for StaticBsatnValidator {
    fn heap_usage(&self) -> usize {
        let Self { insns } = self;
        insns.heap_usage()
    }
}

/// Check that `bytes[tag_offset] < num_variants`.
///
/// SAFETY: `tag_offset < bytes.len()`.
unsafe fn check_tag(bytes: &[u8], check: CheckTag) -> Result<u8, DecodeError> {
    // SAFETY: the caller has guaranteed that `tag_offset < bytes.len()`.
    let tag = *unsafe { bytes.get_unchecked(check.tag_offset as usize) };
    if tag < check.num_variants {
        Ok(tag)
    } else {
        Err(DecodeError::InvalidTag { tag, sum_name: None })
    }
}

/// Validates that `bytes`, encoded in BSATN,
/// is valid according to the validation `program`
/// and a corresponding `static_layout`,
///
/// # Safety
///
/// The caller must guarantee that
/// all offsets in `program` are `< static_layout.bsatn_length`.
pub(crate) unsafe fn validate_bsatn(
    program: &StaticBsatnValidator,
    static_layout: &StaticLayout,
    bytes: &[u8],
) -> Result<(), DecodeError> {
    // Validate length of BSATN `bytes` against the expected length.
    let expected = static_layout.bsatn_length as usize;
    let given = bytes.len();
    if expected != given {
        return Err(DecodeError::InvalidLen { expected, given });
    }

    let program = &*program.insns;
    let mut instr_ptr = 0;
    loop {
        match program.get(instr_ptr as usize).copied() {
            None => break,
            Some(Insn::CheckBool(offset)) => {
                instr_ptr += 1;
                // SAFETY: the caller has guaranteed
                // that all offsets in `program` are `< expected`
                // which we by now know is `= bytes.len()`.
                let byte = *unsafe { bytes.get_unchecked(offset as usize) };
                if byte > 1 {
                    return Err(DecodeError::InvalidBool(byte));
                }
            }
            Some(Insn::Goto(new_insn)) => instr_ptr = new_insn,
            Some(Insn::CheckTag(check)) => {
                // SAFETY: the caller has guaranteed
                // that all offsets in `program` are `< expected`
                // which we by now know is `= bytes.len()`.
                unsafe { check_tag(bytes, check) }?;
                instr_ptr += 1;
            }
            Some(Insn::CheckReadTagRelBranch(check)) => {
                // SAFETY: the caller has guaranteed
                // that all offsets in `program` are `< expected`
                // which we by now know is `= bytes.len()`.
                let tag = unsafe { check_tag(bytes, check) }?;
                instr_ptr += tag as u16 + 1;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::{
        bflatn_to::write_row_to_page, blob_store::HashMapBlobStore, page::Page, row_type_visitor::row_type_visitor,
    };
    use proptest::{prelude::*, prop_assert_eq, proptest};
    use spacetimedb_sats::bsatn::to_vec;
    use spacetimedb_sats::proptest::generate_typed_row;
    use spacetimedb_sats::{AlgebraicType, ProductType};

    proptest! {
        // This test checks that `validate_bsatn(...).is_ok() == write_row_to_page(..).is_ok()`.
        #![proptest_config(ProptestConfig {
            max_global_rejects: 65536,
            cases: if cfg!(miri) { 8 } else { 2048 },
            ..<_>::default()
        })]
        #[test]
        fn validation_same_as_write_row_to_pages((ty, val) in generate_typed_row()) {
            let ty: RowTypeLayout = ty.into();
            let Some(static_layout) = StaticLayout::for_row_type(&ty) else {
                // `ty` has a var-len member or a sum with different payload lengths,
                // so the fast path doesn't apply.
                return Err(TestCaseError::reject("Var-length type"));
            };
            let validator = static_bsatn_validator(&ty);
            let bsatn = to_vec(&val).unwrap();
            let res_validate = unsafe { validate_bsatn(&validator, &static_layout, &bsatn) };

            let mut page = Page::new(ty.size());
            let visitor = row_type_visitor(&ty);
            let blob_store = &mut HashMapBlobStore::default();
            let res_write = unsafe { write_row_to_page(&mut page, blob_store, &visitor, &ty, &val) };

            prop_assert_eq!(res_validate.is_ok(), res_write.is_ok());
        }

        #[test]
        fn bad_bool_validates_to_error(byte in 2u8..) {
            let ty: RowTypeLayout = ProductType::from([AlgebraicType::Bool]).into();
            let static_layout = StaticLayout::for_row_type(&ty).unwrap();
            let validator = static_bsatn_validator(&ty);

            let bsatn = [byte];
            let res_validate = unsafe { validate_bsatn(&validator, &static_layout, &bsatn) };
            prop_assert_eq!(res_validate, Err(DecodeError::InvalidBool(byte)));
        }
    }
}
