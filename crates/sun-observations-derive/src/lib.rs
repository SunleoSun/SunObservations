use proc_macro::TokenStream;
mod observation;
mod validation;

#[proc_macro_derive(Observation, attributes(observation))]
pub fn derive_observation(input: TokenStream) -> TokenStream {
    match observation::expand(syn::parse_macro_input!(input as syn::DeriveInput)) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
