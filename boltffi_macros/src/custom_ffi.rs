use proc_macro::TokenStream;
use quote::quote;
use syn::{ImplItem, ItemImpl, Type, parse_macro_input};

pub fn custom_ffi_impl(item: TokenStream) -> TokenStream {
    let item_impl = parse_macro_input!(item as ItemImpl);

    if !item_impl.generics.params.is_empty() {
        return syn::Error::new_spanned(&item_impl.generics, "custom_ffi does not support generics")
            .to_compile_error()
            .into();
    }

    let trait_ident_ok = item_impl
        .trait_
        .as_ref()
        .and_then(|(_, path, _)| path.segments.last())
        .is_some_and(|seg| seg.ident == "CustomFfiConvertible");

    if !trait_ident_ok {
        return syn::Error::new_spanned(&item_impl, "custom_ffi must annotate an impl of CustomFfiConvertible")
            .to_compile_error()
            .into();
    }

    let self_ty = item_impl.self_ty.as_ref();

    let ffi_repr = item_impl
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Type(assoc) if assoc.ident == "FfiRepr" => Some(&assoc.ty),
            _ => None,
        })
        .next();

    let Some(ffi_repr) = ffi_repr else {
        return syn::Error::new_spanned(&item_impl, "custom_ffi requires `type FfiRepr = ...;` in the impl block")
            .to_compile_error()
            .into();
    };

    let self_ty_no_group = match self_ty {
        Type::Group(group) => group.elem.as_ref(),
        _ => self_ty,
    };

    let expanded = quote! {
        #item_impl

        impl ::boltffi::__private::wire::WireSize for #self_ty_no_group {
            #[inline]
            fn is_fixed_size() -> bool {
                <#ffi_repr as ::boltffi::__private::wire::WireSize>::is_fixed_size()
            }

            #[inline]
            fn fixed_size() -> Option<usize> {
                <#ffi_repr as ::boltffi::__private::wire::WireSize>::fixed_size()
            }

            #[inline]
            fn wire_size(&self) -> usize {
                let repr = <#self_ty_no_group as ::boltffi::CustomFfiConvertible>::into_ffi(self);
                <#ffi_repr as ::boltffi::__private::wire::WireSize>::wire_size(&repr)
            }
        }

        impl ::boltffi::__private::wire::WireEncode for #self_ty_no_group {
            #[inline]
            fn encode_to(&self, buf: &mut [u8]) -> usize {
                let repr = <#self_ty_no_group as ::boltffi::CustomFfiConvertible>::into_ffi(self);
                <#ffi_repr as ::boltffi::__private::wire::WireEncode>::encode_to(&repr, buf)
            }
        }

        impl ::boltffi::__private::wire::WireDecode for #self_ty_no_group {
            #[inline]
            fn decode_from(buf: &[u8]) -> ::boltffi::__private::wire::DecodeResult<Self> {
                let (repr, used) = <#ffi_repr as ::boltffi::__private::wire::WireDecode>::decode_from(buf)?;
                let value = <#self_ty_no_group as ::boltffi::CustomFfiConvertible>::try_from_ffi(repr)
                    .map_err(|_| ::boltffi::__private::wire::DecodeError::InvalidValue)?;
                Ok((value, used))
            }
        }
    };

    expanded.into()
}
