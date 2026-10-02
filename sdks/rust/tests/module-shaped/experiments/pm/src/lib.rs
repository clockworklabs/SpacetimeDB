use proc_macro::TokenStream;
/// Attribute that returns its item unchanged, so we can see what rustc allows it to be applied to.
#[proc_macro_attribute]
pub fn passthrough(_args: TokenStream, item: TokenStream) -> TokenStream { item }

/// Attribute that defines a "descriptor" macro_rules next to the item, re-exported by path.
/// `#[describe(foo)] struct X;` emits `macro_rules! __desc_foo { ($($cb:tt)*) => { $($cb)* ! { foo } } } pub(crate) use __desc_foo;`
#[proc_macro_attribute]
pub fn describe(args: TokenStream, item: TokenStream) -> TokenStream {
    let name = args.to_string();
    let s = format!("{item}\n#[doc(hidden)] macro_rules! __desc_{name} {{ ( [$($cb:tt)*] $($acc:tt)* ) => {{ $($cb)* ! {{ $($acc)* {name} }} }} }}\n#[doc(hidden)] pub(crate) use __desc_{name};");
    s.parse().unwrap()
}

/// CPS driver: `collect!{ [remaining paths] acc... }`. If remaining is empty, emits `pub const COLLECTED: &[&str] = &[acc...]`.
/// Otherwise invokes the head descriptor with ourselves as callback.
#[proc_macro]
pub fn collect(input: TokenStream) -> TokenStream {
    let mut it = input.into_iter();
    let first = it.next().unwrap();
    let rest: Vec<_> = it.collect();
    let acc: TokenStream = rest.into_iter().collect();
    let remaining = match first { proc_macro::TokenTree::Group(g) => g.stream().to_string(), _ => panic!() };
    let paths: Vec<String> = remaining.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    if paths.is_empty() {
        let names: Vec<String> = acc.to_string().split_whitespace().map(|n| format!("{n:?}")).collect();
        format!("pub const COLLECTED: &[&str] = &[{}];", names.join(",")).parse().unwrap()
    } else {
        let head = &paths[0];
        let tail = paths[1..].join(",");
        format!("{head}! {{ [::pm::collect] [{tail}] {acc} }}").parse().unwrap()
    }
}
