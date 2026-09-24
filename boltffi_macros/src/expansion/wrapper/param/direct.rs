use boltffi_binding::{DirectValueType, Primitive, Receive};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use crate::expansion::{
    error::Error,
    wrapper::{self, names},
};

use super::Tokens;

pub struct Input {
    ty: DirectValueType,
    receive: Receive,
    rust_type: Type,
    ident: Ident,
    failure: TokenStream,
}

impl Input {
    pub fn new(
        ty: &DirectValueType,
        receive: Receive,
        rust_type: Type,
        ident: Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            ty: ty.clone(),
            receive,
            rust_type,
            ident,
            failure,
        }
    }

    pub fn native(self) -> Result<Tokens, Error> {
        match &self.ty {
            DirectValueType::Primitive(primitive) => {
                PrimitiveParam::new(*primitive, self.receive, self.ident).tokens()
            }
            DirectValueType::Record(_) if self.receive != Receive::ByValue => {
                NativeRecordParam::new(self.receive, self.ident, self.rust_type, self.failure)
                    .tokens()
            }
            DirectValueType::Record(_) => {
                RecordInput::new(self.receive, self.rust_type, self.ident, self.failure).native()
            }
            DirectValueType::Enum(_) => {
                PassableParam::new(self.receive, self.ident, self.rust_type).tokens()
            }
            _ => Err(Error::UnsupportedExpansion("direct parameter")),
        }
    }

    pub fn wasm32(self) -> Result<Tokens, Error> {
        match &self.ty {
            DirectValueType::Primitive(primitive) => {
                PrimitiveParam::new(*primitive, self.receive, self.ident).tokens()
            }
            DirectValueType::Record(_) => {
                RecordInput::new(self.receive, self.rust_type, self.ident, self.failure).wasm32()
            }
            DirectValueType::Enum(_) => {
                PassableParam::new(self.receive, self.ident, self.rust_type).tokens()
            }
            _ => Err(Error::UnsupportedExpansion("direct parameter")),
        }
    }

    fn argument(receive: Receive, ident: &Ident) -> Result<TokenStream, Error> {
        match receive {
            Receive::ByValue => Ok(quote! { #ident }),
            Receive::ByRef => Ok(quote! { &#ident }),
            Receive::ByMutRef => Ok(quote! { &mut #ident }),
            _ => Err(Error::UnsupportedExpansion(
                "unknown direct parameter receive mode",
            )),
        }
    }
}

pub struct RecordInput {
    receive: Receive,
    rust_type: Type,
    ident: Ident,
    failure: TokenStream,
}

impl RecordInput {
    pub fn new(receive: Receive, rust_type: Type, ident: Ident, failure: TokenStream) -> Self {
        Self {
            receive,
            rust_type,
            ident,
            failure,
        }
    }

    pub fn native(self) -> Result<Tokens, Error> {
        PassableParam::new(self.receive, self.ident, self.rust_type).tokens()
    }

    pub fn wasm32(self) -> Result<Tokens, Error> {
        WasmRecordParam::new(self.receive, self.ident, self.rust_type, self.failure).tokens()
    }
}

struct PrimitiveParam {
    primitive: Primitive,
    receive: Receive,
    ident: Ident,
}

impl PrimitiveParam {
    fn new(primitive: Primitive, receive: Receive, ident: Ident) -> Self {
        Self {
            primitive,
            receive,
            ident,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let ident = &self.ident;
        let ffi_type = wrapper::type_ref::primitive(self.primitive)?;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type],
            owned_values: Vec::new(),
            conversions: self.conversions(),
            writebacks: Vec::new(),
            argument: Input::argument(self.receive, ident)?,
        })
    }

    fn conversions(&self) -> Vec<TokenStream> {
        let ident = &self.ident;
        match self.receive {
            Receive::ByMutRef => vec![quote! { let mut #ident = #ident; }],
            _ => Vec::new(),
        }
    }
}

struct PassableParam {
    receive: Receive,
    ident: Ident,
    rust_type: Type,
}

impl PassableParam {
    fn new(receive: Receive, ident: Ident, rust_type: Type) -> Self {
        Self {
            receive,
            ident,
            rust_type,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let ident = &self.ident;
        let rust_type = &self.rust_type;
        let ffi_type = quote! { <#rust_type as ::boltffi::__private::Passable>::In };
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type],
            owned_values: Vec::new(),
            conversions: self.conversions(),
            writebacks: Vec::new(),
            argument: Input::argument(self.receive, ident)?,
        })
    }

    fn conversions(&self) -> Vec<TokenStream> {
        let ident = &self.ident;
        let rust_type = &self.rust_type;
        match self.receive {
            Receive::ByMutRef => vec![quote! {
                let mut #ident: #rust_type = unsafe {
                    <#rust_type as ::boltffi::__private::Passable>::unpack(#ident)
                };
            }],
            _ => vec![quote! {
                let #ident: #rust_type = unsafe {
                    <#rust_type as ::boltffi::__private::Passable>::unpack(#ident)
                };
            }],
        }
    }
}

pub struct NativeRecordParam {
    receive: Receive,
    ident: Ident,
    rust_type: Type,
    failure: TokenStream,
}

impl NativeRecordParam {
    pub fn new(receive: Receive, ident: Ident, rust_type: Type, failure: TokenStream) -> Self {
        Self {
            receive,
            ident,
            rust_type,
            failure,
        }
    }

    pub fn tokens(self) -> Result<Tokens, Error> {
        match self.receive {
            Receive::ByValue => {
                PassableParam::new(self.receive, self.ident, self.rust_type).tokens()
            }
            Receive::ByRef | Receive::ByMutRef => self.pointer_tokens(),
            _ => Err(Error::UnsupportedExpansion(
                "unknown direct record receive mode",
            )),
        }
    }

    fn pointer_tokens(self) -> Result<Tokens, Error> {
        let ident = &self.ident;
        let ffi_type = self.ffi_type()?;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type],
            owned_values: Vec::new(),
            conversions: vec![self.conversion()?],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }

    fn ffi_type(&self) -> Result<TokenStream, Error> {
        let rust_type = &self.rust_type;
        match self.receive {
            Receive::ByRef => {
                Ok(quote! { *const <#rust_type as ::boltffi::__private::Passable>::In })
            }
            Receive::ByMutRef => {
                Ok(quote! { *mut <#rust_type as ::boltffi::__private::Passable>::In })
            }
            _ => Err(Error::UnsupportedExpansion(
                "unknown borrowed direct record receive mode",
            )),
        }
    }

    fn conversion(&self) -> Result<TokenStream, Error> {
        let ident = &self.ident;
        let rust_type = &self.rust_type;
        let failure = &self.failure;
        match self.receive {
            Receive::ByRef => Ok(quote! {
                if #ident.is_null() {
                    ::boltffi::__private::set_last_error(concat!(stringify!(#ident), ": null direct record pointer"));
                    #failure
                }
                let #ident: &#rust_type = unsafe { &*(#ident as *const #rust_type) };
            }),
            Receive::ByMutRef => Ok(quote! {
                if #ident.is_null() {
                    ::boltffi::__private::set_last_error(concat!(stringify!(#ident), ": null direct record pointer"));
                    #failure
                }
                let #ident: &mut #rust_type = unsafe { &mut *(#ident as *mut #rust_type) };
            }),
            _ => Err(Error::UnsupportedExpansion(
                "unknown borrowed direct record receive mode",
            )),
        }
    }
}

struct WasmRecordParam {
    receive: Receive,
    ident: Ident,
    rust_type: Type,
    failure: TokenStream,
}

impl WasmRecordParam {
    fn new(receive: Receive, ident: Ident, rust_type: Type, failure: TokenStream) -> Self {
        Self {
            receive,
            ident,
            rust_type,
            failure,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let ident = &self.ident;
        let ffi_type = self.ffi_type()?;
        let out = names::Parameter::new(ident).writeback();
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type],
            owned_values: Vec::new(),
            conversions: vec![self.conversion(&out)?],
            writebacks: self.writebacks(&out)?,
            argument: Input::argument(self.receive, ident)?,
        })
    }

    fn ffi_type(&self) -> Result<TokenStream, Error> {
        match self.receive {
            Receive::ByMutRef => Ok(quote! { *mut u8 }),
            Receive::ByValue | Receive::ByRef => Ok(quote! { *const u8 }),
            _ => Err(Error::UnsupportedExpansion(
                "unknown direct record receive mode",
            )),
        }
    }

    fn conversion(&self, out: &Ident) -> Result<TokenStream, Error> {
        let ident = &self.ident;
        let rust_type = &self.rust_type;
        let failure = &self.failure;
        match self.receive {
            Receive::ByMutRef => Ok(quote! {
                let #out = #ident;
                if #out.is_null() {
                    ::boltffi::__private::set_last_error(concat!(stringify!(#ident), ": null direct record pointer"));
                    #failure
                }
                let mut #ident: #rust_type = unsafe {
                    let __boltffi_value =
                        ::core::ptr::read_unaligned(#out as *const <#rust_type as ::boltffi::__private::Passable>::In);
                    <#rust_type as ::boltffi::__private::Passable>::unpack(__boltffi_value)
                };
            }),
            Receive::ByValue | Receive::ByRef => Ok(quote! {
                if #ident.is_null() {
                    ::boltffi::__private::set_last_error(concat!(stringify!(#ident), ": null direct record pointer"));
                    #failure
                }
                let #ident: #rust_type = unsafe {
                    let __boltffi_value =
                        ::core::ptr::read_unaligned(#ident as *const <#rust_type as ::boltffi::__private::Passable>::In);
                    <#rust_type as ::boltffi::__private::Passable>::unpack(__boltffi_value)
                };
            }),
            _ => Err(Error::UnsupportedExpansion(
                "unknown direct record receive mode",
            )),
        }
    }

    fn writebacks(&self, out: &Ident) -> Result<Vec<TokenStream>, Error> {
        let ident = &self.ident;
        let rust_type = &self.rust_type;
        match self.receive {
            Receive::ByMutRef => Ok(vec![quote! {
                unsafe {
                    ::core::ptr::write_unaligned(
                        #out as *mut <#rust_type as ::boltffi::__private::Passable>::In,
                        <#rust_type as ::boltffi::__private::Passable>::pack(#ident)
                    );
                }
            }]),
            Receive::ByValue | Receive::ByRef => Ok(Vec::new()),
            _ => Err(Error::UnsupportedExpansion(
                "unknown direct record receive mode",
            )),
        }
    }
}

impl Tokens {
    pub fn with_direct_writeback(
        mut self,
        rust_type: &Type,
        receiver: &Ident,
        failure: &TokenStream,
    ) -> Self {
        if !self.writebacks.is_empty() {
            return self;
        }
        let out = names::Parameter::new(receiver).writeback();
        let ffi_type = quote! { *mut <#rust_type as ::boltffi::__private::Passable>::In };
        self.ffi_parameters.push(quote! { #out: #ffi_type });
        self.ffi_parameter_types.push(ffi_type);
        self.conversions.push(quote! {
            if #out.is_null() {
                ::boltffi::__private::set_last_error("receiver writeback pointer is null".to_string());
                #failure
            }
        });
        self.writebacks.push(quote! {
            unsafe {
                ::core::ptr::write_unaligned(
                    #out,
                    <#rust_type as ::boltffi::__private::Passable>::pack(#receiver)
                );
            }
        });
        self
    }
}
