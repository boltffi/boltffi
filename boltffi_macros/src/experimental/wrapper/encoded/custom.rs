use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CodecNode, CodecRead, CustomConverterPath,
    CustomConverterPathRoot, CustomTypeConverter, CustomTypeId, ElementCount, EnumId, MapKind, Op,
    Primitive, RecordId,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Expr, ExprPath, Index, parse_str};

use crate::experimental::{error::Error, expansion::Expansion, surface::RenderSurface, wrapper};

pub struct Incoming<'expansion, 'lowered, S: RenderSurface> {
    codec: &'lowered CodecNode,
    expansion: &'expansion Expansion<'lowered, S>,
}

pub struct Outgoing<'expansion, 'lowered, S: RenderSurface> {
    codec: &'lowered CodecNode,
    expansion: &'expansion Expansion<'lowered, S>,
}

pub struct BorrowedOutgoing<'expansion, 'lowered, S: RenderSurface> {
    codec: &'lowered CodecNode,
    expansion: &'expansion Expansion<'lowered, S>,
}

pub struct IncomingConversion {
    tokens: TokenStream,
    fallible: bool,
    changed: bool,
}

struct IncomingConverter<'expansion, S: RenderSurface> {
    expansion: &'expansion Expansion<'expansion, S>,
}

struct OutgoingConverter<'expansion, S: RenderSurface> {
    expansion: &'expansion Expansion<'expansion, S>,
    mode: OutgoingValueMode,
}

struct IncomingTransform {
    tokens: TokenStream,
    representation: TokenStream,
    fallible: bool,
    changed: bool,
}

enum OutgoingTransform {
    Identity,
    ReferenceConverter(TokenStream),
    ValueConverter(TokenStream),
    Optional(Box<OutgoingTransform>),
    Sequence(Box<OutgoingTransform>),
    Tuple(Vec<OutgoingTransform>),
    Result {
        ok: Box<OutgoingTransform>,
        err: Box<OutgoingTransform>,
    },
    Map {
        key: Box<OutgoingTransform>,
        value: Box<OutgoingTransform>,
    },
    CustomRepresentation {
        custom: Box<OutgoingTransform>,
        representation: Box<OutgoingTransform>,
    },
}

struct ConverterRenderer;

struct PathRenderer;

#[derive(Clone, Copy)]
enum OutgoingValueMode {
    Owned,
    Borrowed,
}

impl IncomingConversion {
    pub fn tokens(&self) -> &TokenStream {
        &self.tokens
    }

    pub const fn fallible(&self) -> bool {
        self.fallible
    }

    pub const fn changed(&self) -> bool {
        self.changed
    }

    fn value_or_return_error(&self) -> TokenStream {
        let tokens = self.tokens();
        match self.fallible {
            true => quote! {
                match #tokens {
                    Ok(value) => value,
                    Err(error) => return Err(error),
                }
            },
            false => quote! { #tokens },
        }
    }
}

impl IncomingTransform {
    fn identity(representation: TokenStream) -> Self {
        Self {
            tokens: TokenStream::new(),
            representation,
            fallible: false,
            changed: false,
        }
    }

    fn new(
        tokens: TokenStream,
        representation: TokenStream,
        fallible: bool,
        changed: bool,
    ) -> Self {
        Self {
            tokens,
            representation,
            fallible,
            changed,
        }
    }

    fn apply(&self, value: TokenStream) -> IncomingConversion {
        self.apply_with_type(value, None)
    }

    fn apply_typed(&self, value: TokenStream, input_type: TokenStream) -> IncomingConversion {
        self.apply_with_type(value, Some(input_type))
    }

    fn apply_with_type(
        &self,
        value: TokenStream,
        input_type: Option<TokenStream>,
    ) -> IncomingConversion {
        match self.changed {
            true => IncomingConversion {
                tokens: {
                    let tokens = &self.tokens;
                    match input_type {
                        Some(input_type) => quote! {
                            (|__boltffi_value: #input_type| (#tokens)(__boltffi_value))(#value)
                        },
                        None => quote! { (#tokens)(#value) },
                    }
                },
                fallible: self.fallible,
                changed: true,
            },
            false => IncomingConversion {
                tokens: value,
                fallible: false,
                changed: false,
            },
        }
    }

    fn value_or_return_error(&self, value: TokenStream) -> TokenStream {
        self.apply(value).value_or_return_error()
    }

    fn representation(&self) -> TokenStream {
        self.representation.clone()
    }
}

impl OutgoingTransform {
    fn changed(&self) -> bool {
        !matches!(self, Self::Identity)
    }

    fn apply(&self, value: TokenStream, mode: OutgoingValueMode) -> TokenStream {
        match self {
            Self::Identity => value,
            Self::ReferenceConverter(converter) => quote! { (#converter)(&#value) },
            Self::ValueConverter(converter) => quote! { (#converter)(#value) },
            Self::Optional(inner) => {
                let inner = inner.apply(quote! { value }, mode);
                mode.optional(value, inner)
            }
            Self::Sequence(element) => {
                let element = element.apply(quote! { value }, mode);
                mode.sequence(value, element)
            }
            Self::Tuple(elements) => {
                let values = elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| {
                        let field = mode.tuple_field(value.clone(), Index::from(index));
                        element.apply(field, mode)
                    })
                    .collect::<Vec<_>>();
                quote! { (#(#values,)*) }
            }
            Self::Result { ok, err } => {
                let ok = ok.apply(quote! { value }, mode);
                let err = err.apply(quote! { error }, mode);
                quote! {
                    match #value {
                        Ok(value) => Ok(#ok),
                        Err(error) => Err(#err),
                    }
                }
            }
            Self::Map { key, value: item } => {
                let key = key.apply(quote! { key }, mode);
                let item = item.apply(quote! { value }, mode);
                mode.map(value, key, item)
            }
            Self::CustomRepresentation {
                custom,
                representation,
            } => {
                let custom = custom.apply(value, mode);
                let representation =
                    representation.apply(quote! { __boltffi_representation }, mode);
                quote! {
                    {
                        let __boltffi_representation = #custom;
                        #representation
                    }
                }
            }
        }
    }
}

impl<'expansion, S: RenderSurface> IncomingConverter<'expansion, S> {
    fn new(expansion: &'expansion Expansion<'expansion, S>) -> Self {
        Self { expansion }
    }
}

impl<'expansion, S: RenderSurface> OutgoingConverter<'expansion, S> {
    fn new(expansion: &'expansion Expansion<'expansion, S>, mode: OutgoingValueMode) -> Self {
        Self { expansion, mode }
    }
}

impl OutgoingValueMode {
    fn custom(self, converter: TokenStream) -> OutgoingTransform {
        match self {
            Self::Owned => OutgoingTransform::ReferenceConverter(converter),
            Self::Borrowed => OutgoingTransform::ValueConverter(converter),
        }
    }

    fn optional(self, value: TokenStream, inner: TokenStream) -> TokenStream {
        match self {
            Self::Owned => quote! { #value.map(|value| #inner) },
            Self::Borrowed => quote! { #value.as_ref().map(|value| #inner) },
        }
    }

    fn sequence(self, value: TokenStream, element: TokenStream) -> TokenStream {
        match self {
            Self::Owned => quote! {
                #value
                    .into_iter()
                    .map(|value| #element)
                    .collect::<Vec<_>>()
            },
            Self::Borrowed => quote! {
                #value
                    .iter()
                    .map(|value| #element)
                    .collect::<Vec<_>>()
            },
        }
    }

    fn tuple_field(self, value: TokenStream, index: Index) -> TokenStream {
        match self {
            Self::Owned => quote! { (#value).#index },
            Self::Borrowed => quote! { &(#value).#index },
        }
    }

    fn map(self, value: TokenStream, key: TokenStream, item: TokenStream) -> TokenStream {
        match self {
            Self::Owned => quote! {
                #value
                    .into_iter()
                    .map(|(key, value)| (#key, #item))
                    .collect::<Vec<_>>()
            },
            Self::Borrowed => quote! {
                #value
                    .iter()
                    .map(|(key, value)| (#key, #item))
                    .collect::<Vec<_>>()
            },
        }
    }
}

impl<'expansion, 'lowered, S: RenderSurface> Incoming<'expansion, 'lowered, S> {
    pub const fn new(
        codec: &'lowered CodecNode,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self { codec, expansion }
    }

    pub fn convert(&self, value: TokenStream) -> Result<IncomingConversion, Error> {
        if !self.codec.contains_custom() {
            return Ok(IncomingTransform::identity(quote! { _ }).apply(value));
        }

        let transform = self
            .codec
            .render_read_with(&mut IncomingConverter::new(self.expansion))?;
        match transform.changed {
            true => Ok(transform.apply_typed(value, transform.representation())),
            false => Ok(transform.apply(value)),
        }
    }

    pub fn decoded_type(&self) -> Result<Option<TokenStream>, Error> {
        match self.codec.contains_custom() {
            true => self
                .codec
                .render_read_with(&mut IncomingConverter::new(self.expansion))
                .map(|transform| transform.changed.then(|| transform.representation())),
            false => Ok(None),
        }
    }
}

impl<'expansion, S: RenderSurface> CodecRead for IncomingConverter<'expansion, S> {
    type Expr = Result<IncomingTransform, Error>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Expr {
        Ok(IncomingTransform::identity(
            wrapper::type_ref::Renderer.primitive(primitive)?,
        ))
    }

    fn string(&mut self) -> Self::Expr {
        Ok(IncomingTransform::identity(quote! { String }))
    }

    fn interned_string(&mut self, _static_values: &[String]) -> Self::Expr {
        // The pool type belongs to the source signature, not the codec IR.
        // `_` preserves the decoded InternedString<P> type while allowing a
        // sibling custom conversion to provide the enclosing representation.
        Ok(IncomingTransform::identity(quote! { _ }))
    }

    fn bytes(&mut self) -> Self::Expr {
        Ok(IncomingTransform::identity(quote! { Vec<u8> }))
    }

    fn direct_record(&mut self, _: RecordId) -> Self::Expr {
        Ok(IncomingTransform::identity(quote! { _ }))
    }

    fn encoded_record(&mut self, _: RecordId) -> Self::Expr {
        Ok(IncomingTransform::identity(quote! { _ }))
    }

    fn c_style_enum(&mut self, _: EnumId) -> Self::Expr {
        Ok(IncomingTransform::identity(quote! { _ }))
    }

    fn data_enum(&mut self, _: EnumId) -> Self::Expr {
        Ok(IncomingTransform::identity(quote! { _ }))
    }

    fn class_handle(&mut self, _: ClassId) -> Self::Expr {
        Err(Error::UnsupportedExpansion(
            "class handle representation type",
        ))
    }

    fn callback_handle(&mut self, _: CallbackId) -> Self::Expr {
        Err(Error::UnsupportedExpansion(
            "callback handle representation type",
        ))
    }

    fn custom(&mut self, id: CustomTypeId, representation: Self::Expr) -> Self::Expr {
        let representation = representation?;
        let custom = self.expansion.custom_type(id)?;
        let converter = ConverterRenderer.render(custom.converters().try_from_ffi())?;
        let representation_type = representation.representation();
        if !representation.changed {
            return Ok(IncomingTransform::new(
                converter,
                representation_type,
                true,
                true,
            ));
        }
        let representation_value = representation.value_or_return_error(quote! { __boltffi_value });
        Ok(IncomingTransform::new(
            quote! {
                |__boltffi_value: #representation_type| {
                    let __boltffi_representation = #representation_value;
                    (#converter)(__boltffi_representation)
                }
            },
            representation_type,
            true,
            true,
        ))
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Expr {
        Ok(IncomingTransform::identity(
            wrapper::type_ref::Renderer.builtin(kind)?,
        ))
    }

    fn optional(&mut self, inner: Self::Expr) -> Self::Expr {
        let inner = inner?;
        let inner_type = inner.representation();
        let representation = quote! { Option<#inner_type> };
        match inner.changed {
            true => {
                let inner = inner.apply(quote! { value });
                let inner_tokens = inner.tokens();
                Ok(match inner.fallible {
                    true => IncomingTransform::new(
                        quote! {
                            |__boltffi_value: #representation| {
                                match __boltffi_value {
                                    Some(value) => #inner_tokens.map(Some),
                                    None => Ok(None),
                                }
                            }
                        },
                        representation,
                        true,
                        true,
                    ),
                    false => IncomingTransform::new(
                        quote! {
                            |__boltffi_value: #representation| {
                                __boltffi_value.map(|value| #inner_tokens)
                            }
                        },
                        representation,
                        false,
                        true,
                    ),
                })
            }
            false => Ok(IncomingTransform::identity(representation)),
        }
    }

    fn sequence(&mut self, _: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
        let element = element?;
        let element_type = element.representation();
        let representation = quote! { Vec<#element_type> };
        match element.changed {
            true => {
                let element = element.apply(quote! { value });
                let element_tokens = element.tokens();
                Ok(match element.fallible {
                    true => IncomingTransform::new(
                        quote! {
                            |__boltffi_value: #representation| {
                                __boltffi_value
                                    .into_iter()
                                    .map(|value| #element_tokens)
                                    .collect::<Result<Vec<_>, _>>()
                            }
                        },
                        representation,
                        true,
                        true,
                    ),
                    false => IncomingTransform::new(
                        quote! {
                            |__boltffi_value: #representation| {
                                __boltffi_value
                                    .into_iter()
                                    .map(|value| #element_tokens)
                                    .collect::<Vec<_>>()
                            }
                        },
                        representation,
                        false,
                        true,
                    ),
                })
            }
            false => Ok(IncomingTransform::identity(representation)),
        }
    }

    fn tuple(&mut self, elements: Vec<Self::Expr>) -> Self::Expr {
        let elements = elements.into_iter().collect::<Result<Vec<_>, _>>()?;
        let representation_elements = elements
            .iter()
            .map(IncomingTransform::representation)
            .collect::<Vec<_>>();
        let representation = quote! { (#(#representation_elements,)*) };
        let changed = elements.iter().any(|element| element.changed);
        let fallible = elements.iter().any(|element| element.fallible);
        match changed {
            true => {
                let values = elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| {
                        let index = Index::from(index);
                        element.value_or_return_error(quote! { (__boltffi_value).#index })
                    })
                    .collect::<Vec<_>>();
                Ok(match fallible {
                    true => IncomingTransform::new(
                        quote! {
                            |__boltffi_value: #representation| {
                                Ok((#(#values,)*))
                            }
                        },
                        representation,
                        true,
                        true,
                    ),
                    false => IncomingTransform::new(
                        quote! {
                            |__boltffi_value: #representation| {
                                (#(#values,)*)
                            }
                        },
                        representation,
                        false,
                        true,
                    ),
                })
            }
            false => Ok(IncomingTransform::identity(representation)),
        }
    }

    fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr {
        let ok = ok?;
        let err = err?;
        let ok_type = ok.representation();
        let err_type = err.representation();
        let representation = quote! { Result<#ok_type, #err_type> };
        let changed = ok.changed || err.changed;
        let fallible = ok.fallible || err.fallible;
        match changed {
            true => {
                let ok = ok.apply(quote! { value });
                let err = err.apply(quote! { error });
                let ok_tokens = ok.tokens();
                let err_tokens = err.tokens();
                let ok_arm = match ok.fallible {
                    true => quote! { #ok_tokens.map(Ok) },
                    false => quote! { Ok(#ok_tokens) },
                };
                let err_arm = match err.fallible {
                    true => quote! { #err_tokens.map(Err) },
                    false => quote! { Err(#err_tokens) },
                };
                let tokens = match fallible {
                    true => {
                        let ok_arm = match ok.fallible {
                            true => ok_arm,
                            false => quote! { Ok(#ok_arm) },
                        };
                        let err_arm = match err.fallible {
                            true => err_arm,
                            false => quote! { Ok(#err_arm) },
                        };
                        quote! {
                            |__boltffi_value: #representation| {
                                match __boltffi_value {
                                    Ok(value) => #ok_arm,
                                    Err(error) => #err_arm,
                                }
                            }
                        }
                    }
                    false => quote! {
                        |__boltffi_value: #representation| {
                            match __boltffi_value {
                                Ok(value) => #ok_arm,
                                Err(error) => #err_arm,
                            }
                        }
                    },
                };
                Ok(IncomingTransform::new(
                    tokens,
                    representation,
                    fallible,
                    true,
                ))
            }
            false => Ok(IncomingTransform::identity(representation)),
        }
    }

    fn map(&mut self, kind: MapKind, key: Self::Expr, value: Self::Expr) -> Self::Expr {
        let key = key?;
        let value = value?;
        let key_type = key.representation();
        let value_type = value.representation();
        let representation = match kind {
            MapKind::Hash => quote! { ::std::collections::HashMap<#key_type, #value_type> },
            MapKind::BTree => quote! { ::std::collections::BTreeMap<#key_type, #value_type> },
        };
        let changed = key.changed || value.changed;
        let fallible = key.fallible || value.fallible;
        match changed {
            true => {
                let key = key.value_or_return_error(quote! { key });
                let value = value.value_or_return_error(quote! { value });
                Ok(match fallible {
                    true => IncomingTransform::new(
                        quote! {
                            |__boltffi_value: #representation| {
                                __boltffi_value
                                    .into_iter()
                                    .map(|(key, value)| {
                                        Ok((#key, #value))
                                    })
                                    .collect::<Result<_, _>>()
                            }
                        },
                        representation,
                        true,
                        true,
                    ),
                    false => IncomingTransform::new(
                        quote! {
                            |__boltffi_value: #representation| {
                                __boltffi_value
                                    .into_iter()
                                    .map(|(key, value)| (#key, #value))
                                    .collect::<_>()
                            }
                        },
                        representation,
                        false,
                        true,
                    ),
                })
            }
            false => Ok(IncomingTransform::identity(representation)),
        }
    }
}

impl<'expansion, S: RenderSurface> CodecRead for OutgoingConverter<'expansion, S> {
    type Expr = Result<OutgoingTransform, Error>;

    fn primitive(&mut self, _: Primitive) -> Self::Expr {
        Ok(OutgoingTransform::Identity)
    }

    fn string(&mut self) -> Self::Expr {
        Ok(OutgoingTransform::Identity)
    }

    fn interned_string(&mut self, _static_values: &[String]) -> Self::Expr {
        Ok(OutgoingTransform::Identity)
    }

    fn bytes(&mut self) -> Self::Expr {
        Ok(OutgoingTransform::Identity)
    }

    fn direct_record(&mut self, _: RecordId) -> Self::Expr {
        Ok(OutgoingTransform::Identity)
    }

    fn encoded_record(&mut self, _: RecordId) -> Self::Expr {
        Ok(OutgoingTransform::Identity)
    }

    fn c_style_enum(&mut self, _: EnumId) -> Self::Expr {
        Ok(OutgoingTransform::Identity)
    }

    fn data_enum(&mut self, _: EnumId) -> Self::Expr {
        Ok(OutgoingTransform::Identity)
    }

    fn class_handle(&mut self, _: ClassId) -> Self::Expr {
        Ok(OutgoingTransform::Identity)
    }

    fn callback_handle(&mut self, _: CallbackId) -> Self::Expr {
        Ok(OutgoingTransform::Identity)
    }

    fn custom(&mut self, id: CustomTypeId, representation: Self::Expr) -> Self::Expr {
        let representation = representation?;
        let custom = self.expansion.custom_type(id)?;
        let converter = ConverterRenderer.render(custom.converters().into_ffi())?;
        let custom = self.mode.custom(converter);
        match representation.changed() {
            true => Ok(OutgoingTransform::CustomRepresentation {
                custom: Box::new(custom),
                representation: Box::new(representation),
            }),
            false => Ok(custom),
        }
    }

    fn builtin(&mut self, _: BuiltinType) -> Self::Expr {
        Ok(OutgoingTransform::Identity)
    }

    fn optional(&mut self, inner: Self::Expr) -> Self::Expr {
        let inner = inner?;
        match inner.changed() {
            true => Ok(OutgoingTransform::Optional(Box::new(inner))),
            false => Ok(OutgoingTransform::Identity),
        }
    }

    fn sequence(&mut self, _: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
        let element = element?;
        match element.changed() {
            true => Ok(OutgoingTransform::Sequence(Box::new(element))),
            false => Ok(OutgoingTransform::Identity),
        }
    }

    fn tuple(&mut self, elements: Vec<Self::Expr>) -> Self::Expr {
        let elements = elements.into_iter().collect::<Result<Vec<_>, _>>()?;
        match elements.iter().any(OutgoingTransform::changed) {
            true => Ok(OutgoingTransform::Tuple(elements)),
            false => Ok(OutgoingTransform::Identity),
        }
    }

    fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr {
        let ok = ok?;
        let err = err?;
        match ok.changed() || err.changed() {
            true => Ok(OutgoingTransform::Result {
                ok: Box::new(ok),
                err: Box::new(err),
            }),
            false => Ok(OutgoingTransform::Identity),
        }
    }

    fn map(&mut self, _: MapKind, key: Self::Expr, value: Self::Expr) -> Self::Expr {
        let key = key?;
        let value = value?;
        match key.changed() || value.changed() {
            true => Ok(OutgoingTransform::Map {
                key: Box::new(key),
                value: Box::new(value),
            }),
            false => Ok(OutgoingTransform::Identity),
        }
    }
}

impl<'expansion, 'lowered, S: RenderSurface> Outgoing<'expansion, 'lowered, S> {
    pub const fn new(
        codec: &'lowered CodecNode,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self { codec, expansion }
    }

    pub fn has_custom_conversion(&self) -> bool {
        self.codec.contains_custom()
    }

    pub fn convert(&self, value: TokenStream) -> Result<TokenStream, Error> {
        let transform = self.codec.render_read_with(&mut OutgoingConverter::new(
            self.expansion,
            OutgoingValueMode::Owned,
        ))?;
        Ok(transform.apply(value, OutgoingValueMode::Owned))
    }
}

impl<'expansion, 'lowered, S: RenderSurface> BorrowedOutgoing<'expansion, 'lowered, S> {
    pub const fn new(
        codec: &'lowered CodecNode,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self { codec, expansion }
    }

    pub fn has_custom_conversion(&self) -> bool {
        self.codec.contains_custom()
    }

    pub fn convert(&self, value: TokenStream) -> Result<TokenStream, Error> {
        let transform = self.codec.render_read_with(&mut OutgoingConverter::new(
            self.expansion,
            OutgoingValueMode::Borrowed,
        ))?;
        Ok(transform.apply(value, OutgoingValueMode::Borrowed))
    }
}

impl ConverterRenderer {
    fn render(self, converter: &CustomTypeConverter) -> Result<TokenStream, Error> {
        match converter {
            CustomTypeConverter::Path(path) => PathRenderer.render(path),
            CustomTypeConverter::TraitMethod(converter) => {
                let receiver = PathRenderer.render_type(converter.receiver())?;
                let method =
                    parse_str::<syn::Ident>(converter.method().as_str()).map_err(|_| {
                        Error::SourceSyntaxMismatch("custom converter method is not Rust syntax")
                    })?;
                Ok(quote! {
                    <#receiver as ::boltffi::CustomFfiConvertible>::#method
                })
            }
            CustomTypeConverter::Expression(expression) => parse_str::<Expr>(expression.source())
                .map(|expression| quote! { #expression })
                .map_err(|_| Error::SourceSyntaxMismatch("custom converter is not Rust syntax")),
            _ => Err(Error::UnsupportedExpansion("unknown custom converter")),
        }
    }
}

impl PathRenderer {
    fn render(self, path: &CustomConverterPath) -> Result<TokenStream, Error> {
        parse_str::<ExprPath>(&self.source(path)?)
            .map(|path| quote! { #path })
            .map_err(|_| Error::SourceSyntaxMismatch("custom converter path is not Rust syntax"))
    }

    fn render_type(self, path: &CustomConverterPath) -> Result<syn::Type, Error> {
        parse_str::<syn::Type>(&self.source(path)?).map_err(|_| {
            Error::SourceSyntaxMismatch("custom converter receiver is not Rust syntax")
        })
    }

    fn source(&self, path: &CustomConverterPath) -> Result<String, Error> {
        Ok(self.prefix(path)? + &self.segments(path))
    }

    fn prefix(&self, path: &CustomConverterPath) -> Result<String, Error> {
        Ok(match path.root() {
            CustomConverterPathRoot::Relative => String::new(),
            CustomConverterPathRoot::Crate => "crate::".to_owned(),
            CustomConverterPathRoot::Self_ => "self::".to_owned(),
            CustomConverterPathRoot::Super(levels) => {
                std::iter::repeat_n("super", levels.get())
                    .collect::<Vec<_>>()
                    .join("::")
                    + "::"
            }
            CustomConverterPathRoot::Absolute => "::".to_owned(),
            _ => {
                return Err(Error::UnsupportedExpansion(
                    "unknown custom converter path root",
                ));
            }
        })
    }

    fn segments(&self, path: &CustomConverterPath) -> String {
        path.segments()
            .iter()
            .map(|segment| segment.as_str())
            .collect::<Vec<_>>()
            .join("::")
    }
}
