use boltffi_binding::{Native, Primitive, Wasm32};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Type;

use crate::experimental::{
    error::Error,
    wrapper::{Render, names, returns::Tokens, scalar_option::WasmScalar},
};

pub struct Renderer;
pub struct Failure;
pub struct FailureInput {
    primitive: Primitive,
}
pub struct Empty {
    primitive: Primitive,
}
pub struct Incoming;

pub struct Input {
    primitive: Primitive,
    value: syn::Ident,
}

pub struct IncomingInput {
    primitive: Primitive,
    rust_type: Type,
    value: TokenStream,
}

impl Input {
    pub fn new(primitive: Primitive, value: syn::Ident) -> Self {
        Self { primitive, value }
    }
}

impl FailureInput {
    pub fn new(primitive: Primitive) -> Self {
        Self { primitive }
    }
}

impl Empty {
    pub fn new(primitive: Primitive) -> Self {
        Self { primitive }
    }
}

impl IncomingInput {
    pub fn new(primitive: Primitive, rust_type: Type, value: TokenStream) -> Self {
        Self {
            primitive,
            rust_type,
            value,
        }
    }
}

impl Render<Native, Input> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input) -> Result<Self::Output, Error> {
        let value = input.value;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: Vec::new(),
            return_type: quote! { -> ::boltffi::__private::FfiBuf },
            body: quote! {
                ::boltffi::__private::FfiBuf::wire_encode(&#value)
            },
        })
    }
}

impl Render<Wasm32, Input> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input) -> Result<Self::Output, Error> {
        let value = input.value;
        let present = names::Locals::new(value.span()).value();
        if matches!(input.primitive, Primitive::F64) {
            return Ok(Tokens {
                items: Vec::new(),
                ffi_parameters: Vec::new(),
                return_type: quote! { -> f64 },
                body: quote! {
                    match #value {
                        Some(#present) => {
                            if #present.is_nan() {
                                ::boltffi::__private::write_option_f64_presence(true);
                            }
                            #present
                        }
                        None => {
                            ::boltffi::__private::write_option_f64_presence(false);
                            f64::NAN
                        }
                    }
                },
            });
        }
        let scalar = WasmScalar::new(input.primitive, present.clone());
        let return_type = scalar.carrier_type();
        let none = scalar.none();
        let some = scalar.outgoing()?;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: Vec::new(),
            return_type: quote! { -> #return_type },
            body: quote! {
                match #value {
                    Some(#present) => #some,
                    None => #none,
                }
            },
        })
    }
}

impl Render<Native, FailureInput> for Failure {
    type Output = TokenStream;

    fn render(self, input: FailureInput) -> Result<Self::Output, Error> {
        let empty =
            <Renderer as Render<Native, Empty>>::render(Renderer, Empty::new(input.primitive))?;
        let body = empty.body();
        Ok(quote! {
            return #body;
        })
    }
}

impl Render<Wasm32, FailureInput> for Failure {
    type Output = TokenStream;

    fn render(self, input: FailureInput) -> Result<Self::Output, Error> {
        let empty =
            <Renderer as Render<Wasm32, Empty>>::render(Renderer, Empty::new(input.primitive))?;
        let body = empty.body();
        Ok(quote! {
            return #body;
        })
    }
}

impl Render<Native, Empty> for Renderer {
    type Output = Tokens;

    fn render(self, _input: Empty) -> Result<Self::Output, Error> {
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: Vec::new(),
            return_type: quote! { -> ::boltffi::__private::FfiBuf },
            body: quote! { ::boltffi::__private::FfiBuf::default() },
        })
    }
}

impl Render<Wasm32, Empty> for Renderer {
    type Output = Tokens;

    fn render(self, input: Empty) -> Result<Self::Output, Error> {
        if matches!(input.primitive, Primitive::F64) {
            return Ok(Tokens {
                items: Vec::new(),
                ffi_parameters: Vec::new(),
                return_type: quote! { -> f64 },
                body: quote! {
                    {
                        ::boltffi::__private::write_option_f64_presence(false);
                        f64::NAN
                    }
                },
            });
        }
        let scalar = WasmScalar::new(
            input.primitive,
            names::Locals::new(Span::call_site()).value(),
        );
        let return_type = scalar.carrier_type();
        let none = scalar.none();
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: Vec::new(),
            return_type: quote! { -> #return_type },
            body: none,
        })
    }
}

impl Render<Native, IncomingInput> for Incoming {
    type Output = TokenStream;

    fn render(self, input: IncomingInput) -> Result<Self::Output, Error> {
        let rust_type = input.rust_type;
        let value = input.value;
        Ok(quote! {
            {
                let __boltffi_result = #value;
                match ::boltffi::__private::wire::decode::<#rust_type>(unsafe {
                    __boltffi_result.as_byte_slice()
                }) {
                    Ok(__boltffi_value) => __boltffi_value,
                    Err(error) => {
                        panic!("callback method optional scalar return conversion failed: {:?}", error)
                    }
                }
            }
        })
    }
}

impl Render<Wasm32, IncomingInput> for Incoming {
    type Output = TokenStream;

    fn render(self, input: IncomingInput) -> Result<Self::Output, Error> {
        let value = input.value;
        let result = names::Locals::new(Span::call_site()).result();
        let scalar = WasmScalar::new(input.primitive, result.clone());
        let is_none = scalar.is_none();
        let some = scalar.incoming()?;
        Ok(quote! {
            {
                let #result = #value;
                if #is_none {
                    None
                } else {
                    Some(#some)
                }
            }
        })
    }
}
