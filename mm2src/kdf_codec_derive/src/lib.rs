//! Procedural `#[derive(Serializable)]` and `#[derive(Deserializable)]`
//! macros for the KDF binary codec.
//!
//! Both macros are defined for structs only (named or tuple), and treat
//! one field type specially: any field whose outermost type segment is
//! `Vec` is encoded as a length-prefixed list (`stream.append_list` /
//! `reader.read_list`); every other field uses the `Serializable` /
//! `Deserializable` impl directly.
//!
//! The generated code refers to the codec by the dependency name
//! `serialization` (currently the package-renamed alias of `kdf_codec`)
//! so that consumers do not need any source change.
//!
//! KDF-original. Phase B (B.5) replacement for
//! `mm2_bitcoin/serialization_derive`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, Data, DataStruct, DeriveInput, Field, Fields, Index, Type, TypePath};

#[proc_macro_derive(Serializable)]
pub fn derive_serializable(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    expand_serializable(&ast).into()
}

#[proc_macro_derive(Deserializable)]
pub fn derive_deserializable(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    expand_deserializable(&ast).into()
}

// ── Serializable ────────────────────────────────────────────────────────

fn expand_serializable(ast: &DeriveInput) -> TokenStream2 {
    let name = &ast.ident;
    let fields = require_struct_fields(ast, "Serializable");

    let writes = fields.iter().enumerate().map(|(idx, f)| {
        let access = field_access(idx, f);
        if is_vec_field(&f.ty) {
            quote! { stream.append_list(&#access); }
        } else {
            quote! { stream.append(&#access); }
        }
    });

    let sizes = fields.iter().enumerate().map(|(idx, f)| {
        let access = field_access(idx, f);
        if is_vec_field(&f.ty) {
            quote! { ::serialization::serialized_list_size(&#access) }
        } else {
            quote! { #access.serialized_size() }
        }
    });

    quote! {
        impl ::serialization::Serializable for #name {
            fn serialize(&self, stream: &mut ::serialization::Stream) {
                #( #writes )*
            }

            fn serialized_size(&self) -> usize {
                #( #sizes )+*
            }
        }
    }
}

// ── Deserializable ───────────────────────────────────────────────────────

fn expand_deserializable(ast: &DeriveInput) -> TokenStream2 {
    let name = &ast.ident;
    let fields = require_struct_fields(ast, "Deserializable");

    // Both named structs and tuple structs accept the brace-init syntax
    // `Self { field_or_index: expr, ... }`, so we emit one shape for both.
    let reads = fields.iter().enumerate().map(|(idx, f)| {
        let read_expr = if is_vec_field(&f.ty) {
            quote! { reader.read_list()? }
        } else {
            quote! { reader.read()? }
        };

        match f.ident.as_ref() {
            Some(ident) => quote! { #ident: #read_expr, },
            None => {
                let idx_lit = Index::from(idx);
                quote! { #idx_lit: #read_expr, }
            },
        }
    });

    quote! {
        impl ::serialization::Deserializable for #name {
            fn deserialize<__T>(
                reader: &mut ::serialization::Reader<__T>,
            ) -> ::std::result::Result<Self, ::serialization::Error>
            where
                __T: ::std::io::Read,
            {
                Ok(#name { #( #reads )* })
            }
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────────────

fn require_struct_fields<'a>(ast: &'a DeriveInput, trait_name: &str) -> Vec<&'a Field> {
    match &ast.data {
        Data::Struct(DataStruct { fields, .. }) => match fields {
            Fields::Named(named) => named.named.iter().collect(),
            Fields::Unnamed(unnamed) => unnamed.unnamed.iter().collect(),
            Fields::Unit => panic!("#[derive({})] is not defined for unit structs.", trait_name),
        },
        _ => panic!("#[derive({})] is only defined for structs.", trait_name),
    }
}

/// Render the `self.<field>` accessor for use inside the generated impl.
fn field_access(idx: usize, field: &Field) -> TokenStream2 {
    match field.ident.as_ref() {
        Some(ident) => quote! { self.#ident },
        None => {
            let i = Index::from(idx);
            quote! { self.#i }
        },
    }
}

/// True iff the outermost type segment is the bare identifier `Vec`.
/// Mirrors the lookup the original macro performed.
fn is_vec_field(ty: &Type) -> bool {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(seg) = path.segments.first() {
            return seg.ident == "Vec";
        }
    }
    false
}
