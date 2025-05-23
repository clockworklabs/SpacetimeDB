//! A [`VarLenMembers`] visitor for [`AlgebraicType`],
//! supporting any non-recursive `AlgebraicType`,
//! including sums and products.
//!
//! The general architecture is:
//! first, we walk the `AlgebraicType` to construct a [`VarLenRoseTree`],
//! a [Rose Tree](https://en.wikipedia.org/wiki/Rose_tree)
//! with a structure that matches the original `AlgebraicType`,
//! but with non-var-len members stripped out
//! and var-len members resolved to their offsets within a row.
//!
//! We then flatten the `VarLenRoseTree` into a [`VarLenVisitorProgram`],
//! a simple bytecode language which can be interpreted relatively efficiently
//! to visit the var-len members in a row.
//!
//! A `VarLenVisitorProgram` for a fixed-length-only row will be empty.
//!
//! A `VarLenVisitorProgram` for a (potentially nested) product type
//! will be a sequence of `VisitOffset` instructions,
//! with no control flow.
//!
//! [`SumType`](spacetimedb_sats::SumType)s which contain var-len refs
//! will introduce control-flow to the `VarLenVisitorProgram`
//! using `SwitchOnTag` to visit the appropriate variant,
//! and `Goto` to return to the part of the var-len object after the sum.
//!
//! The `VarLenMembers` impl for `VarLenVisitorProgram`
//! implements a simple interpreter loop for the var-len visitor bytecode.

use crate::{layout::ProductTypeLayoutView, MemoryUsage};

use super::{
    indexes::{Byte, Bytes, PageOffset},
    layout::{align_to, AlgebraicTypeLayout, HasLayout, RowTypeLayout, SumTypeLayout},
    page::get_ref,
    var_len::{VarLenMembers, VarLenRef},
};
use core::fmt;
use core::marker::PhantomData;
use itertools::Itertools;
use std::sync::Arc;

/// Construct an implementor of `VarLenMembers`,
/// which visits the var-len members in a row of `ty`.
///
/// This is a potentially expensive operation,
/// so the resulting `VarLenVisitorProgram` should be stored and re-used.
pub fn row_type_visitor(ty: &RowTypeLayout) -> VarLenVisitorProgram {
    if ty.layout().fixed {
        // Fast-path: The row type doesn't contain var-len members, so quit early.
        return VarLenVisitorProgram { insns: [].into() };
    }

    let rose_tree = product_type_to_rose_tree(ty.product(), &mut 0);

    rose_tree_to_visitor_program(&rose_tree)
}

/// Construct a `VarLenRoseTree` from `ty`.
///
/// See [`algebraic_type_to_rose_tree`] for more details.
fn product_type_to_rose_tree(ty: ProductTypeLayoutView<'_>, current_offset: &mut usize) -> VarLenRoseTree {
    // Loop over all the product elements,
    // which we store in-order,
    // and collect them into a subtree.

    // Better to over-allocate than under-allocate (maybe).
    let mut contents = Vec::with_capacity(ty.elements.len());

    for elt in ty.elements {
        match algebraic_type_to_rose_tree(&elt.ty, current_offset) {
            // No need to collect empty subtrees.
            VarLenRoseTree::Empty => {}
            // Nested products can be flattened.
            VarLenRoseTree::Product(children) => contents.extend(children),
            // `Offset` and `Sum` must be stored as a child of the product.
            child => contents.push(child),
        }
    }

    // After visiting the elements,
    // make `current_offset` aligned for this member,
    // to store the member's trailing padding.
    *current_offset = align_to(*current_offset, ty.align());

    match contents.len() {
        // A product with no var-len members is `Empty`.
        0 => VarLenRoseTree::Empty,
        // A product with a single subtree behaves like that subtree,
        // so prune the intermediate node.
        1 => contents.pop().unwrap(),
        // For a product with multiple children, return a node with those children.
        _ => VarLenRoseTree::Product(contents),
    }
}

/// Construct a `VarLenRoseTree` from the sum type `ty`.
/// The sum types are the main reason we go through the complication of a rose tree
/// and why we cannot end up with a simple `[u16]`
/// (i.e., [`AlignedVarLenOffsets`](super::var_len::AlignedVarLenOffsets))
/// as our `VarlenMembers` type.
///
/// See [`algebraic_type_to_rose_tree`] for more details.
fn sum_type_to_rose_tree(ty: &SumTypeLayout, current_offset: &mut usize) -> VarLenRoseTree {
    // The tag goes before the variant data.
    // Currently, this is the same as `*current_offset`.
    let tag_offset = *current_offset + ty.offset_of_tag();

    // For each variant, collect that variant's sub-tree.
    let mut variants = ty
        .variants
        .iter()
        .enumerate()
        .map(|(tag, variant)| {
            let var_ty = &variant.ty;

            // All variants are stored overlapping at the offset of the sum.
            // Don't let them mutate `current_offset`.
            // Note that we store sums with tag first,
            // followed by data/payload.
            //
            // `offset_of_variant_data` is defined as 0,
            // but included for future-proofing.
            let mut child_offset = *current_offset + ty.offset_of_variant_data(tag as u8);
            algebraic_type_to_rose_tree(var_ty, &mut child_offset)
        })
        .collect::<Vec<_>>();

    // Store the new offset after the sum.
    *current_offset += ty.size();

    if variants.iter().all(|var| matches!(var, VarLenRoseTree::Empty)) {
        // Sums with no var-len members are `Empty`.
        VarLenRoseTree::Empty
    } else if variants.iter().all_equal() {
        // When all variants have the same set of var-len refs,
        // there's no need to switch on the tag, so prune the intermediate node.
        // A special case of this is single-variant sums.
        variants.pop().unwrap()
    } else {
        // For `Sum`s with multiple variants,
        // at least one of which contains a var-len obj,
        // return a `Sum` node.
        VarLenRoseTree::Sum {
            tag_offset: tag_offset as u16,
            tag_data_processors: variants,
        }
    }
}

/// Construct a `VarLenRoseTree` from `ty`.
///
/// `current_offset` should be passed as `&mut 0` upon entry to the row-type,
/// and will be incremented as appropriate during recursive traversal
/// to track the offset in bytes of the member currently being visited.
fn algebraic_type_to_rose_tree(ty: &AlgebraicTypeLayout, current_offset: &mut usize) -> VarLenRoseTree {
    // Align the `current_offset` for `ty`.
    // This accounts for any padding before `ty`.
    *current_offset = align_to(*current_offset, ty.align());

    match ty {
        AlgebraicTypeLayout::VarLen(_) => {
            // Strings, arrays, and maps are stored as `VarLenRef`s.
            // These are the offsets we want to find.

            // Post-increment the size of `VarLenRef` into `current_offset`,
            // so it stores the offset after this member.
            let member = *current_offset as u16;
            *current_offset += ty.size();
            VarLenRoseTree::Offset(member)
        }
        AlgebraicTypeLayout::Primitive(primitive_type) => {
            // For primitive types, increment `current_offset` past this member,
            // then return the empty tree.
            *current_offset += primitive_type.size();
            VarLenRoseTree::Empty
        }
        AlgebraicTypeLayout::Product(ty) => product_type_to_rose_tree(ty.view(), current_offset),
        AlgebraicTypeLayout::Sum(ty) => sum_type_to_rose_tree(ty, current_offset),
    }
}

/// A [Rose Tree](https://en.wikipedia.org/wiki/Rose_tree)
/// containing information about the var-len members in an `AlgebraicType`.
#[derive(PartialEq, Eq)]
enum VarLenRoseTree {
    /// No var-len members.
    Empty,

    /// A var-len member at `row + N` bytes.
    Offset(u16),

    /// A product type which contains multiple var-len members.
    Product(Vec<VarLenRoseTree>),

    /// A sum type which contains at least one variant with at least one var-len member.
    Sum {
        /// The sum's tag is at `row + tag_offset` bytes.
        tag_offset: u16,
        /// The var-len members of variant `N` are described in `tag_data_processors[N]`.
        tag_data_processors: Vec<VarLenRoseTree>,
    },
}

/// Compile the [`VarLenRoseTree`] to [`VarLenVisitorProgram`].
fn rose_tree_to_visitor_program(tree: &VarLenRoseTree) -> VarLenVisitorProgram {
    let mut program = Vec::new();

    fn compile_tree(tree: &VarLenRoseTree, into: &mut Vec<Insn>) {
        match tree {
            VarLenRoseTree::Empty => {}
            VarLenRoseTree::Offset(offset) => into.push(Insn::VisitOffset(*offset)),
            VarLenRoseTree::Product(elts) => {
                for elt in elts {
                    compile_tree(elt, into);
                }
            }
            VarLenRoseTree::Sum {
                tag_offset,
                tag_data_processors,
            } => {
                let dummy_insn = into.len();
                // We'll replace this later with a `SwitchOnTag`.
                into.push(Insn::FIXUP);

                let mut goto_fixup_locations = Vec::with_capacity(tag_data_processors.len());
                let mut jump_targets = Vec::with_capacity(tag_data_processors.len());

                for branch in tag_data_processors {
                    jump_targets.push(into.len() as u16);
                    compile_tree(branch, into);
                    goto_fixup_locations.push(into.len());

                    // We'll rewrite this to store the after-sum address later.
                    into.push(Insn::FIXUP);
                }

                let goto_addr = into.len();
                for idx in goto_fixup_locations {
                    into[idx] = Insn::Goto(goto_addr as u16);
                }

                into[dummy_insn] = Insn::SwitchOnTag {
                    tag_offset: *tag_offset,
                    jump_targets: jump_targets.into(),
                };
            }
        }
    }

    compile_tree(tree, &mut program);

    remove_trailing_gotos(&mut program);

    VarLenVisitorProgram { insns: program.into() }
}

/// Remove any trailing gotos.
///
/// They are not needed as they will only go towards the end,
/// so we can just cut them out.
fn remove_trailing_gotos(program: &mut Vec<Insn>) -> bool {
    let mut changed = false;
    for idx in (0..program.len()).rev() {
        match program[idx] {
            Insn::Goto(_) => {
                program.pop();
                changed = true;
            }
            _ => break,
        }
    }
    changed
}

/// The instruction set of a [`VarLenVisitorProgram`].
#[derive(Debug, Clone, PartialEq, Eq)]
enum Insn {
    // TODO(perf): consider boxing this variant (or making it a variable-width instruction)
    //             to minimize sizeof(insn),
    //             This could be valuable, assuming that sum types in tables are rare.
    //             E.g. a `SwitchOnTag` could be replaced with:
    //       ```ignore
    //       +0: ReadTagRelativeBranch(tag_offset) // read tag, add it to instruction pointer
    //       +1: Goto(target 0)
    //       +2: Goto(target 1)
    //       +n: Goto(target n - 1)
    //       target 0: code to visit variant 0
    //       target 1: code to visit variant 1
    //       ```
    /// Read a byte `tag` from `row + tag_offset`,
    /// and branch to `jump_targets[tag]`.
    ///
    /// Entering a sum will `SwitchOnTag` to visit the appropriate variant.
    SwitchOnTag {
        /// The tag to dispatch on is stored at `row+tag_offset`.
        tag_offset: u16,
        /// Indexes within the vec of insns.
        /// Invariant: `∀ jt ∈ jump_targets. jt > instr_ptr`.
        jump_targets: Box<[u16]>,
    },

    /// Visit the `VarLenRef` at `row + N` bytes.
    VisitOffset(
        /// The offset relative to the `row` at which the `VarLenRef` slot resides.
        u16,
    ),

    /// Unconditionally branch to the instruction at `program[N]`
    /// where `N > instruction pointer`.
    ///
    /// After visiting a sum variant, the variant will `Goto` to skip all subsequent sum variants.
    Goto(
        /// The new instruction pointer.
        u16,
    ),
}

impl Insn {
    const FIXUP: Self = Self::Goto(u16::MAX);
}

impl MemoryUsage for Insn {}

#[allow(clippy::disallowed_macros)] // This is for test code.
pub fn dump_visitor_program(program: &VarLenVisitorProgram) {
    for (idx, insn) in program.insns.iter().enumerate() {
        eprintln!("{idx:2}: {insn}");
    }
}

impl fmt::Display for Insn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VisitOffset(offset) => write!(f, "visit {offset}"),
            Self::Goto(x) => write!(f, "goto -> {x}"),
            Self::SwitchOnTag {
                tag_offset,
                jump_targets,
            } => write!(f, "switch_on_tag at {tag_offset} {jump_targets:?}"),
        }
    }
}

/// A var-len visitor `program` constructed for a particular row type `ty`,
/// a [`RowTypeLayout`], using [`row_type_visitor`],
/// which is the only way a program can be produced.
///
/// Users of the `program` must ensure,
/// when calling [`VarLenMembers::visit_var_len`] or [`VarLenMembers::visit_var_len_mut`],
/// that is only ever used on a `row` where `row: ty`.
///
/// A program consists of a list of simple byte-code instructions
/// which can be interpreted to visit the var-len members in a row.
/// Forward progress, and thus termination,
/// during interpretation is guaranteed when evaluating a program,
/// as all jumps (`SwitchOnTag` and `Goto`) will set `new_instr_ptr > old_instr_ptr`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VarLenVisitorProgram {
    /// The list of instructions that make up this program.
    insns: Arc<[Insn]>,
}

impl MemoryUsage for VarLenVisitorProgram {
    fn heap_usage(&self) -> usize {
        let Self { insns } = self;
        insns.heap_usage()
    }
}

/// Evalutes the `program`,
/// provided the `instr_ptr` as its program counter / intruction pointer,
/// and a callback `read_tag` to extract a tag at the given offset,
/// until `Some(offset)` is reached,
/// or the program halts.
///
/// SAFETY: This function is safe,
/// as it is the responsibility of `read_tag` to ensure that `read_tag(tag_offset)`
/// for the tag offsets stored in `program` is valid.
#[inline]
fn next_vlr_offset(program: &[Insn], instr_ptr: &mut u16, read_tag: impl Fn(u16) -> u8) -> Option<PageOffset> {
    loop {
        match program.get(*instr_ptr as usize)? {
            Insn::VisitOffset(offset) => {
                *instr_ptr += 1;
                return Some(PageOffset(*offset));
            }
            Insn::Goto(next) => *instr_ptr = *next,
            Insn::SwitchOnTag {
                tag_offset,
                jump_targets,
            } => {
                let tag = read_tag(*tag_offset);
                let go_to = jump_targets[tag as usize];
                *instr_ptr = go_to;
            }
        }
    }
}

// Reads the `tag: u8` at `offset` in `row`.
//
// SAFETY: `offset` mut be in bounds of `row`
// and `row[offset]` must refer to a valid `u8`.
#[inline]
unsafe fn read_tag(row: *const Byte, offset: u16) -> u8 {
    // SAFETY: Caller promised that `offset` will be in bounds of `self.row`.
    let byte_ptr = unsafe { row.add(offset as usize) }.cast();
    // SAFETY: Caller promised that `offset`, so `byte_ptr`, points to a valid `u8`.
    unsafe { *byte_ptr }
}

// SAFETY: Both methods visit the same permutation
// as both use the same underlying mechanism `next_vlr_offset` on the same `&[Insn]`.
unsafe impl VarLenMembers for VarLenVisitorProgram {
    type Iter<'this, 'row> = VarLenVisitorProgramIter<'this, 'row>;
    type IterMut<'this, 'row> = VarLenVisitorProgramIterMut<'this, 'row>;

    unsafe fn visit_var_len<'this, 'row>(&'this self, row: &'row Bytes) -> Self::Iter<'this, 'row> {
        // SAFETY:
        //
        // - Caller promised that `row` is properly aligned for the row type
        //   so based on this assumption, our program will yield references that are properly
        //   aligned for `VarLenGranule`s.
        //
        // - Caller promised that `row.len() == row_type.size()`.
        //   This ensures that our program will yield references that are in bounds of `row`.
        Self::Iter {
            instr_ptr: 0,
            row,
            program: &self.insns,
        }
    }

    unsafe fn visit_var_len_mut<'this, 'row>(&'this self, row: &'row mut Bytes) -> Self::IterMut<'this, 'row> {
        // SAFETY: Same as in `visit_var_len` above.
        Self::IterMut {
            instr_ptr: 0,
            _row_lifetime: PhantomData,
            row: row.as_mut_ptr(),
            program: &self.insns,
        }
    }
}

/// The iterator type for `VarLenVisitorProgram::visit_var_len`.
pub struct VarLenVisitorProgramIter<'visitor, 'row> {
    /// The program used to find the var-len ref offsets.
    program: &'visitor [Insn],
    /// The row to find the var-len refs in.
    row: &'row Bytes,
    /// The program counter / instruction pointer.
    instr_ptr: u16,
}

impl<'row> Iterator for VarLenVisitorProgramIter<'_, 'row> {
    type Item = &'row VarLenRef;

    fn next(&mut self) -> Option<Self::Item> {
        // Reads the `tag: u8` at `offset`.
        // SAFETY: Constructing the iterator is a promise that `self.row[offset]` refers to a valid `u8`.
        let read_tag = |offset| unsafe { read_tag(self.row.as_ptr().cast(), offset) };

        let offset = next_vlr_offset(self.program, &mut self.instr_ptr, read_tag)?;
        // SAFETY: Constructing the iterator is a promise that
        // `offset`s produced by the program will be in bounds of `self.row`
        // and that the derived pointer must be properly aligned for a `VarLenRef`.
        //
        // Moreover, `self.row` is non-null, so adding the offset to it results in a non-null pointer.
        // By having `self.row: &'row Bytes` we also know that the pointer is valid for reads
        // and that it will be for `'row` which is tied to the lifetime of `Self::Item`.
        Some(unsafe { get_ref::<VarLenRef>(self.row, offset) })
    }
}

/// The iterator type for `VarLenVisitorProgram::visit_var_len_mut`.
pub struct VarLenVisitorProgramIterMut<'visitor, 'row> {
    /// The program used to find the var-len ref offsets.
    program: &'visitor [Insn],
    _row_lifetime: PhantomData<&'row mut Bytes>,
    /// Pointer to the row to find the var-len refs in.
    row: *mut Byte,
    /// The program counter / instruction pointer.
    instr_ptr: u16,
}

impl<'row> Iterator for VarLenVisitorProgramIterMut<'_, 'row> {
    type Item = &'row mut VarLenRef;

    fn next(&mut self) -> Option<Self::Item> {
        // Reads the `tag: u8` at `offset`.
        // SAFETY: Constructing the iterator is a promise that `self.row[offset]` refers to a valid `u8`.
        let read_tag = |offset| unsafe { read_tag(self.row, offset) };

        let offset = next_vlr_offset(self.program, &mut self.instr_ptr, read_tag)?;
        // SAFETY: Constructing the iterator is a promise that
        // `offset`s produced by the program will be in bounds of `self.row`.
        let vlr_ptr: *mut VarLenRef = unsafe { self.row.add(offset.idx()).cast() };
        // SAFETY: Constructing the iterator is a promise that
        // The derived pointer must be properly aligned for a `VarLenRef`.
        //
        // Moreover, `self.row` is non-null, so adding the offset to it results in a non-null pointer.
        // By having `self.row: &'row mut Bytes` we also know that the pointer is valid for reads and writes
        // and that it will be for `'row` which is tied to the lifetime of `Self::Item`.
        Some(unsafe { &mut *vlr_ptr })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::indexes::Size;
    use spacetimedb_sats::{AlgebraicType, ProductType};

    fn row_type<T: Into<ProductType>>(row_ty: T) -> RowTypeLayout {
        let ty: ProductType = row_ty.into();
        ty.into()
    }

    fn addr<T>(x: &T) -> usize {
        x as *const _ as usize
    }

    fn check_addrs<const N: usize>(prog: &VarLenVisitorProgram, row: &Bytes, expected: [usize; N]) {
        let mut visitor = unsafe { prog.visit_var_len(row) };
        for expected in expected.map(|i| &row[i]) {
            assert_eq!(addr(visitor.next().unwrap()), addr(expected));
        }
        assert!(visitor.next().is_none());
    }

    #[test]
    fn visit_var_len_in_u32_string_u8() {
        let ty = row_type([AlgebraicType::U32, AlgebraicType::String, AlgebraicType::U8]);
        assert_eq!(ty.size(), Size(12));

        // alloc an array of u32 to ensure 4-byte alignment.
        let row = &[0xa5a5_a5a5u32; 3];
        let row = row.as_ptr().cast::<[Byte; 12]>();
        let row = unsafe { &*row };

        check_addrs(&row_type_visitor(&ty), row, [4]);
    }

    #[test]
    fn visit_var_len_nested_product() {
        let ty = row_type([
            AlgebraicType::U32,
            AlgebraicType::String,
            AlgebraicType::U8,
            AlgebraicType::product([AlgebraicType::U32, AlgebraicType::String]),
            AlgebraicType::U8,
        ]);
        assert_eq!(ty.size(), Size(24));

        // Alloc an array of u32 to ensure 4-byte alignment.
        let row = &[0xa5a5_a5a5u32; 6];
        let row = row.as_ptr().cast::<[Byte; 24]>();
        let row = unsafe { &*row };

        check_addrs(&row_type_visitor(&ty), row, [4, 16]);
    }

    #[test]
    fn visit_var_len_bare_enum() {
        let ty = row_type([AlgebraicType::option(
            AlgebraicType::String, // tag 0, size 4, align 2
        )]);
        assert_eq!(ty.size(), Size(6));
        assert_eq!(ty.align(), 2);
        let outer_sum = &ty[0].as_sum().unwrap();
        let outer_tag = outer_sum.offset_of_tag();

        let row = &mut [0xa5a5u16; 3];
        let row_ptr = row.as_mut_ptr().cast::<[Byte; 6]>();
        let row = unsafe { &mut *row_ptr };

        let program = row_type_visitor(&ty);
        // Variant 1 (String) is live
        row[outer_tag] = 0;
        check_addrs(&program, row, [2]);

        // Variant 1 (none) is live
        row[outer_tag] = 1;
        check_addrs(&program, row, []);
    }

    #[test]
    fn visit_var_len_nested_enum() {
        // Tables are always product types.
        let ty = row_type([AlgebraicType::sum([
            AlgebraicType::U32,                                                  // tag 0, size 4, align 4
            AlgebraicType::String,                                               // tag 1, size 4, align 2
            AlgebraicType::product([AlgebraicType::U32, AlgebraicType::String]), // tag 2, size 8, align 4
            AlgebraicType::sum(
                // tag 3, size 8, align 4
                [
                    AlgebraicType::U32,    // tag (3, 0), size 4, align 4
                    AlgebraicType::String, // tag (3, 1), size 4, align 2
                ],
            ),
        ])]);
        assert_eq!(ty.size(), Size(12));
        assert_eq!(ty.align(), 4);
        let outer_sum = &ty[0].as_sum().unwrap();
        let outer_tag = outer_sum.offset_of_tag();
        assert_eq!(outer_tag, 0);
        let inner_sum = outer_sum.variants[3].ty.as_sum().unwrap();
        let inner_tag = outer_sum.offset_of_variant_data(3) + inner_sum.offset_of_tag();
        assert_eq!(inner_tag, 4);

        let row = &mut [0xa5a5_a5a5u32; 3];
        let row_ptr = row.as_mut_ptr().cast::<[Byte; 12]>();
        let row = unsafe { &mut *row_ptr };

        let program = row_type_visitor(&ty);

        // Variant 0 (U32) is live
        row[outer_tag] = 0;
        check_addrs(&program, row, []);

        // Variant 1 (String) is live
        row[outer_tag] = 1;
        check_addrs(&program, row, [4]);

        // Variant 2 (Product) is live
        row[outer_tag] = 2;
        check_addrs(&program, row, [8]);

        // Variant 3 (Sum) is live but its tag is not valid yet.
        row[outer_tag] = 3;

        // Variant 3, 0 (Sum, U32) is live.
        row[inner_tag] = 0;
        check_addrs(&program, row, []);

        // Variant 3, 1 (Sum, String) is live.
        row[inner_tag] = 1;
        check_addrs(&program, row, [8]);
    }
}
