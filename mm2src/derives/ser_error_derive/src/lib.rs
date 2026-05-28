//! # `ser_error_derive` — proc-macro for `#[derive(SerializeErrorType)]`
//!
//! Companion crate to [`ser_error`]. Emits a sealed marker
//! implementation of `ser_error::__private::SerializeErrorTypeImpl` for
//! the annotated type, **after** verifying at compile time that the
//! type carries the right `#[serde(tag = "...", content = "...")]`
//! attributes.
//!
//! # Validation
//!
//! - Only `enum` types are accepted (struct / union are rejected with
//!   a compile error).
//! - The `#[serde(untagged)]` attribute is rejected (only adjacent-
//!   tagged enums produce the `{ error_type, error_data }` wire shape).
//! - The `tag` and `content` literals must equal [`ser_error::TAG`]
//!   (`"error_type"`) and [`ser_error::CONTENT`] (`"error_data"`)
//!   respectively. Anything else is a hard error.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::Meta::{List, NameValue, Path};
use syn::{parse_macro_input, Data, DeriveInput, Error, NestedMeta};

use ser_error::{CONTENT, TAG};

// ---------------------------------------------------------------------------
// Attribute identifier constants
// ---------------------------------------------------------------------------

const SERDE_IDENT: &str = "serde";
const TAG_ATTR: &str = "tag";
const CONTENT_ATTR: &str = "content";
const UNTAGGED_ATTR: &str = "untagged";

// ---------------------------------------------------------------------------
// Compile-error helper
// ---------------------------------------------------------------------------

/// Wraps a string for emission as a `compile_error!(...)` token tree.
struct CompileError(String);

macro_rules! compile_err {
    ($($arg:tt)*) => { $crate::CompileError(format!($($arg)*)) };
}

impl From<CompileError> for TokenStream2 {
    fn from(e: CompileError) -> Self { Error::new(Span::call_site(), e.0).to_compile_error() }
}

impl From<CompileError> for TokenStream {
    fn from(e: CompileError) -> Self { TokenStream2::from(e).into() }
}

// ---------------------------------------------------------------------------
// Derive entry point
// ---------------------------------------------------------------------------

/// `#[derive(SerializeErrorType)]` — see the crate-level docs and
/// [`ser_error::SerializeErrorType`] for the full contract.
#[proc_macro_derive(SerializeErrorType, attributes(serde))]
pub fn serialize_error_type(input: TokenStream) -> TokenStream {
    let input: DeriveInput = parse_macro_input!(input);

    // Only enums are supported, and only when the right serde attrs are present.
    match &input.data {
        Data::Enum(_) => {
            if let Err(e) = check_enum_attributes(&input) {
                return e.into();
            }
        },
        Data::Struct(_) => return compile_err!("'SerializeErrorType' cannot be implement for a struct yet").into(),
        Data::Union(_) => return compile_err!("'SerializeErrorType' cannot be implement for a union").into(),
    }

    let ident = input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    // The single line of generated code: an empty marker impl that the
    // ser_error blanket impl picks up. Wrapped inside an anonymous const so
    // the `extern crate ser_error;` does not pollute the caller's namespace.
    wrap_in_anon_const(quote! {
        #[automatically_derived]
        impl #impl_generics ser_error::__private::SerializeErrorTypeImpl for #ident #type_generics #where_clause {}
    })
}

// ---------------------------------------------------------------------------
// Attribute validation
// ---------------------------------------------------------------------------

/// Walk the enum's `#[serde(...)]` attributes once and confirm
/// `tag = "error_type", content = "error_data"` is present and that
/// `untagged` is absent.
fn check_enum_attributes(input: &DeriveInput) -> Result<(), CompileError> {
    let mut tag: Option<String> = None;
    let mut content: Option<String> = None;

    for meta_item in input.attrs.iter().flat_map(serde_meta_items) {
        match meta_item {
            NestedMeta::Meta(NameValue(m)) if m.path.is_ident(TAG_ATTR) => {
                tag = Some(parse_lit_str(TAG_ATTR, m.lit)?);
            },
            NestedMeta::Meta(NameValue(m)) if m.path.is_ident(CONTENT_ATTR) => {
                content = Some(parse_lit_str(CONTENT_ATTR, m.lit)?);
            },
            NestedMeta::Meta(Path(word)) if word.is_ident(UNTAGGED_ATTR) => {
                return Err(compile_err!(
                    "'SerializeErrorType' can be implemented for tagged enum only"
                ));
            },
            _ => {},
        }
    }

    expect_literal(TAG_ATTR, TAG, tag.as_deref())?;
    expect_literal(CONTENT_ATTR, CONTENT, content.as_deref())?;
    Ok(())
}

/// Confirm `actual` (if present) equals `expected`; otherwise emit an
/// `expected … found …` compile error mentioning `attr_name`.
fn expect_literal(attr_name: &str, expected: &str, actual: Option<&str>) -> Result<(), CompileError> {
    match actual {
        Some(found) if found == expected => Ok(()),
        Some(found) => Err(compile_err!(
            "'SerializeErrorType': expected {attr} = \"{exp}\", found {attr} = \"{got}\"",
            attr = attr_name,
            exp = expected,
            got = found,
        )),
        None => Err(compile_err!(
            "'SerializeErrorType': expected #[serde({attr} = \"{exp}\")]",
            attr = attr_name,
            exp = expected,
        )),
    }
}

// ---------------------------------------------------------------------------
// Low-level syn helpers
// ---------------------------------------------------------------------------

/// Extract the comma-separated items inside `#[serde(...)]`. Non-list
/// `serde` attributes (e.g. unrelated `#[doc = "..."]`) are ignored.
fn serde_meta_items(attr: &syn::Attribute) -> Vec<NestedMeta> {
    if !attr.path.is_ident(SERDE_IDENT) {
        return Vec::new();
    }
    match attr.parse_meta() {
        Ok(List(meta)) => meta.nested.into_iter().collect(),
        _ => Vec::new(),
    }
}

/// Pull the string body out of `#[serde(name = "...")]`.
fn parse_lit_str(attr_ident: &str, lit: syn::Lit) -> Result<String, CompileError> {
    match lit {
        syn::Lit::Str(lit) => Ok(lit.value()),
        _ => Err(compile_err!(
            "expected serde '{}' attribute to be a string: `{} = \"...\"`",
            attr_ident,
            attr_ident
        )),
    }
}

// ---------------------------------------------------------------------------
// Codegen helper
// ---------------------------------------------------------------------------

/// Wrap `code` inside an anonymous `const _: () = { extern crate
/// ser_error; ... };` block so the marker impl can name
/// `ser_error::__private::...` regardless of whether the consuming
/// crate has imported `ser_error` directly.
fn wrap_in_anon_const(code: TokenStream2) -> TokenStream {
    quote! {
        const _: () = {
            extern crate ser_error;
            #code
        };
    }
    .into()
}
