use crate::custom_types::{CustomTypeRegistry, contains_custom_types};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, ItemEnum, ItemStruct, Type};

pub fn is_primitive_type(ty: &Type) -> bool {
    match ty {
        Type::Path(type_path) => {
            if let Some(ident) = type_path.path.get_ident() {
                let name = ident.to_string();
                matches!(
                    name.as_str(),
                    "bool" | "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "f32" | "f64" | "isize" | "usize"
                )
            } else {
                false
            }
        }
        _ => false,
    }
}

pub fn is_struct_blittable(field_types: &[&Type]) -> bool {
    field_types.iter().all(|ty| is_primitive_type(ty))
}

pub fn generate_wire_impls(item_struct: &ItemStruct, custom_types: &CustomTypeRegistry) -> TokenStream {
    let struct_name = &item_struct.ident;
    let (impl_generics, ty_generics, where_clause) = item_struct.generics.split_for_impl();

    let fields = match &item_struct.fields {
        Fields::Named(named) => &named.named,
        _ => return quote! {},
    };

    if fields.is_empty() {
        return generate_empty_struct_impls(struct_name);
    }

    let field_names: Vec<_> = fields.iter().filter_map(|f| f.ident.as_ref()).collect();

    let field_types: Vec<_> = fields.iter().map(|f| &f.ty).collect();

    let is_blittable = is_struct_blittable(&field_types);

    let wire_size_impl = generate_wire_size_impl(
        struct_name,
        &impl_generics,
        &ty_generics,
        where_clause,
        &field_names,
        &field_types,
        custom_types,
        is_blittable,
    );

    let wire_encode_impl = generate_wire_encode_impl(
        struct_name,
        &impl_generics,
        &ty_generics,
        where_clause,
        &field_names,
        &field_types,
        custom_types,
        is_blittable,
    );

    let wire_decode_impl = generate_wire_decode_impl(
        struct_name,
        &impl_generics,
        &ty_generics,
        where_clause,
        &field_names,
        &field_types,
        custom_types,
        is_blittable,
    );

    let blittable_impl = if is_blittable {
        quote! {
            unsafe impl #impl_generics ::boltffi::__private::wire::Blittable for #struct_name #ty_generics #where_clause {}
        }
    } else {
        quote! {}
    };

    quote! {
        #wire_size_impl
        #wire_encode_impl
        #wire_decode_impl
        #blittable_impl
    }
}

fn generate_empty_struct_impls(struct_name: &syn::Ident) -> TokenStream {
    quote! {
        impl ::boltffi::__private::wire::WireSize for #struct_name {
            fn is_fixed_size() -> bool { true }
            fn fixed_size() -> Option<usize> { Some(2) }
            fn wire_size(&self) -> usize { 2 }
        }

        impl ::boltffi::__private::wire::WireEncode for #struct_name {
            fn encode_to(&self, buf: &mut [u8]) -> usize {
                buf[0..2].copy_from_slice(&0u16.to_le_bytes());
                2
            }
        }

        impl ::boltffi::__private::wire::WireDecode for #struct_name {
            fn decode_from(buf: &[u8]) -> ::boltffi::__private::wire::DecodeResult<Self> {
                if buf.len() < 2 {
                    return Err(::boltffi::__private::wire::DecodeError::BufferTooSmall);
                }
                Ok((Self {}, 2))
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn generate_wire_size_impl(
    struct_name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    field_names: &[&syn::Ident],
    field_types: &[&Type],
    custom_types: &CustomTypeRegistry,
    is_blittable: bool,
) -> TokenStream {
    if is_blittable {
        return quote! {
            impl #impl_generics ::boltffi::__private::wire::WireSize for #struct_name #ty_generics #where_clause {
                fn is_fixed_size() -> bool { true }
                fn fixed_size() -> Option<usize> { Some(::core::mem::size_of::<Self>()) }
                fn wire_size(&self) -> usize { ::core::mem::size_of::<Self>() }
            }
        };
    }

    let all_fixed_check = field_types.iter().map(|ty| {
        if contains_custom_types(ty, custom_types) {
            let wire_ty = wire_type_for(ty, custom_types);
            quote! { <#wire_ty as ::boltffi::__private::wire::WireSize>::is_fixed_size() }
        } else {
            quote! { <#ty as ::boltffi::__private::wire::WireSize>::is_fixed_size() }
        }
    });

    let fixed_size_sum = field_types.iter().map(|ty| {
        if contains_custom_types(ty, custom_types) {
            let wire_ty = wire_type_for(ty, custom_types);
            quote! { <#wire_ty as ::boltffi::__private::wire::WireSize>::fixed_size().unwrap_or(0) }
        } else {
            quote! { <#ty as ::boltffi::__private::wire::WireSize>::fixed_size().unwrap_or(0) }
        }
    });

    let field_wire_sizes = field_names
        .iter()
        .zip(field_types.iter())
        .map(|(name, ty)| wire_size_expr(ty, custom_types, quote! { &self.#name }));

    quote! {
        impl #impl_generics ::boltffi::__private::wire::WireSize for #struct_name #ty_generics #where_clause {
            fn is_fixed_size() -> bool {
                #(#all_fixed_check)&&*
            }

            fn fixed_size() -> Option<usize> {
                if <Self as ::boltffi::__private::wire::WireSize>::is_fixed_size() {
                    Some(#(#fixed_size_sum)+*)
                } else {
                    None
                }
            }

            fn wire_size(&self) -> usize {
                <Self as ::boltffi::__private::wire::WireSize>::fixed_size().unwrap_or_else(|| {
                    #(#field_wire_sizes)+*
                })
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn generate_wire_encode_impl(
    struct_name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    field_names: &[&syn::Ident],
    field_types: &[&Type],
    custom_types: &CustomTypeRegistry,
    is_blittable: bool,
) -> TokenStream {
    if is_blittable {
        return quote! {
            impl #impl_generics ::boltffi::__private::wire::WireEncode for #struct_name #ty_generics #where_clause {
                const IS_BLITTABLE: bool = true;

                fn encode_to(&self, buf: &mut [u8]) -> usize {
                    let size = ::core::mem::size_of::<Self>();
                    let src = self as *const Self as *const u8;
                    unsafe {
                        ::core::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), size);
                    }
                    size
                }
            }
        };
    }

    let encode_fields = field_names.iter().zip(field_types.iter()).map(|(name, ty)| {
        let field_buf = syn::Ident::new(&format!("__boltffi_buf_{}", name), name.span());
        let encode_expr = encode_to_expr(ty, custom_types, quote! { &self.#name }, quote! { #field_buf });
        quote! {
            let #field_buf = &mut buf[written..];
            written += #encode_expr;
        }
    });

    quote! {
        impl #impl_generics ::boltffi::__private::wire::WireEncode for #struct_name #ty_generics #where_clause {
            fn encode_to(&self, buf: &mut [u8]) -> usize {
                let mut written = 0usize;
                #(#encode_fields)*
                written
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn generate_wire_decode_impl(
    struct_name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    field_names: &[&syn::Ident],
    field_types: &[&Type],
    custom_types: &CustomTypeRegistry,
    is_blittable: bool,
) -> TokenStream {
    let field_names_for_struct: Vec<_> = field_names.iter().map(|n| quote! { #n }).collect();

    if is_blittable {
        return quote! {
            impl #impl_generics ::boltffi::__private::wire::WireDecode for #struct_name #ty_generics #where_clause {
                const IS_BLITTABLE: bool = true;

                fn decode_from(buf: &[u8]) -> ::boltffi::__private::wire::DecodeResult<Self> {
                    let size = ::core::mem::size_of::<Self>();
                    if buf.len() < size {
                        return Err(::boltffi::__private::wire::DecodeError::BufferTooSmall);
                    }
                    let value = unsafe { ::core::ptr::read_unaligned(buf.as_ptr() as *const Self) };
                    Ok((value, size))
                }
            }
        };
    }

    let struct_name_str = struct_name.to_string();
    let decode_fields = field_names.iter().zip(field_types.iter()).map(|(name, ty)| {
        let decode_expr = decode_from_expr(ty, custom_types, quote! { &buf[position..] });
        let field_name_str = name.to_string();
        let struct_name_lit = &struct_name_str;
        quote! {
            let (#name, size) = #decode_expr.map_err(|e| {
                eprintln!("[boltffi] wire decode error in {}.{} at position {} (buf_len={}): {:?}",
                    #struct_name_lit, #field_name_str, position, buf.len(), e);
                e
            })?;
            position += size;
        }
    });

    quote! {
        impl #impl_generics ::boltffi::__private::wire::WireDecode for #struct_name #ty_generics #where_clause {
            const IS_BLITTABLE: bool = false;

            fn decode_from(buf: &[u8]) -> ::boltffi::__private::wire::DecodeResult<Self> {
                let mut position = 0usize;
                #(#decode_fields)*
                Ok((Self { #(#field_names_for_struct),* }, position))
            }
        }
    }
}

fn wire_type_for(ty: &Type, custom_types: &CustomTypeRegistry) -> Type {
    if let Some(entry) = custom_types.lookup(ty) {
        return entry.repr_type().unwrap_or_else(|_| ty.clone());
    }

    let Type::Path(type_path) = ty else {
        return ty.clone();
    };

    let Some(segment) = type_path.path.segments.last() else {
        return ty.clone();
    };

    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return ty.clone();
    };

    match segment.ident.to_string().as_str() {
        "Vec" => args
            .args
            .first()
            .and_then(type_arg)
            .map(|inner| {
                let inner_wire = wire_type_for(inner, custom_types);
                syn::parse_quote!(Vec<#inner_wire>)
            })
            .unwrap_or_else(|| ty.clone()),
        "Option" => args
            .args
            .first()
            .and_then(type_arg)
            .map(|inner| {
                let inner_wire = wire_type_for(inner, custom_types);
                syn::parse_quote!(Option<#inner_wire>)
            })
            .unwrap_or_else(|| ty.clone()),
        "Result" => {
            let ok = args.args.first().and_then(type_arg);
            let err = args.args.iter().nth(1).and_then(type_arg);
            match (ok, err) {
                (Some(ok), Some(err)) => {
                    let ok_wire = wire_type_for(ok, custom_types);
                    let err_wire = wire_type_for(err, custom_types);
                    syn::parse_quote!(Result<#ok_wire, #err_wire>)
                }
                _ => ty.clone(),
            }
        }
        _ => ty.clone(),
    }
}

fn type_arg(arg: &syn::GenericArgument) -> Option<&Type> {
    match arg {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    }
}

fn wire_size_expr(ty: &Type, custom_types: &CustomTypeRegistry, value_expr: TokenStream) -> TokenStream {
    if let Some(entry) = custom_types.lookup(ty) {
        let into_fn = entry.to_fn_path();
        return quote! { ::boltffi::__private::wire::WireSize::wire_size(&#into_fn(#value_expr)) };
    }

    let Type::Path(type_path) = ty else {
        return quote! { ::boltffi::__private::wire::WireSize::wire_size(#value_expr) };
    };

    let Some(segment) = type_path.path.segments.last() else {
        return quote! { ::boltffi::__private::wire::WireSize::wire_size(#value_expr) };
    };

    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return quote! { ::boltffi::__private::wire::WireSize::wire_size(#value_expr) };
    };

    match segment.ident.to_string().as_str() {
        "Vec" => args
            .args
            .first()
            .and_then(type_arg)
            .filter(|inner| contains_custom_types(inner, custom_types))
            .map(|inner| {
                let inner_size = wire_size_expr(inner, custom_types, quote! { element });
                quote! {
                    ::boltffi::__private::wire::VEC_COUNT_SIZE
                        + (#value_expr)
                            .iter()
                            .map(|element| #inner_size)
                            .sum::<usize>()
                }
            })
            .unwrap_or_else(|| quote! { ::boltffi::__private::wire::WireSize::wire_size(#value_expr) }),
        "Option" => args
            .args
            .first()
            .and_then(type_arg)
            .filter(|inner| contains_custom_types(inner, custom_types))
            .map(|inner| {
                let inner_size = wire_size_expr(inner, custom_types, quote! { value });
                quote! {
                    match #value_expr {
                        Some(value) => ::boltffi::__private::wire::OPTION_FLAG_SIZE + #inner_size,
                        None => ::boltffi::__private::wire::OPTION_FLAG_SIZE,
                    }
                }
            })
            .unwrap_or_else(|| quote! { ::boltffi::__private::wire::WireSize::wire_size(#value_expr) }),
        "Result" => {
            let ok = args.args.first().and_then(type_arg);
            let err = args.args.iter().nth(1).and_then(type_arg);

            match (ok, err) {
                (Some(ok), Some(err)) if contains_custom_types(ok, custom_types) || contains_custom_types(err, custom_types) => {
                    let ok_size = wire_size_expr(ok, custom_types, quote! { ok_value });
                    let err_size = wire_size_expr(err, custom_types, quote! { err_value });
                    quote! {
                        match #value_expr {
                            Ok(ok_value) => ::boltffi::__private::wire::RESULT_TAG_SIZE + #ok_size,
                            Err(err_value) => ::boltffi::__private::wire::RESULT_TAG_SIZE + #err_size,
                        }
                    }
                }
                _ => quote! { ::boltffi::__private::wire::WireSize::wire_size(#value_expr) },
            }
        }
        _ => quote! { ::boltffi::__private::wire::WireSize::wire_size(#value_expr) },
    }
}

fn encode_to_expr(ty: &Type, custom_types: &CustomTypeRegistry, value_expr: TokenStream, buf_expr: TokenStream) -> TokenStream {
    if let Some(entry) = custom_types.lookup(ty) {
        let into_fn = entry.to_fn_path();
        return quote! {
            {
                let __boltffi_custom_value = #into_fn(#value_expr);
                ::boltffi::__private::wire::WireEncode::encode_to(&__boltffi_custom_value, #buf_expr)
            }
        };
    }

    let Type::Path(type_path) = ty else {
        return quote! { ::boltffi::__private::wire::WireEncode::encode_to(#value_expr, #buf_expr) };
    };

    let Some(segment) = type_path.path.segments.last() else {
        return quote! { ::boltffi::__private::wire::WireEncode::encode_to(#value_expr, #buf_expr) };
    };

    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return quote! { ::boltffi::__private::wire::WireEncode::encode_to(#value_expr, #buf_expr) };
    };

    match segment.ident.to_string().as_str() {
        "Vec" => args
            .args
            .first()
            .and_then(type_arg)
            .filter(|inner| contains_custom_types(inner, custom_types))
            .map(|inner| {
                let inner_encode = encode_to_expr(
                    inner,
                    custom_types,
                    quote! { element },
                    quote! { &mut #buf_expr[::boltffi::__private::wire::VEC_COUNT_SIZE + offset..] },
                );
                quote! {
                    {
                        let count = (#value_expr).len() as u32;
                        #buf_expr[..::boltffi::__private::wire::VEC_COUNT_SIZE].copy_from_slice(&count.to_le_bytes());
                        let payload_written = (#value_expr).iter().fold(0usize, |offset, element| {
                            offset + #inner_encode
                        });
                        ::boltffi::__private::wire::VEC_COUNT_SIZE + payload_written
                    }
                }
            })
            .unwrap_or_else(|| quote! { ::boltffi::__private::wire::WireEncode::encode_to(#value_expr, #buf_expr) }),
        "Option" => args
            .args
            .first()
            .and_then(type_arg)
            .filter(|inner| contains_custom_types(inner, custom_types))
            .map(|inner| {
                let inner_encode = encode_to_expr(
                    inner,
                    custom_types,
                    quote! { value },
                    quote! { &mut #buf_expr[::boltffi::__private::wire::OPTION_FLAG_SIZE..] },
                );
                quote! {
                    match #value_expr {
                        Some(value) => {
                            #buf_expr[0] = 1;
                            ::boltffi::__private::wire::OPTION_FLAG_SIZE + #inner_encode
                        }
                        None => {
                            #buf_expr[0] = 0;
                            ::boltffi::__private::wire::OPTION_FLAG_SIZE
                        }
                    }
                }
            })
            .unwrap_or_else(|| quote! { ::boltffi::__private::wire::WireEncode::encode_to(#value_expr, #buf_expr) }),
        "Result" => {
            let ok = args.args.first().and_then(type_arg);
            let err = args.args.iter().nth(1).and_then(type_arg);

            match (ok, err) {
                (Some(ok), Some(err)) if contains_custom_types(ok, custom_types) || contains_custom_types(err, custom_types) => {
                    let ok_encode = encode_to_expr(
                        ok,
                        custom_types,
                        quote! { ok_value },
                        quote! { &mut #buf_expr[::boltffi::__private::wire::RESULT_TAG_SIZE..] },
                    );
                    let err_encode = encode_to_expr(
                        err,
                        custom_types,
                        quote! { err_value },
                        quote! { &mut #buf_expr[::boltffi::__private::wire::RESULT_TAG_SIZE..] },
                    );
                    quote! {
                        match #value_expr {
                            Ok(ok_value) => {
                                #buf_expr[0] = 0;
                                ::boltffi::__private::wire::RESULT_TAG_SIZE + #ok_encode
                            }
                            Err(err_value) => {
                                #buf_expr[0] = 1;
                                ::boltffi::__private::wire::RESULT_TAG_SIZE + #err_encode
                            }
                        }
                    }
                }
                _ => quote! { ::boltffi::__private::wire::WireEncode::encode_to(#value_expr, #buf_expr) },
            }
        }
        _ => quote! { ::boltffi::__private::wire::WireEncode::encode_to(#value_expr, #buf_expr) },
    }
}

fn decode_from_expr(ty: &Type, custom_types: &CustomTypeRegistry, buf_expr: TokenStream) -> TokenStream {
    if let Some(entry) = custom_types.lookup(ty) {
        let repr_ty = entry.repr_type().unwrap_or_else(|_| syn::parse_quote!(()));
        let try_from_fn = entry.try_from_fn_path();
        return quote! {
            {
                match <#repr_ty as ::boltffi::__private::wire::WireDecode>::decode_from(#buf_expr) {
                    Ok((repr_value, used)) => match #try_from_fn(repr_value) {
                        Ok(value) => Ok((value, used)),
                        Err(_) => Err(::boltffi::__private::wire::DecodeError::InvalidValue),
                    },
                    Err(error) => Err(error),
                }
            }
        };
    }

    let Type::Path(type_path) = ty else {
        return quote! { <#ty as ::boltffi::__private::wire::WireDecode>::decode_from(#buf_expr) };
    };

    let Some(segment) = type_path.path.segments.last() else {
        return quote! { <#ty as ::boltffi::__private::wire::WireDecode>::decode_from(#buf_expr) };
    };

    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return quote! { <#ty as ::boltffi::__private::wire::WireDecode>::decode_from(#buf_expr) };
    };

    match segment.ident.to_string().as_str() {
        "Vec" => args
            .args
            .first()
            .and_then(type_arg)
            .filter(|inner| contains_custom_types(inner, custom_types))
            .map(|inner| {
                let inner_decode = decode_from_expr(inner, custom_types, quote! { inner_buf });
                quote! {
                    {
                        let buffer = #buf_expr;
                        let (count, count_used) = <u32 as ::boltffi::__private::wire::WireDecode>::decode_from(buffer)?;
                        let count = count as usize;
                        let initial = (Vec::with_capacity(count), 0usize);
                        let (values, payload_used) = (0..count).try_fold(initial, |(mut values, offset), _| {
                            let inner_buf = buffer.get(count_used + offset..).ok_or(::boltffi::__private::wire::DecodeError::BufferTooSmall)?;
                            let (value, used) = #inner_decode?;
                            values.push(value);
                            Ok((values, offset + used))
                        })?;
                        Ok((values, count_used + payload_used))
                    }
                }
            })
            .unwrap_or_else(|| quote! { <#ty as ::boltffi::__private::wire::WireDecode>::decode_from(#buf_expr) }),
        "Option" => args
            .args
            .first()
            .and_then(type_arg)
            .filter(|inner| contains_custom_types(inner, custom_types))
            .map(|inner| {
                let inner_decode = decode_from_expr(inner, custom_types, quote! { inner_buf });
                quote! {
                    {
                        let buffer = #buf_expr;
                        if buffer.is_empty() {
                            Err(::boltffi::__private::wire::DecodeError::BufferTooSmall)
                        } else {
                            match buffer[0] {
                                0 => Ok((None, ::boltffi::__private::wire::OPTION_FLAG_SIZE)),
                                1 => {
                                    let inner_buf = buffer.get(::boltffi::__private::wire::OPTION_FLAG_SIZE..).ok_or(::boltffi::__private::wire::DecodeError::BufferTooSmall)?;
                                    let (value, used) = #inner_decode?;
                                    Ok((Some(value), ::boltffi::__private::wire::OPTION_FLAG_SIZE + used))
                                }
                                _ => Err(::boltffi::__private::wire::DecodeError::InvalidBool),
                            }
                        }
                    }
                }
            })
            .unwrap_or_else(|| quote! { <#ty as ::boltffi::__private::wire::WireDecode>::decode_from(#buf_expr) }),
        "Result" => {
            let ok = args.args.first().and_then(type_arg);
            let err = args.args.iter().nth(1).and_then(type_arg);

            match (ok, err) {
                (Some(ok), Some(err)) if contains_custom_types(ok, custom_types) || contains_custom_types(err, custom_types) => {
                    let ok_decode = decode_from_expr(ok, custom_types, quote! { inner_buf });
                    let err_decode = decode_from_expr(err, custom_types, quote! { inner_buf });
                    quote! {
                        {
                            let buffer = #buf_expr;
                            if buffer.is_empty() {
                                Err(::boltffi::__private::wire::DecodeError::BufferTooSmall)
                            } else {
                                match buffer[0] {
                                    0 => {
                                        let inner_buf = buffer.get(::boltffi::__private::wire::RESULT_TAG_SIZE..).ok_or(::boltffi::__private::wire::DecodeError::BufferTooSmall)?;
                                        let (value, used) = #ok_decode?;
                                        Ok((Ok(value), ::boltffi::__private::wire::RESULT_TAG_SIZE + used))
                                    }
                                    1 => {
                                        let inner_buf = buffer.get(::boltffi::__private::wire::RESULT_TAG_SIZE..).ok_or(::boltffi::__private::wire::DecodeError::BufferTooSmall)?;
                                        let (value, used) = #err_decode?;
                                        Ok((Err(value), ::boltffi::__private::wire::RESULT_TAG_SIZE + used))
                                    }
                                    _ => Err(::boltffi::__private::wire::DecodeError::InvalidBool),
                                }
                            }
                        }
                    }
                }
                _ => quote! { <#ty as ::boltffi::__private::wire::WireDecode>::decode_from(#buf_expr) },
            }
        }
        _ => quote! { <#ty as ::boltffi::__private::wire::WireDecode>::decode_from(#buf_expr) },
    }
}

pub fn generate_enum_wire_impls(item_enum: &ItemEnum, custom_types: &CustomTypeRegistry) -> TokenStream {
    let enum_name = &item_enum.ident;
    let (impl_generics, ty_generics, where_clause) = item_enum.generics.split_for_impl();

    let variants: Vec<_> = item_enum.variants.iter().collect();

    if variants.is_empty() {
        return quote! {};
    }

    let wire_size_impl = generate_enum_wire_size_impl(enum_name, &impl_generics, &ty_generics, where_clause, &variants, custom_types);

    let wire_encode_impl = generate_enum_wire_encode_impl(enum_name, &impl_generics, &ty_generics, where_clause, &variants, custom_types);

    let wire_decode_impl = generate_enum_wire_decode_impl(enum_name, &impl_generics, &ty_generics, where_clause, &variants, custom_types);

    quote! {
        #wire_size_impl
        #wire_encode_impl
        #wire_decode_impl
    }
}

fn generate_enum_wire_size_impl(
    enum_name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    variants: &[&syn::Variant],
    custom_types: &CustomTypeRegistry,
) -> TokenStream {
    let all_unit = variants.iter().all(|v| v.fields.is_empty());

    let wire_size_arms = variants.iter().map(|variant| {
        let variant_name = &variant.ident;
        match &variant.fields {
            Fields::Unit => {
                quote! { Self::#variant_name => 4 }
            }
            Fields::Unnamed(fields) => {
                let field_types: Vec<_> = fields.unnamed.iter().map(|f| &f.ty).collect();
                let field_bindings: Vec<_> = (0..fields.unnamed.len()).map(|i| quote::format_ident!("f{}", i)).collect();
                let field_wire_sizes = field_bindings
                    .iter()
                    .zip(field_types.iter())
                    .map(|(binding, ty)| wire_size_expr(ty, custom_types, quote! { #binding }));
                quote! {
                    Self::#variant_name(#(#field_bindings),*) => {
                        4 + #( #field_wire_sizes )+*
                    }
                }
            }
            Fields::Named(fields) => {
                let field_names: Vec<_> = fields.named.iter().filter_map(|f| f.ident.as_ref()).collect();
                let field_types: Vec<_> = fields.named.iter().map(|f| &f.ty).collect();
                let field_wire_sizes = field_names
                    .iter()
                    .zip(field_types.iter())
                    .map(|(binding, ty)| wire_size_expr(ty, custom_types, quote! { #binding }));
                quote! {
                    Self::#variant_name { #(#field_names),* } => {
                        4 + #( #field_wire_sizes )+*
                    }
                }
            }
        }
    });

    if all_unit {
        quote! {
            impl #impl_generics ::boltffi::__private::wire::WireSize for #enum_name #ty_generics #where_clause {
                fn is_fixed_size() -> bool { true }
                fn fixed_size() -> Option<usize> { Some(4) }
                fn wire_size(&self) -> usize { 4 }
            }
        }
    } else {
        quote! {
            impl #impl_generics ::boltffi::__private::wire::WireSize for #enum_name #ty_generics #where_clause {
                fn is_fixed_size() -> bool { false }
                fn fixed_size() -> Option<usize> { None }
                fn wire_size(&self) -> usize {
                    match self {
                        #(#wire_size_arms),*
                    }
                }
            }
        }
    }
}

fn generate_enum_wire_encode_impl(
    enum_name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    variants: &[&syn::Variant],
    custom_types: &CustomTypeRegistry,
) -> TokenStream {
    let encode_arms = variants.iter().enumerate().map(|(discriminant, variant)| {
        let variant_name = &variant.ident;
        let discriminant_i32 = discriminant as i32;

        match &variant.fields {
            Fields::Unit => {
                quote! {
                    Self::#variant_name => {
                        buf[0..4].copy_from_slice(&(#discriminant_i32 as i32).to_le_bytes());
                        4
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_types: Vec<_> = fields.unnamed.iter().map(|f| &f.ty).collect();
                let field_bindings: Vec<_> = (0..fields.unnamed.len()).map(|i| quote::format_ident!("f{}", i)).collect();
                let encode_fields = field_bindings.iter().zip(field_types.iter()).map(|(binding, ty)| {
                    let field_buf = quote::format_ident!("__boltffi_buf_{}", binding);
                    let encode_expr = encode_to_expr(ty, custom_types, quote! { #binding }, quote! { #field_buf });
                    quote! {
                        let #field_buf = &mut buf[written..];
                        written += #encode_expr;
                    }
                });
                quote! {
                    Self::#variant_name(#(#field_bindings),*) => {
                        buf[0..4].copy_from_slice(&(#discriminant_i32 as i32).to_le_bytes());
                        let mut written = 4usize;
                        #(#encode_fields)*
                        written
                    }
                }
            }
            Fields::Named(fields) => {
                let field_names: Vec<_> = fields.named.iter().filter_map(|f| f.ident.as_ref()).collect();
                let field_types: Vec<_> = fields.named.iter().map(|f| &f.ty).collect();
                let encode_fields = field_names.iter().zip(field_types.iter()).map(|(binding, ty)| {
                    let field_buf = quote::format_ident!("__boltffi_buf_{}", binding);
                    let encode_expr = encode_to_expr(ty, custom_types, quote! { #binding }, quote! { #field_buf });
                    quote! {
                        let #field_buf = &mut buf[written..];
                        written += #encode_expr;
                    }
                });
                quote! {
                    Self::#variant_name { #(#field_names),* } => {
                        buf[0..4].copy_from_slice(&(#discriminant_i32 as i32).to_le_bytes());
                        let mut written = 4usize;
                        #(#encode_fields)*
                        written
                    }
                }
            }
        }
    });

    quote! {
        impl #impl_generics ::boltffi::__private::wire::WireEncode for #enum_name #ty_generics #where_clause {
            fn encode_to(&self, buf: &mut [u8]) -> usize {
                match self {
                    #(#encode_arms),*
                }
            }
        }
    }
}

fn generate_enum_wire_decode_impl(
    enum_name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    variants: &[&syn::Variant],
    custom_types: &CustomTypeRegistry,
) -> TokenStream {
    let decode_arms = variants.iter().enumerate().map(|(discriminant, variant)| {
        let variant_name = &variant.ident;
        let discriminant_i32 = discriminant as i32;

        match &variant.fields {
            Fields::Unit => {
                quote! {
                    #discriminant_i32 => Ok((Self::#variant_name, 4))
                }
            }
            Fields::Unnamed(fields) => {
                let field_types: Vec<_> = fields.unnamed.iter().map(|f| &f.ty).collect();
                let field_bindings: Vec<_> = (0..fields.unnamed.len()).map(|i| quote::format_ident!("f{}", i)).collect();
                let decode_fields = field_bindings.iter().zip(field_types.iter()).map(|(binding, ty)| {
                    let decode_expr = decode_from_expr(ty, custom_types, quote! { &buf[position..] });
                    quote! {
                        let (#binding, size) = #decode_expr?;
                        position += size;
                    }
                });
                quote! {
                    #discriminant_i32 => {
                        let mut position = 4usize;
                        #(#decode_fields)*
                        Ok((Self::#variant_name(#(#field_bindings),*), position))
                    }
                }
            }
            Fields::Named(fields) => {
                let field_names: Vec<_> = fields.named.iter().filter_map(|f| f.ident.as_ref()).collect();
                let field_types: Vec<_> = fields.named.iter().map(|f| &f.ty).collect();
                let decode_fields = field_names.iter().zip(field_types.iter()).map(|(name, ty)| {
                    let decode_expr = decode_from_expr(ty, custom_types, quote! { &buf[position..] });
                    quote! {
                        let (#name, size) = #decode_expr?;
                        position += size;
                    }
                });
                quote! {
                    #discriminant_i32 => {
                        let mut position = 4usize;
                        #(#decode_fields)*
                        Ok((Self::#variant_name { #(#field_names),* }, position))
                    }
                }
            }
        }
    });

    quote! {
        impl #impl_generics ::boltffi::__private::wire::WireDecode for #enum_name #ty_generics #where_clause {
            fn decode_from(buf: &[u8]) -> ::boltffi::__private::wire::DecodeResult<Self> {
                let disc_bytes: [u8; 4] = buf.get(0..4)
                    .ok_or(::boltffi::__private::wire::DecodeError::BufferTooSmall)?
                    .try_into()
                    .map_err(|_| ::boltffi::__private::wire::DecodeError::BufferTooSmall)?;
                let discriminant = i32::from_le_bytes(disc_bytes);
                match discriminant {
                    #(#decode_arms),*,
                    _ => Err(::boltffi::__private::wire::DecodeError::BufferTooSmall)
                }
            }
        }
    }
}
