use proc_macro2::TokenStream;

pub struct Diagnostic {

}

/// Takes the parsed input from a `#[wasm_bindgen]` macro and returns the generated bindings
pub fn expand(attr: TokenStream, input: TokenStream) -> Result<TokenStream, Diagnostic> {
    unimplemented!()
}