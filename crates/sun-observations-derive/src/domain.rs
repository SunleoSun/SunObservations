use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream},
    Data, DeriveInput, Expr, Fields, Ident, Path, Token,
};

fn runtime() -> syn::Result<TokenStream> {
    Ok(
        match proc_macro_crate::crate_name("sun-observations")
            .map_err(|e| syn::Error::new(proc_macro2::Span::call_site(), e.to_string()))?
        {
            proc_macro_crate::FoundCrate::Itself => quote!(::sun_observations::certifications),
            proc_macro_crate::FoundCrate::Name(n) => {
                let n = format_ident!("{}", n);
                quote!(::#n::certifications)
            }
        },
    )
}

pub fn derive(input: DeriveInput) -> syn::Result<TokenStream> {
    let rt = runtime()?;
    let name = input.ident;
    let vis = input.vis;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            name,
            "DomainInput currently requires a concrete input type",
        ));
    }
    let descriptor = format_ident!("{}DomainFields", name);
    let mut extra = TokenStream::new();
    let mut descriptor_fields = Vec::new();
    let mut descriptor_init = Vec::new();
    let schema;
    let encode;
    let decode;
    match input.data {
        Data::Struct(s) => {
            let Fields::Named(fields) = s.fields else {
                return Err(syn::Error::new_spanned(
                    name,
                    "DomainInput structs require named fields",
                ));
            };
            let ids: Vec<_> = fields
                .named
                .iter()
                .map(|f| f.ident.as_ref().unwrap())
                .collect();
            let tys: Vec<_> = fields.named.iter().map(|f| &f.ty).collect();
            for (i, (id, ty)) in ids.iter().zip(&tys).enumerate() {
                descriptor_fields.push(quote!(pub #id: #rt::DomainField<#ty>));
                descriptor_init.push(quote!(#id: field.child(#rt::DomainPathStep::Field(#i))));
            }
            schema = quote!(#rt::DomainSchema::Struct(vec![#((stringify!(#ids), <#tys as #rt::DomainInput>::schema())),*]));
            encode = quote!(#rt::DomainValue::Struct(vec![#(#rt::DomainInput::to_domain_value(&self.#ids)),*]));
            decode = quote!({ let #rt::DomainValue::Struct(values) = value else { return Err(#rt::domain_error("invalid struct value")); }; let mut values = values.into_iter(); Ok(Self { #(#ids: <#tys as #rt::DomainInput>::from_domain_value(values.next().ok_or_else(|| #rt::domain_error("missing field"))?)?),* }) });
        }
        Data::Enum(e) => {
            extra.extend(
                quote!(impl #rt::DomainScalar for #name {} impl #rt::DomainEnum for #name {}),
            );
            let mut schemas = Vec::new();
            let mut enc = Vec::new();
            let mut dec = Vec::new();
            for (index, variant) in e.variants.iter().enumerate() {
                let id = &variant.ident;
                let vd = format_ident!("{}{}DomainFields", name, id);
                let ids: Vec<_> = variant
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(i, f)| f.ident.clone().unwrap_or_else(|| format_ident!("_{}", i)))
                    .collect();
                let tys: Vec<_> = variant.fields.iter().map(|f| &f.ty).collect();
                let members: Vec<_> = ids
                    .iter()
                    .zip(&tys)
                    .map(|(id, ty)| quote!(pub #id: #rt::DomainField<#ty>))
                    .collect();
                let init: Vec<_> = ids.iter().enumerate().map(|(i,id)| quote!(#id: field.child::<Self>(#rt::DomainPathStep::Variant(#index)).child(#rt::DomainPathStep::Field(#i)))).collect();
                extra.extend(quote!(#[doc(hidden)] #[allow(non_snake_case)] #vis struct #vd { #(#members),* }));
                descriptor_fields.push(quote!(pub #id: #vd));
                descriptor_init.push(quote!(#id: #vd { #(#init),* }));
                schemas.push(quote!((
                    stringify!(#id),
                    vec![#((stringify!(#ids), <#tys as #rt::DomainInput>::schema())),*]
                )));
                let pattern = match variant.fields {
                    Fields::Named(_) => quote!(Self::#id { #(#ids),* }),
                    Fields::Unnamed(_) => quote!(Self::#id(#(#ids),*)),
                    Fields::Unit => quote!(Self::#id),
                };
                enc.push(quote!(#pattern => #rt::DomainValue::Enum(#index, vec![#(#rt::DomainInput::to_domain_value(#ids)),*])));
                let vals = ids.iter().zip(&tys).map(|(id,ty)| quote!(#id: <#ty as #rt::DomainInput>::from_domain_value(values.next().ok_or_else(|| #rt::domain_error("missing payload"))?)?));
                let construct = match variant.fields {
                    Fields::Named(_) => quote!(Self::#id { #(#vals),* }),
                    Fields::Unnamed(_) => {
                        let vals=tys.iter().map(|ty| quote!(<#ty as #rt::DomainInput>::from_domain_value(values.next().ok_or_else(|| #rt::domain_error("missing payload"))?)?));
                        quote!(Self::#id(#(#vals),*))
                    }
                    Fields::Unit => quote!(Self::#id),
                };
                dec.push(quote!(#index => Ok(#construct)));
            }
            schema = quote!(#rt::DomainSchema::Enum(vec![#(#schemas),*]));
            encode = quote!(match self { #(#enc),* });
            decode = quote!({ let #rt::DomainValue::Enum(tag, values) = value else { return Err(#rt::domain_error("invalid enum value")); }; let mut values = values.into_iter(); match tag { #(#dec),*, _ => Err(#rt::domain_error("invalid enum tag")) } });
        }
        _ => {
            return Err(syn::Error::new_spanned(
                name,
                "DomainInput does not support unions",
            ))
        }
    }
    Ok(quote! {
        #[doc(hidden)] #[allow(non_snake_case)] #vis struct #descriptor { #(#descriptor_fields),* }
        #extra
        impl #rt::DomainInput for #name {
            type Fields = #descriptor;
            fn schema() -> #rt::DomainSchema { #schema }
            fn domain_fields(field: #rt::DomainField<Self>) -> Self::Fields { #descriptor { #(#descriptor_init),* } }
            fn to_domain_value(&self) -> #rt::DomainValue { #encode }
            fn from_domain_value(value: #rt::DomainValue) -> Result<Self, #rt::AnalysisError> { #decode }
        }
    })
}

enum Entry {
    Expr(Expr),
    Nested(Ident, Vec<Entry>),
    Variant(Path, Vec<Entry>),
    When(Expr, Vec<Entry>),
    Each(Expr),
    EachNested(Vec<Entry>),
}
fn entries(input: ParseStream) -> syn::Result<Vec<Entry>> {
    let mut result = Vec::new();
    while !input.is_empty() {
        let fork = input.fork();
        let head: Path = fork.parse()?;
        let simple = head.get_ident().map(ToString::to_string);
        let entry = if simple.as_deref() == Some("when") {
            input.parse::<Ident>()?;
            let condition;
            parenthesized!(condition in input);
            let condition = condition.parse()?;
            let body;
            braced!(body in input);
            Entry::When(condition, entries(&body)?)
        } else if fork.peek(syn::token::Brace) {
            let path: Path = input.parse()?;
            let body;
            braced!(body in input);
            if let Some(id) = path.get_ident() {
                if id == "each" {
                    Entry::EachNested(entries(&body)?)
                } else {
                    Entry::Nested(id.clone(), entries(&body)?)
                }
            } else {
                Entry::Variant(path, entries(&body)?)
            }
        } else if simple.as_deref() == Some("each") && fork.peek(syn::token::Paren) {
            input.parse::<Ident>()?;
            let body;
            parenthesized!(body in input);
            Entry::Each(body.parse()?)
        } else {
            Entry::Expr(input.parse()?)
        };
        result.push(entry);
        if !input.is_empty() {
            input.parse::<Token![,]>()?;
        }
    }
    Ok(result)
}
struct Definition {
    ty: syn::TypePath,
    entries: Vec<Entry>,
}
impl Parse for Definition {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ty = input.parse()?;
        let body;
        braced!(body in input);
        Ok(Self {
            ty,
            entries: entries(&body)?,
        })
    }
}
fn root(expr: &Expr) -> Option<Ident> {
    match expr {
        Expr::Path(p) => p.path.get_ident().cloned(),
        Expr::MethodCall(m) => root(&m.receiver),
        Expr::Field(f) => root(&f.base),
        _ => None,
    }
}
fn rewrite_domain_field_paths(expr: &mut Expr, roots: &[Ident]) {
    fn chain(expr: &Expr) -> Option<(Ident, Vec<syn::Member>)> {
        match expr {
            Expr::Path(path) => path.path.get_ident().cloned().map(|root| (root, vec![])),
            Expr::Field(field) => {
                let (root, mut members) = chain(&field.base)?;
                members.push(field.member.clone());
                Some((root, members))
            }
            _ => None,
        }
    }

    if let Some((root, members)) = chain(expr) {
        if !members.is_empty() && roots.contains(&root) {
            let mut rewritten: Expr = syn::parse_quote!(#root);
            for member in members {
                rewritten = syn::parse_quote!((#rewritten).fields().#member);
            }
            *expr = rewritten;
            return;
        }
    }

    match expr {
        Expr::MethodCall(call) => {
            rewrite_domain_field_paths(&mut call.receiver, roots);
            for arg in &mut call.args {
                rewrite_domain_field_paths(arg, roots);
            }
        }
        Expr::Call(call) => {
            for arg in &mut call.args {
                rewrite_domain_field_paths(arg, roots);
            }
        }
        Expr::Paren(paren) => rewrite_domain_field_paths(&mut paren.expr, roots),
        Expr::Group(group) => rewrite_domain_field_paths(&mut group.expr, roots),
        Expr::Reference(reference) => rewrite_domain_field_paths(&mut reference.expr, roots),
        _ => {}
    }
}

fn bindings(entries: &[Entry], out: &mut Vec<Ident>) {
    fn add(out: &mut Vec<Ident>, id: Ident) {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    for entry in entries {
        match entry {
            Entry::Expr(e) | Entry::When(e, _) => {
                if let Expr::MethodCall(m) = e {
                    if let Some(id) = root(&m.receiver) {
                        add(out, id);
                    }
                    if matches!(
                        m.method.to_string().as_str(),
                        "less_than"
                            | "less_equal"
                            | "greater_than"
                            | "greater_equal"
                            | "equal"
                            | "not_equal"
                    ) {
                        for arg in &m.args {
                            if let Some(id) = root(arg) {
                                if id
                                    .to_string()
                                    .chars()
                                    .next()
                                    .is_some_and(char::is_lowercase)
                                {
                                    add(out, id);
                                }
                            }
                        }
                    }
                }
                if let Entry::When(_, body) = entry {
                    bindings(body, out);
                }
            }
            Entry::Nested(id, _) => add(out, id.clone()),
            _ => {}
        }
    }
}
fn rewrite_length(expr: &mut Expr) {
    if let Expr::MethodCall(m) = expr {
        rewrite_length(&mut m.receiver);
    }
    if let Expr::Call(c) = expr {
        if let Expr::Path(p) = &*c.func {
            if p.path.is_ident("length") {
                *expr = syn::parse_quote!(__field.length());
            }
        }
    }
}
fn emit(entries: &[Entry], rt: &TokenStream, bind: bool) -> TokenStream {
    let mut ids = Vec::new();
    if bind {
        bindings(entries, &mut ids);
    }
    let mut code = quote!(#(let #ids = &__fields.#ids;)*);
    for entry in entries {
        code.extend(match entry {
            Entry::Expr(e) => {
                let mut e = e.clone();
                rewrite_domain_field_paths(&mut e, &ids);
                rewrite_length(&mut e);
                let e = &e;
                let e = if let Expr::Call(c) = e {
                    if let Expr::Path(p) = &*c.func {
                        if p.path.is_ident("all_enum_values")
                            || p.path.is_ident("selected_enum_values")
                            || p.path.is_ident("length")
                            || p.path.is_ident("fixed_value")
                            || p.path.is_ident("selected_values")
                            || p.path.is_ident("in_range")
                            || p.path.is_ident("one_of_ranges")
                        {
                            let id = p.path.get_ident().unwrap();
                            let args = &c.args;
                            quote!(__field.#id(#args))
                        } else {
                            quote!(#e)
                        }
                    } else {
                        quote!(#e)
                    }
                } else {
                    quote!(#e)
                };
                quote!(__domain.add(#e, &__gates);)
            }
            Entry::When(condition, body) => {
                let mut condition = condition.clone();
                rewrite_domain_field_paths(&mut condition, &ids);
                let body = emit(body, rt, false);
                quote!({ let mut __gates = __gates.clone(); __gates.push(#condition); #body })
            }
            Entry::Nested(id, body) => {
                let body = emit(body, rt, true);
                quote!({ let __field = #id.clone(); let __fields = __field.fields(); #body })
            }
            Entry::Variant(path, body) => {
                let id = &path.segments.last().unwrap().ident;
                let body = emit(body, rt, true);
                let mut enum_path = path.clone();
                enum_path.segments.pop();
                enum_path.segments.pop_punct();
                quote!({ #rt::DomainField::<#enum_path>::assert_type(&__field); let __fields = __fields.#id; #body })
            }
            Entry::Each(e) => {
                let Expr::Call(c) = e else {
                    return syn::Error::new_spanned(e, "each requires a constructive value space")
                        .to_compile_error();
                };
                let func = &c.func;
                let args = &c.args;
                quote!(__domain.add(__field.each().#func(#args), &__gates);)
            }
            Entry::EachNested(body) => {
                let body = emit(body, rt, true);
                quote!({ let __field = __field.each(); let __fields = __field.fields(); #body })
            }
        });
    }
    code
}
pub fn domain(input: proc_macro::TokenStream) -> syn::Result<TokenStream> {
    let def: Definition = syn::parse(input)?;
    let rt = runtime()?;
    let ty = def.ty;
    let body = emit(&def.entries, &rt, true);
    Ok(
        quote!({ let mut __domain = #rt::Domain::<#ty>::new(); let __field = #rt::DomainField::<#ty>::root(); let __fields=__field.fields(); let __gates: Vec<#rt::DomainRelation> = Vec::new(); #body __domain }),
    )
}

pub fn fact(input: proc_macro::TokenStream) -> syn::Result<TokenStream> {
    let expr: Expr = syn::parse(input)?;
    let rt = runtime()?;
    fn contains_case_access(e: &Expr) -> bool {
        struct Find(bool);
        impl<'ast> syn::visit::Visit<'ast> for Find {
            fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
                if matches!(
                    call.method.to_string().as_str(),
                    "input" | "result" | "execution" | "observations" | "spans"
                ) && matches!(
                    &*call.receiver,
                    Expr::Path(path) if path.path.is_ident("case")
                ) {
                    self.0 = true;
                }
                syn::visit::visit_expr_method_call(self, call);
            }
        }
        let mut f = Find(false);
        syn::visit::Visit::visit_expr(&mut f, e);
        f.0
    }
    Ok(if contains_case_access(&expr) {
        quote!(case.fact(stringify!(#expr), |case| #expr))
    } else {
        quote!(#rt::fact(stringify!(#expr), #expr))
    })
}
