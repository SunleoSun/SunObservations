use crate::validation;
use proc_macro2::TokenStream;
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{format_ident, quote};
use syn::{parse_quote, punctuated::Punctuated, DeriveInput, Ident, LitStr, Token};

pub fn expand(mut input: DeriveInput) -> syn::Result<TokenStream> {
    let mut namespace: Option<LitStr> = None;
    let mut level: Option<Ident> = None;
    let mut tags: Option<Vec<LitStr>> = None;
    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("observation"))
    {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("namespace") {
                if namespace.is_some() {
                    return Err(meta.error("duplicate namespace"));
                }
                let value: LitStr = meta.value()?.parse()?;
                if !validation::namespace(&value.value()) {
                    return Err(syn::Error::new(
                        value.span(),
                        "namespace must contain [a-z][a-z0-9_]* segments separated by ::",
                    ));
                }
                namespace = Some(value);
            } else if meta.path.is_ident("level") {
                if level.is_some() {
                    return Err(meta.error("duplicate level"));
                }
                let value: Ident = meta.value()?.parse()?;
                if !matches!(value.to_string().as_str(), "INFO" | "WARN" | "ERROR") {
                    return Err(syn::Error::new(
                        value.span(),
                        "level must be INFO, WARN, or ERROR; severity is not verbosity",
                    ));
                }
                level = Some(value);
            } else if meta.path.is_ident("tags") {
                if tags.is_some() {
                    return Err(meta.error("duplicate tags"));
                }
                let content;
                syn::parenthesized!(content in meta.input);
                let values = Punctuated::<LitStr, Token![,]>::parse_terminated(&content)?;
                for tag in &values {
                    if !validation::segment(&tag.value()) {
                        return Err(syn::Error::new(
                            tag.span(),
                            "tags must match [a-z][a-z0-9_]*",
                        ));
                    }
                }
                tags = Some(values.into_iter().collect());
            } else {
                return Err(meta.error("unknown observation attribute"));
            }
            Ok(())
        })?;
    }
    let namespace = namespace.ok_or_else(|| {
        syn::Error::new_spanned(&input.ident, "missing required observation namespace")
    })?;
    let level = level.unwrap_or_else(|| format_ident!("INFO"));
    let tags = tags.unwrap_or_default();
    let runtime = match crate_name("sun-observations") {
        Ok(FoundCrate::Name(name)) => {
            let name = format_ident!("{}", name);
            quote!(::#name)
        }
        Ok(FoundCrate::Itself) => quote!(::sun_observations),
        Err(error) => return Err(syn::Error::new_spanned(&input.ident, error.to_string())),
    };
    let name = &input.ident;
    let (_, type_generics, _) = input.generics.split_for_impl();
    let bound = parse_quote!(#name #type_generics: #runtime::__private::serde::Serialize + 'static);
    input.generics.make_where_clause().predicates.push(bound);
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #runtime::Observation for #name #type_generics #where_clause {
            const NAMESPACE: &'static str = #namespace;
            const LEVEL: #runtime::ObservationLevel = #runtime::ObservationLevel::#level;
            const TAGS: &'static [&'static str] = &[#(#tags),*];
        }
    })
}

#[cfg(test)]
#[path = "observation_test.rs"]
mod tests;
