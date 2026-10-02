// Option 1: an attribute macro on a non-inline module.
// rustc 1.96.1: error[E0658]: file modules in proc macro input are unstable (rust-lang/rust#54727)
#[cfg(feature = "attr_on_noninline_mod")]
#[pm::passthrough]
mod child;

// Option 1: an attribute macro on an inline module that declares a non-inline module.
// rustc 1.96.1: error[E0658]: file modules in proc macro input are unstable (rust-lang/rust#54727)
#[cfg(feature = "attr_on_inline_mod_with_noninline_child")]
#[pm::passthrough]
mod child {
    mod grandchild;
}

// Option 1 applied from inside a module file, `#![pm::passthrough]`.
// rustc 1.96.1: error[E0658]: inner macro attributes are unstable (rust-lang/rust#54726),
// and the attribute on the `mod inner;` item it becomes is again E0658 (rust-lang/rust#54727).
#[cfg(feature = "custom_inner_attr")]
mod inner;

// Option 2 with CPS: each item's attribute defines a descriptor `macro_rules!` and re-exports it by path
// with `pub(crate) use`. A collector in another module walks the descriptors and sees every item.
mod few {
    #[pm::describe(alpha)]
    pub struct Alpha;
    #[pm::describe(beta)]
    pub struct Beta;
    #[pm::describe(gamma)]
    pub struct Gamma;
}
pub mod collected_few {
    pm::collect! { [super::few::__desc_alpha, super::few::__desc_beta, super::few::__desc_gamma] }
}

#[test]
fn cps_collects_items_from_another_module() {
    assert_eq!(collected_few::COLLECTED, &["alpha", "beta", "gamma"]);
}
