//! Derive macros for ergonomic enum conversions.
//!
//! Provides four macros:
//! - [`EnumFromInner`] — `From<T>` for newtype variants
//! - [`EnumFromStringify`] — `From<T>` via `.to_string()` for String variants
//! - [`EnumFromTrait`] — implement a custom trait via delegation to a variant
//! - [`EnumVariantList`] — `variant_list()` returning all unit variants

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Error, Fields, Variant};

// ---------------------------------------------------------------------------
// Helper: ensure the derive target is an enum
// ---------------------------------------------------------------------------

fn require_enum(input: &DeriveInput) -> Result<&syn::DataEnum, Error> {
    match &input.data {
        Data::Enum(e) => Ok(e),
        _ => Err(Error::new_spanned(
            &input.ident,
            "this derive macro can only be applied to enums",
        )),
    }
}

/// Extract the single unnamed field type from a variant like `Foo(Bar)`.
fn single_unnamed_field(v: &Variant) -> Result<&syn::Type, Error> {
    match &v.fields {
        Fields::Unnamed(u) if u.unnamed.len() == 1 => Ok(&u.unnamed.first().unwrap().ty),
        _ => Err(Error::new_spanned(
            &v.ident,
            "variant must have exactly one unnamed field",
        )),
    }
}

/// Find all attributes with the given name on a variant.
fn attrs_named<'a>(v: &'a Variant, name: &str) -> Vec<&'a syn::Attribute> {
    v.attrs.iter().filter(|a| a.path().is_ident(name)).collect()
}

// ===========================================================================
// EnumFromInner
// ===========================================================================

/// Generates `impl From<InnerType> for Enum` for each variant tagged with
/// `#[from_inner]`.
///
/// # Example
/// ```ignore
/// #[derive(EnumFromInner)]
/// enum Wrapper {
///     #[from_inner]
///     Text(String),
///     #[from_inner]
///     Number(i64),
///     Ignored(Vec<u8>),
/// }
/// // produces:
/// // impl From<String> for Wrapper { fn from(v: String) -> Self { Self::Text(v) } }
/// // impl From<i64> for Wrapper { fn from(v: i64) -> Self { Self::Number(v) } }
/// ```
#[proc_macro_derive(EnumFromInner, attributes(from_inner))]
pub fn derive_from_inner(tokens: TokenStream) -> TokenStream {
    let input = parse_macro_input!(tokens as DeriveInput);
    match expand_from_inner(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_from_inner(input: &DeriveInput) -> Result<TokenStream2, Error> {
    let data = require_enum(input)?;
    let enum_name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut impls = Vec::new();

    for variant in &data.variants {
        if attrs_named(variant, "from_inner").is_empty() {
            continue;
        }
        let inner_ty = single_unnamed_field(variant)?;
        let var_name = &variant.ident;

        impls.push(quote! {
            impl #impl_generics ::core::convert::From<#inner_ty> for #enum_name #ty_generics #where_clause {
                fn from(val: #inner_ty) -> Self {
                    Self::#var_name(val)
                }
            }
        });
    }

    if impls.is_empty() {
        return Err(Error::new_spanned(
            enum_name,
            "EnumFromInner: at least one variant must be tagged with #[from_inner]",
        ));
    }

    Ok(quote! { #(#impls)* })
}

// ===========================================================================
// EnumFromStringify
// ===========================================================================

/// Generates `impl From<SourceType> for Enum` where the conversion calls
/// `.to_string()` on the source value. The target variant must wrap a `String`.
///
/// Multiple source types can be listed on a single variant.
///
/// # Example
/// ```ignore
/// #[derive(EnumFromStringify)]
/// enum AppError {
///     #[from_stringify("std::io::Error", "serde_json::Error")]
///     IoError(String),
/// }
/// ```
#[proc_macro_derive(EnumFromStringify, attributes(from_stringify))]
pub fn derive_from_stringify(tokens: TokenStream) -> TokenStream {
    let input = parse_macro_input!(tokens as DeriveInput);
    match expand_from_stringify(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_from_stringify(input: &DeriveInput) -> Result<TokenStream2, Error> {
    let data = require_enum(input)?;
    let enum_name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut impls = Vec::new();

    for variant in &data.variants {
        let matching = attrs_named(variant, "from_stringify");
        if matching.is_empty() {
            continue;
        }

        let inner_ty = single_unnamed_field(variant)?;
        // Verify the inner type is String.
        let is_string = matches!(inner_ty, syn::Type::Path(p) if p.path.is_ident("String"));
        if !is_string {
            return Err(Error::new_spanned(
                inner_ty,
                "EnumFromStringify: variant inner type must be String",
            ));
        }

        let var_name = &variant.ident;

        for attr in matching {
            // Parse the attribute argument as a parenthesized list of string literals.
            let source_types: Vec<syn::LitStr> = attr.parse_args_with(|input: syn::parse::ParseStream| {
                let mut out = Vec::new();
                while !input.is_empty() {
                    out.push(input.parse::<syn::LitStr>()?);
                    if !input.is_empty() {
                        input.parse::<syn::Token![,]>()?;
                    }
                }
                Ok(out)
            })?;

            for lit in source_types {
                let ty_path: syn::Path = lit.parse()?;
                impls.push(quote! {
                    impl #impl_generics ::core::convert::From<#ty_path> for #enum_name #ty_generics #where_clause {
                        fn from(e: #ty_path) -> Self {
                            Self::#var_name(e.to_string())
                        }
                    }
                });
            }
        }
    }

    if impls.is_empty() {
        return Err(Error::new_spanned(
            enum_name,
            "EnumFromStringify: at least one variant must be tagged with #[from_stringify(\"Type\")]",
        ));
    }

    Ok(quote! { #(#impls)* })
}

// ===========================================================================
// EnumFromTrait
// ===========================================================================

/// Generates a trait implementation for each variant tagged with
/// `#[from_trait(Trait::method)]`. The trait method must take a single
/// argument matching the variant's inner type and return `Self`.
///
/// # Example
/// ```ignore
/// trait WithMessage { fn with_message(msg: String) -> Self; }
///
/// #[derive(EnumFromTrait)]
/// enum Err {
///     #[from_trait(WithMessage::with_message)]
///     Message(String),
/// }
/// // produces:
/// // impl WithMessage for Err {
/// //     fn with_message(val: String) -> Self { Self::Message(val) }
/// // }
/// ```
#[proc_macro_derive(EnumFromTrait, attributes(from_trait))]
pub fn derive_from_trait(tokens: TokenStream) -> TokenStream {
    let input = parse_macro_input!(tokens as DeriveInput);
    match expand_from_trait(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_from_trait(input: &DeriveInput) -> Result<TokenStream2, Error> {
    let data = require_enum(input)?;
    let enum_name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let mut impls = Vec::new();

    for variant in &data.variants {
        let matching = attrs_named(variant, "from_trait");
        if matching.is_empty() {
            continue;
        }

        let inner_ty = single_unnamed_field(variant)?;
        let var_name = &variant.ident;

        for attr in matching {
            // Parse "Trait::method" path.
            let trait_method: syn::Path = attr.parse_args()?;
            let segments: Vec<_> = trait_method.segments.iter().collect();
            if segments.len() < 2 {
                return Err(Error::new_spanned(
                    &trait_method,
                    "expected Trait::method format (e.g. WithInternal::internal)",
                ));
            }

            // Last segment is the method, everything before is the trait path.
            let method_ident = &segments.last().unwrap().ident;
            let trait_segments = &segments[..segments.len() - 1];
            let trait_path: syn::Path = {
                let mut p = syn::Path {
                    leading_colon: trait_method.leading_colon,
                    segments: syn::punctuated::Punctuated::new(),
                };
                for (i, seg) in trait_segments.iter().enumerate() {
                    p.segments.push((*seg).clone());
                    if i + 1 < trait_segments.len() {
                        p.segments.push_punct(syn::token::PathSep::default());
                    }
                }
                p
            };

            impls.push(quote! {
                impl #impl_generics #trait_path for #enum_name #ty_generics #where_clause {
                    fn #method_ident(val: #inner_ty) -> Self {
                        Self::#var_name(val)
                    }
                }
            });
        }
    }

    if impls.is_empty() {
        return Err(Error::new_spanned(
            enum_name,
            "EnumFromTrait: at least one variant must be tagged with #[from_trait(Trait::method)]",
        ));
    }

    Ok(quote! { #(#impls)* })
}

// ===========================================================================
// EnumVariantList
// ===========================================================================

/// Generates a `variant_list()` method that returns a `Vec` of all unit
/// variants.
///
/// # Example
/// ```ignore
/// #[derive(Clone, EnumVariantList)]
/// enum Color { Red, Green, Blue }
/// assert_eq!(Color::variant_list(), vec![Color::Red, Color::Green, Color::Blue]);
/// ```
#[proc_macro_derive(EnumVariantList)]
pub fn derive_variant_list(tokens: TokenStream) -> TokenStream {
    let input = parse_macro_input!(tokens as DeriveInput);
    match expand_variant_list(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_variant_list(input: &DeriveInput) -> Result<TokenStream2, Error> {
    let data = require_enum(input)?;
    let enum_name = &input.ident;

    let variants: Vec<_> = data
        .variants
        .iter()
        .map(|v| {
            if !matches!(v.fields, Fields::Unit) {
                return Err(Error::new_spanned(
                    &v.ident,
                    "EnumVariantList: all variants must be unit variants (no fields)",
                ));
            }
            Ok(&v.ident)
        })
        .collect::<Result<_, _>>()?;

    Ok(quote! {
        impl #enum_name {
            pub fn variant_list() -> Vec<#enum_name> {
                vec![#( #enum_name::#variants ),*]
            }
        }
    })
}
