use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, ItemEnum, ItemStruct, Type};

use crate::data::analysis::StructDataShape;
use crate::index::custom_types::{CustomTypeRegistry, contains_custom_types};

pub(super) struct StructWireExpansion<'a> {
    item_struct: &'a ItemStruct,
    custom_types: &'a CustomTypeRegistry,
}

pub(super) struct EnumWireExpansion<'a> {
    item_enum: &'a ItemEnum,
    custom_types: &'a CustomTypeRegistry,
}

struct StructWireRenderContext<'a> {
    struct_name: &'a syn::Ident,
    impl_generics: &'a syn::ImplGenerics<'a>,
    ty_generics: &'a syn::TypeGenerics<'a>,
    where_clause: Option<&'a syn::WhereClause>,
    field_names: &'a [&'a syn::Ident],
    field_types: &'a [&'a Type],
    is_blittable: bool,
}

struct WireTypePlan<'a> {
    rust_type: &'a Type,
    custom_types: &'a CustomTypeRegistry,
}

enum WireContainerShape<'a> {
    Vec(&'a Type),
    Option(&'a Type),
    Result { ok: &'a Type, err: &'a Type },
    Other,
}

impl<'a> WireTypePlan<'a> {
    fn new(rust_type: &'a Type, custom_types: &'a CustomTypeRegistry) -> Self {
        Self {
            rust_type,
            custom_types,
        }
    }

    fn wire_type(&self) -> Type {
        if let Some(entry) = self.custom_types.lookup(self.rust_type) {
            return entry.repr_type().unwrap_or_else(|_| self.rust_type.clone());
        }

        match self.container_shape() {
            WireContainerShape::Vec(inner_type) => {
                let inner_wire = Self::new(inner_type, self.custom_types).wire_type();
                syn::parse_quote!(Vec<#inner_wire>)
            }
            WireContainerShape::Option(inner_type) => {
                let inner_wire = Self::new(inner_type, self.custom_types).wire_type();
                syn::parse_quote!(Option<#inner_wire>)
            }
            WireContainerShape::Result { ok, err } => {
                let ok_wire = Self::new(ok, self.custom_types).wire_type();
                let err_wire = Self::new(err, self.custom_types).wire_type();
                syn::parse_quote!(Result<#ok_wire, #err_wire>)
            }
            WireContainerShape::Other => self.rust_type.clone(),
        }
    }

    fn wire_size_expr(&self, value_expr: TokenStream) -> TokenStream {
        if let Some(entry) = self.custom_types.lookup(self.rust_type) {
            let into_fn = entry.to_fn_path();
            return quote! { ::boltffi::__private::wire::WireEncode::wire_size(&#into_fn(#value_expr)) };
        }

        match self.container_shape() {
            WireContainerShape::Vec(inner_type)
                if contains_custom_types(inner_type, self.custom_types) =>
            {
                let inner_size =
                    Self::new(inner_type, self.custom_types).wire_size_expr(quote! { element });
                quote! {
                    ::boltffi::__private::wire::VEC_COUNT_SIZE
                        + (#value_expr)
                            .iter()
                            .map(|element| #inner_size)
                            .sum::<usize>()
                }
            }
            WireContainerShape::Option(inner_type)
                if contains_custom_types(inner_type, self.custom_types) =>
            {
                let inner_size =
                    Self::new(inner_type, self.custom_types).wire_size_expr(quote! { value });
                quote! {
                    match #value_expr {
                        Some(value) => ::boltffi::__private::wire::OPTION_FLAG_SIZE + #inner_size,
                        None => ::boltffi::__private::wire::OPTION_FLAG_SIZE,
                    }
                }
            }
            WireContainerShape::Result { ok, err }
                if contains_custom_types(ok, self.custom_types)
                    || contains_custom_types(err, self.custom_types) =>
            {
                let ok_size = Self::new(ok, self.custom_types).wire_size_expr(quote! { ok_value });
                let err_size =
                    Self::new(err, self.custom_types).wire_size_expr(quote! { err_value });
                quote! {
                    match #value_expr {
                        Ok(ok_value) => ::boltffi::__private::wire::RESULT_TAG_SIZE + #ok_size,
                        Err(err_value) => ::boltffi::__private::wire::RESULT_TAG_SIZE + #err_size,
                    }
                }
            }
            WireContainerShape::Other => {
                quote! { ::boltffi::__private::wire::WireEncode::wire_size(#value_expr) }
            }
            _ => quote! { ::boltffi::__private::wire::WireEncode::wire_size(#value_expr) },
        }
    }

    fn encode_to_expr(&self, value_expr: TokenStream, buf_expr: TokenStream) -> TokenStream {
        if let Some(entry) = self.custom_types.lookup(self.rust_type) {
            let into_fn = entry.to_fn_path();
            return quote! {
                {
                    let __boltffi_custom_value = #into_fn(#value_expr);
                    ::boltffi::__private::wire::WireEncode::encode_to(&__boltffi_custom_value, #buf_expr)
                }
            };
        }

        match self.container_shape() {
            WireContainerShape::Vec(inner_type)
                if contains_custom_types(inner_type, self.custom_types) =>
            {
                let inner_encode = Self::new(inner_type, self.custom_types).encode_to_expr(
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
            }
            WireContainerShape::Option(inner_type)
                if contains_custom_types(inner_type, self.custom_types) =>
            {
                let inner_encode = Self::new(inner_type, self.custom_types).encode_to_expr(
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
            }
            WireContainerShape::Result { ok, err }
                if contains_custom_types(ok, self.custom_types)
                    || contains_custom_types(err, self.custom_types) =>
            {
                let ok_encode = Self::new(ok, self.custom_types).encode_to_expr(
                    quote! { ok_value },
                    quote! { &mut #buf_expr[::boltffi::__private::wire::RESULT_TAG_SIZE..] },
                );
                let err_encode = Self::new(err, self.custom_types).encode_to_expr(
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
            WireContainerShape::Other => {
                quote! { ::boltffi::__private::wire::WireEncode::encode_to(#value_expr, #buf_expr) }
            }
            _ => {
                quote! { ::boltffi::__private::wire::WireEncode::encode_to(#value_expr, #buf_expr) }
            }
        }
    }

    fn decode_from_expr(&self, buf_expr: TokenStream) -> TokenStream {
        if let Some(entry) = self.custom_types.lookup(self.rust_type) {
            let repr_type = entry.repr_type().unwrap_or_else(|_| syn::parse_quote!(()));
            let try_from_fn = entry.try_from_fn_path();
            return quote! {
                {
                    match <#repr_type as ::boltffi::__private::wire::WireDecode>::decode_from(#buf_expr) {
                        Ok((repr_value, used)) => match #try_from_fn(repr_value) {
                            Ok(value) => Ok((value, used)),
                            Err(_) => Err(::boltffi::__private::wire::DecodeError::InvalidValue(
                                ::boltffi::__private::wire::InvalidWireValue::CustomConversion,
                            )),
                        },
                        Err(error) => Err(error),
                    }
                }
            };
        }

        match self.container_shape() {
            WireContainerShape::Vec(inner_type)
                if contains_custom_types(inner_type, self.custom_types) =>
            {
                let inner_decode =
                    Self::new(inner_type, self.custom_types).decode_from_expr(quote! { inner_buf });
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
            }
            WireContainerShape::Option(inner_type)
                if contains_custom_types(inner_type, self.custom_types) =>
            {
                let inner_decode =
                    Self::new(inner_type, self.custom_types).decode_from_expr(quote! { inner_buf });
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
                                _ => Err(::boltffi::__private::wire::DecodeError::InvalidValue(
                                    ::boltffi::__private::wire::InvalidWireValue::OptionTag,
                                )),
                            }
                        }
                    }
                }
            }
            WireContainerShape::Result { ok, err }
                if contains_custom_types(ok, self.custom_types)
                    || contains_custom_types(err, self.custom_types) =>
            {
                let ok_decode =
                    Self::new(ok, self.custom_types).decode_from_expr(quote! { inner_buf });
                let err_decode =
                    Self::new(err, self.custom_types).decode_from_expr(quote! { inner_buf });
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
                                _ => Err(::boltffi::__private::wire::DecodeError::InvalidValue(
                                    ::boltffi::__private::wire::InvalidWireValue::ResultTag,
                                )),
                            }
                        }
                    }
                }
            }
            WireContainerShape::Other => {
                let rust_type = self.rust_type;
                quote! { <#rust_type as ::boltffi::__private::wire::WireDecode>::decode_from(#buf_expr) }
            }
            _ => {
                let rust_type = self.rust_type;
                quote! { <#rust_type as ::boltffi::__private::wire::WireDecode>::decode_from(#buf_expr) }
            }
        }
    }

    fn container_shape(&self) -> WireContainerShape<'a> {
        let Type::Path(type_path) = self.rust_type else {
            return WireContainerShape::Other;
        };
        let Some(segment) = type_path.path.segments.last() else {
            return WireContainerShape::Other;
        };
        let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return WireContainerShape::Other;
        };

        match segment.ident.to_string().as_str() {
            "Vec" => arguments
                .args
                .first()
                .and_then(Self::type_argument)
                .map(WireContainerShape::Vec)
                .unwrap_or(WireContainerShape::Other),
            "Option" => arguments
                .args
                .first()
                .and_then(Self::type_argument)
                .map(WireContainerShape::Option)
                .unwrap_or(WireContainerShape::Other),
            "Result" => match (
                arguments.args.first().and_then(Self::type_argument),
                arguments.args.iter().nth(1).and_then(Self::type_argument),
            ) {
                (Some(ok), Some(err)) => WireContainerShape::Result { ok, err },
                _ => WireContainerShape::Other,
            },
            _ => WireContainerShape::Other,
        }
    }

    fn type_argument(argument: &'a syn::GenericArgument) -> Option<&'a Type> {
        match argument {
            syn::GenericArgument::Type(rust_type) => Some(rust_type),
            _ => None,
        }
    }
}

pub(super) fn generate_wire_impls(
    item_struct: &ItemStruct,
    custom_types: &CustomTypeRegistry,
) -> TokenStream {
    StructWireExpansion::new(item_struct, custom_types).render()
}

pub(super) fn generate_enum_wire_impls(
    item_enum: &ItemEnum,
    custom_types: &CustomTypeRegistry,
) -> TokenStream {
    EnumWireExpansion::new(item_enum, custom_types).render()
}

impl<'a> StructWireExpansion<'a> {
    fn new(item_struct: &'a ItemStruct, custom_types: &'a CustomTypeRegistry) -> Self {
        Self {
            item_struct,
            custom_types,
        }
    }

    fn render(&self) -> TokenStream {
        let struct_name = &self.item_struct.ident;
        let (impl_generics, ty_generics, where_clause) = self.item_struct.generics.split_for_impl();
        let fields = match &self.item_struct.fields {
            Fields::Named(named_fields) => &named_fields.named,
            _ => return quote! {},
        };

        if fields.is_empty() {
            return self.render_empty_struct();
        }

        let field_names = fields
            .iter()
            .filter_map(|field| field.ident.as_ref())
            .collect::<Vec<_>>();
        let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
        let render_context = StructWireRenderContext {
            struct_name,
            impl_generics: &impl_generics,
            ty_generics: &ty_generics,
            where_clause,
            field_names: &field_names,
            field_types: &field_types,
            is_blittable: StructDataShape::new(self.item_struct).is_blittable(),
        };
        let wire_encode_impl = self.render_wire_encode_impl(&render_context);
        let wire_decode_impl = self.render_wire_decode_impl(&render_context);
        let blittable_impl = if render_context.is_blittable {
            quote! {
                unsafe impl #impl_generics ::boltffi::__private::wire::Blittable for #struct_name #ty_generics #where_clause {}
            }
        } else {
            quote! {}
        };

        quote! {
            #wire_encode_impl
            #wire_decode_impl
            #blittable_impl
        }
    }

    fn render_empty_struct(&self) -> TokenStream {
        let struct_name = &self.item_struct.ident;
        quote! {
            impl ::boltffi::__private::wire::WireEncode for #struct_name {
                fn is_fixed_size() -> bool { true }
                fn fixed_size() -> Option<usize> { Some(2) }
                fn wire_size(&self) -> usize { 2 }

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

    fn render_wire_size_impl(&self, render_context: &StructWireRenderContext<'_>) -> TokenStream {
        if render_context.is_blittable {
            return quote! {
                fn is_fixed_size() -> bool { true }
                fn fixed_size() -> Option<usize> { Some(::core::mem::size_of::<Self>()) }
                fn wire_size(&self) -> usize { ::core::mem::size_of::<Self>() }
            };
        }

        let all_fixed_check = render_context.field_types.iter().map(|field_type| {
            if contains_custom_types(field_type, self.custom_types) {
                let wire_type = WireTypePlan::new(field_type, self.custom_types).wire_type();
                quote! { <#wire_type as ::boltffi::__private::wire::WireEncode>::is_fixed_size() }
            } else {
                quote! { <#field_type as ::boltffi::__private::wire::WireEncode>::is_fixed_size() }
            }
        });
        let fixed_size_sum = render_context.field_types.iter().map(|field_type| {
            if contains_custom_types(field_type, self.custom_types) {
                let wire_type = WireTypePlan::new(field_type, self.custom_types).wire_type();
                quote! { <#wire_type as ::boltffi::__private::wire::WireEncode>::fixed_size().unwrap_or(0) }
            } else {
                quote! { <#field_type as ::boltffi::__private::wire::WireEncode>::fixed_size().unwrap_or(0) }
            }
        });
        let field_wire_sizes = render_context
            .field_names
            .iter()
            .zip(render_context.field_types.iter())
            .map(|(field_name, field_type)| {
                WireTypePlan::new(field_type, self.custom_types)
                    .wire_size_expr(quote! { &self.#field_name })
            });
        quote! {
            fn is_fixed_size() -> bool {
                #(#all_fixed_check)&&*
            }

            fn fixed_size() -> Option<usize> {
                if <Self as ::boltffi::__private::wire::WireEncode>::is_fixed_size() {
                    Some(#(#fixed_size_sum)+*)
                } else {
                    None
                }
            }

            fn wire_size(&self) -> usize {
                <Self as ::boltffi::__private::wire::WireEncode>::fixed_size().unwrap_or_else(|| {
                    #(#field_wire_sizes)+*
                })
            }
        }
    }

    fn render_wire_encode_impl(&self, render_context: &StructWireRenderContext<'_>) -> TokenStream {
        if render_context.is_blittable {
            let struct_name = render_context.struct_name;
            let impl_generics = render_context.impl_generics;
            let ty_generics = render_context.ty_generics;
            let where_clause = render_context.where_clause;
            return quote! {
                impl #impl_generics ::boltffi::__private::wire::WireEncode for #struct_name #ty_generics #where_clause {
                    const ENCODING_KIND: ::boltffi::__private::wire::WireEncodingKind =
                        ::boltffi::__private::wire::WireEncodingKind::Blittable;
                    fn is_fixed_size() -> bool { true }
                    fn fixed_size() -> Option<usize> { Some(::core::mem::size_of::<Self>()) }
                    fn wire_size(&self) -> usize { ::core::mem::size_of::<Self>() }

                    fn encode_to(&self, buf: &mut [u8]) -> usize {
                        <Self as ::boltffi::__private::wire::Blittable>::encode_value(self, buf)
                    }
                }
            };
        }

        let encode_fields = render_context
            .field_names
            .iter()
            .zip(render_context.field_types.iter())
            .map(|(field_name, field_type)| {
                let field_buffer =
                    syn::Ident::new(&format!("__boltffi_buf_{}", field_name), field_name.span());
                let encode_expr = WireTypePlan::new(field_type, self.custom_types)
                    .encode_to_expr(quote! { &self.#field_name }, quote! { #field_buffer });
                quote! {
                    let #field_buffer = &mut buf[written..];
                    written += #encode_expr;
                }
            });
        let struct_name = render_context.struct_name;
        let impl_generics = render_context.impl_generics;
        let ty_generics = render_context.ty_generics;
        let where_clause = render_context.where_clause;
        let wire_size_impl = self.render_wire_size_impl(render_context);

        quote! {
            impl #impl_generics ::boltffi::__private::wire::WireEncode for #struct_name #ty_generics #where_clause {
                #wire_size_impl

                fn encode_to(&self, buf: &mut [u8]) -> usize {
                    let mut written = 0usize;
                    #(#encode_fields)*
                    written
                }
            }
        }
    }

    fn render_wire_decode_impl(&self, render_context: &StructWireRenderContext<'_>) -> TokenStream {
        let field_names_for_struct = render_context
            .field_names
            .iter()
            .map(|field_name| quote! { #field_name })
            .collect::<Vec<_>>();

        if render_context.is_blittable {
            let struct_name = render_context.struct_name;
            let impl_generics = render_context.impl_generics;
            let ty_generics = render_context.ty_generics;
            let where_clause = render_context.where_clause;
            return quote! {
                impl #impl_generics ::boltffi::__private::wire::WireDecode for #struct_name #ty_generics #where_clause {
                    fn decode_from(buf: &[u8]) -> ::boltffi::__private::wire::DecodeResult<Self> {
                        let size = ::core::mem::size_of::<Self>();
                        <Self as ::boltffi::__private::wire::Blittable>::decode_value(buf)
                            .map(|value| (value, size))
                            .ok_or(::boltffi::__private::wire::DecodeError::BufferTooSmall)
                    }
                }
            };
        }

        let struct_name_literal = render_context.struct_name.to_string();
        let decode_fields = render_context
            .field_names
            .iter()
            .zip(render_context.field_types.iter())
            .map(|(field_name, field_type)| {
                let decode_expr = WireTypePlan::new(field_type, self.custom_types)
                    .decode_from_expr(quote! { &buf[__boltffi_position..] });
                let field_name_literal = field_name.to_string();
                quote! {
                    let (#field_name, __boltffi_size) = #decode_expr.map_err(|error| {
                        eprintln!("[boltffi] wire decode error in {}.{} at position {} (buf_len={}): {:?}",
                            #struct_name_literal, #field_name_literal, __boltffi_position, buf.len(), error);
                        error
                    })?;
                    __boltffi_position += __boltffi_size;
                }
            });
        let struct_name = render_context.struct_name;
        let impl_generics = render_context.impl_generics;
        let ty_generics = render_context.ty_generics;
        let where_clause = render_context.where_clause;

        quote! {
            impl #impl_generics ::boltffi::__private::wire::WireDecode for #struct_name #ty_generics #where_clause {
                fn decode_from(buf: &[u8]) -> ::boltffi::__private::wire::DecodeResult<Self> {
                    let mut __boltffi_position = 0usize;
                    #(#decode_fields)*
                    Ok((Self { #(#field_names_for_struct),* }, __boltffi_position))
                }
            }
        }
    }
}

impl<'a> EnumWireExpansion<'a> {
    fn new(item_enum: &'a ItemEnum, custom_types: &'a CustomTypeRegistry) -> Self {
        Self {
            item_enum,
            custom_types,
        }
    }

    fn render(&self) -> TokenStream {
        let enum_name = &self.item_enum.ident;
        let (impl_generics, ty_generics, where_clause) = self.item_enum.generics.split_for_impl();
        let variants = self.item_enum.variants.iter().collect::<Vec<_>>();

        if variants.is_empty() {
            return quote! {};
        }

        let wire_encode_impl = self.render_wire_encode_impl(
            enum_name,
            &impl_generics,
            &ty_generics,
            where_clause,
            &variants,
        );
        let wire_decode_impl = self.render_wire_decode_impl(
            enum_name,
            &impl_generics,
            &ty_generics,
            where_clause,
            &variants,
        );

        quote! {
            #wire_encode_impl
            #wire_decode_impl
        }
    }

    fn render_wire_size_impl(&self, variants: &[&syn::Variant]) -> TokenStream {
        let all_unit = variants.iter().all(|variant| variant.fields.is_empty());
        let wire_size_arms =
            variants.iter().map(|variant| {
                let variant_name = &variant.ident;
                match &variant.fields {
                    Fields::Unit => quote! { Self::#variant_name => 4 },
                    Fields::Unnamed(fields) => {
                        let field_types = fields
                            .unnamed
                            .iter()
                            .map(|field| &field.ty)
                            .collect::<Vec<_>>();
                        let field_bindings = (0..fields.unnamed.len())
                            .map(|index| quote::format_ident!("f{}", index))
                            .collect::<Vec<_>>();
                        let field_wire_sizes = field_bindings.iter().zip(field_types.iter()).map(
                            |(binding, field_type)| {
                                WireTypePlan::new(field_type, self.custom_types)
                                    .wire_size_expr(quote! { #binding })
                            },
                        );
                        quote! {
                            Self::#variant_name(#(#field_bindings),*) => {
                                4 + #( #field_wire_sizes )+*
                            }
                        }
                    }
                    Fields::Named(fields) => {
                        let field_names = fields
                            .named
                            .iter()
                            .filter_map(|field| field.ident.as_ref())
                            .collect::<Vec<_>>();
                        let field_types = fields
                            .named
                            .iter()
                            .map(|field| &field.ty)
                            .collect::<Vec<_>>();
                        let field_wire_sizes = field_names.iter().zip(field_types.iter()).map(
                            |(binding, field_type)| {
                                WireTypePlan::new(field_type, self.custom_types)
                                    .wire_size_expr(quote! { #binding })
                            },
                        );
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
                fn is_fixed_size() -> bool { true }
                fn fixed_size() -> Option<usize> { Some(4) }
                fn wire_size(&self) -> usize { 4 }
            }
        } else {
            quote! {
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

    fn render_wire_encode_impl(
        &self,
        enum_name: &syn::Ident,
        impl_generics: &syn::ImplGenerics,
        ty_generics: &syn::TypeGenerics,
        where_clause: Option<&syn::WhereClause>,
        variants: &[&syn::Variant],
    ) -> TokenStream {
        let wire_size_impl = self.render_wire_size_impl(variants);
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
                    let field_types = fields
                        .unnamed
                        .iter()
                        .map(|field| &field.ty)
                        .collect::<Vec<_>>();
                    let field_bindings = (0..fields.unnamed.len())
                        .map(|index| quote::format_ident!("f{}", index))
                        .collect::<Vec<_>>();
                    let encode_fields = field_bindings.iter().zip(field_types.iter()).map(
                        |(binding, field_type)| {
                            let field_buffer = quote::format_ident!("__boltffi_buf_{}", binding);
                            let encode_expr = WireTypePlan::new(field_type, self.custom_types)
                                .encode_to_expr(quote! { #binding }, quote! { #field_buffer });
                            quote! {
                                let #field_buffer = &mut buf[__boltffi_written..];
                                __boltffi_written += #encode_expr;
                            }
                        },
                    );
                    quote! {
                        Self::#variant_name(#(#field_bindings),*) => {
                            buf[0..4].copy_from_slice(&(#discriminant_i32 as i32).to_le_bytes());
                            let mut __boltffi_written = 4usize;
                            #(#encode_fields)*
                            __boltffi_written
                        }
                    }
                }
                Fields::Named(fields) => {
                    let field_names = fields
                        .named
                        .iter()
                        .filter_map(|field| field.ident.as_ref())
                        .collect::<Vec<_>>();
                    let field_types = fields
                        .named
                        .iter()
                        .map(|field| &field.ty)
                        .collect::<Vec<_>>();
                    let encode_fields =
                        field_names
                            .iter()
                            .zip(field_types.iter())
                            .map(|(binding, field_type)| {
                                let field_buffer =
                                    quote::format_ident!("__boltffi_buf_{}", binding);
                                let encode_expr = WireTypePlan::new(field_type, self.custom_types)
                                    .encode_to_expr(quote! { #binding }, quote! { #field_buffer });
                                quote! {
                                    let #field_buffer = &mut buf[__boltffi_written..];
                                    __boltffi_written += #encode_expr;
                                }
                            });
                    quote! {
                        Self::#variant_name { #(#field_names),* } => {
                            buf[0..4].copy_from_slice(&(#discriminant_i32 as i32).to_le_bytes());
                            let mut __boltffi_written = 4usize;
                            #(#encode_fields)*
                            __boltffi_written
                        }
                    }
                }
            }
        });

        quote! {
            impl #impl_generics ::boltffi::__private::wire::WireEncode for #enum_name #ty_generics #where_clause {
                #wire_size_impl

                fn encode_to(&self, buf: &mut [u8]) -> usize {
                    match self {
                        #(#encode_arms),*
                    }
                }
            }
        }
    }

    fn render_wire_decode_impl(
        &self,
        enum_name: &syn::Ident,
        impl_generics: &syn::ImplGenerics,
        ty_generics: &syn::TypeGenerics,
        where_clause: Option<&syn::WhereClause>,
        variants: &[&syn::Variant],
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
                    let field_types = fields
                        .unnamed
                        .iter()
                        .map(|field| &field.ty)
                        .collect::<Vec<_>>();
                    let field_bindings = (0..fields.unnamed.len())
                        .map(|index| quote::format_ident!("f{}", index))
                        .collect::<Vec<_>>();
                    let decode_fields = field_bindings.iter().zip(field_types.iter()).map(
                        |(binding, field_type)| {
                            let decode_expr = WireTypePlan::new(field_type, self.custom_types)
                                .decode_from_expr(quote! { &buf[__boltffi_position..] });
                            quote! {
                                let (#binding, __boltffi_size) = #decode_expr?;
                                __boltffi_position += __boltffi_size;
                            }
                        },
                    );
                    quote! {
                        #discriminant_i32 => {
                            let mut __boltffi_position = 4usize;
                            #(#decode_fields)*
                            Ok((Self::#variant_name(#(#field_bindings),*), __boltffi_position))
                        }
                    }
                }
                Fields::Named(fields) => {
                    let field_names = fields
                        .named
                        .iter()
                        .filter_map(|field| field.ident.as_ref())
                        .collect::<Vec<_>>();
                    let field_types = fields
                        .named
                        .iter()
                        .map(|field| &field.ty)
                        .collect::<Vec<_>>();
                    let decode_fields = field_names.iter().zip(field_types.iter()).map(
                        |(field_name, field_type)| {
                            let decode_expr = WireTypePlan::new(field_type, self.custom_types)
                                .decode_from_expr(quote! { &buf[__boltffi_position..] });
                            quote! {
                                let (#field_name, __boltffi_size) = #decode_expr?;
                                __boltffi_position += __boltffi_size;
                            }
                        },
                    );
                    quote! {
                        #discriminant_i32 => {
                            let mut __boltffi_position = 4usize;
                            #(#decode_fields)*
                            Ok((Self::#variant_name { #(#field_names),* }, __boltffi_position))
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
                        _ => Err(::boltffi::__private::wire::DecodeError::InvalidValue(
                            ::boltffi::__private::wire::InvalidWireValue::EnumTag
                        ))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    fn render_struct(item: &ItemStruct) -> String {
        let custom_types = CustomTypeRegistry::default();
        generate_wire_impls(item, &custom_types).to_string()
    }

    fn render_enum(item: &ItemEnum) -> String {
        let custom_types = CustomTypeRegistry::default();
        generate_enum_wire_impls(item, &custom_types).to_string()
    }

    #[test]
    fn struct_with_size_field_uses_prefixed_decode_bindings() {
        let item: ItemStruct = parse_quote! {
            pub struct Buffer {
                pub name: String,
                pub size: u32,
            }
        };
        let generated = render_struct(&item);

        assert!(
            !generated.contains("let (size , size)"),
            "duplicate `size` binding still present: {generated}"
        );
        assert!(
            generated.contains("__boltffi_size"),
            "expected __boltffi_size in decode, got: {generated}"
        );
        assert!(
            generated.contains("__boltffi_position"),
            "expected __boltffi_position in decode, got: {generated}"
        );
    }

    #[test]
    fn struct_with_position_field_uses_prefixed_decode_bindings() {
        let item: ItemStruct = parse_quote! {
            pub struct Cursor {
                pub position: u32,
                pub data: String,
            }
        };
        let generated = render_struct(&item);

        assert!(
            generated.contains("let (position , __boltffi_size)"),
            "expected field `position` bound alongside __boltffi_size, got: {generated}"
        );
        assert!(
            generated.contains("__boltffi_position += __boltffi_size"),
            "expected __boltffi_position increment, got: {generated}"
        );
    }

    #[test]
    fn enum_named_variant_with_size_field_uses_prefixed_decode_bindings() {
        let item: ItemEnum = parse_quote! {
            pub enum Message {
                Data { size: u32, payload: String },
                Empty,
            }
        };
        let generated = render_enum(&item);

        assert!(
            !generated.contains("let (size , size)"),
            "duplicate `size` binding still present: {generated}"
        );
        assert!(
            generated.contains("let (size , __boltffi_size)"),
            "expected field `size` bound alongside __boltffi_size, got: {generated}"
        );
        assert!(
            generated.contains("__boltffi_position"),
            "expected __boltffi_position in enum decode, got: {generated}"
        );
    }

    #[test]
    fn enum_named_variant_with_written_field_uses_prefixed_encode_bindings() {
        let item: ItemEnum = parse_quote! {
            pub enum Log {
                Entry { written: u64, text: String },
                None,
            }
        };
        let generated = render_enum(&item);

        assert!(
            generated.contains("__boltffi_written"),
            "expected __boltffi_written in enum encode, got: {generated}"
        );
        assert!(
            generated.contains("let mut __boltffi_written = 4usize"),
            "expected prefixed written counter, got: {generated}"
        );
    }

    #[test]
    fn enum_unnamed_variant_decode_uses_prefixed_bindings() {
        let item: ItemEnum = parse_quote! {
            pub enum Shape {
                Rect(u32, u32),
                Circle(u32),
            }
        };
        let generated = render_enum(&item);

        assert!(
            generated.contains("__boltffi_size"),
            "expected __boltffi_size in enum unnamed decode, got: {generated}"
        );
        assert!(
            generated.contains("__boltffi_position"),
            "expected __boltffi_position in enum unnamed decode, got: {generated}"
        );
    }

    #[test]
    fn struct_encode_with_size_field_accesses_via_self() {
        let item: ItemStruct = parse_quote! {
            pub struct Buffer {
                pub name: String,
                pub size: u32,
            }
        };
        let generated = render_struct(&item);

        assert!(
            generated.contains("& self . size"),
            "expected self.size access in encode, got: {generated}"
        );
    }
}
