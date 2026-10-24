use proc_macro::TokenStream;
mod domain;
mod observation;

#[proc_macro_derive(DomainInput)]
pub fn derive_domain_input(input: TokenStream) -> TokenStream {
    domain::derive(syn::parse_macro_input!(input as syn::DeriveInput))
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

#[proc_macro]
pub fn domain(input: TokenStream) -> TokenStream {
    domain::domain(input)
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

#[proc_macro]
pub fn fact(input: TokenStream) -> TokenStream {
    domain::fact(input)
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}
mod validation;

#[proc_macro_derive(Observation, attributes(observation))]
pub fn derive_observation(input: TokenStream) -> TokenStream {
    match observation::expand(syn::parse_macro_input!(input as syn::DeriveInput)) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}
