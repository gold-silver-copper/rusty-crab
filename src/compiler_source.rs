use syn::{
    Token,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

pub struct Keyword {
    pub name: syn::Ident,
    pub spelling: syn::LitStr,
}

impl Parse for Keyword {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        Ok(Self {
            name,
            spelling: input.parse()?,
        })
    }
}

pub fn keywords(source: &str) -> syn::Result<Vec<Keyword>> {
    struct Symbols {
        keywords: Vec<Keyword>,
    }
    impl Parse for Symbols {
        fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
            let name: syn::Ident = input.parse()?;
            if name != "Keywords" {
                return Err(input.error("expected compiler Keywords section"));
            }
            let content;
            syn::braced!(content in input);
            let keywords = Punctuated::<Keyword, Token![,]>::parse_terminated(&content)?
                .into_iter()
                .collect();
            let name: syn::Ident = input.parse()?;
            if name != "Symbols" {
                return Err(input.error("expected compiler Symbols section"));
            }
            let content;
            syn::braced!(content in input);
            let _: proc_macro2::TokenStream = content.parse()?;
            Ok(Self { keywords })
        }
    }
    let file = syn::parse_file(source)?;
    let mut results = None;
    for item in file.items {
        if let syn::Item::Macro(m) = item {
            if m.mac.path.is_ident("symbols") {
                if results.is_some() {
                    return Err(syn::Error::new_spanned(m, "duplicate symbols invocation"));
                }
                results = Some(syn::parse2::<Symbols>(m.mac.tokens)?.keywords);
            }
        }
    }
    results.ok_or_else(|| {
        syn::Error::new(proc_macro2::Span::call_site(), "missing symbols invocation")
    })
}
