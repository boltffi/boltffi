use boltffi_ast::SourceName;
use boltffi_binding::NativeSymbol;
use proc_macro2::Span;
use quote::format_ident;
use syn::{Ident, PathArguments, Type, parse_str};

use crate::expansion::error::Error;

pub struct Locals {
    span: Span,
}

pub struct Class {
    source: Ident,
}

pub struct Symbol {
    spelling: String,
}

pub struct SourceSpelling {
    spelling: String,
}

impl Locals {
    pub const fn new(span: Span) -> Self {
        Self { span }
    }

    pub fn result(&self) -> Ident {
        Ident::new("__boltffi_result", self.span)
    }

    pub fn error(&self) -> Ident {
        Ident::new("__boltffi_error", self.span)
    }

    pub fn success(&self) -> Ident {
        Ident::new("__boltffi_success", self.span)
    }

    pub fn value(&self) -> Ident {
        Ident::new("__boltffi_value", self.span)
    }

    pub fn return_out(&self) -> Ident {
        Ident::new("__boltffi_return_out", self.span)
    }

    pub fn closure(&self) -> Ident {
        Ident::new("__boltffi_closure", self.span)
    }

    pub fn closure_context(&self) -> Ident {
        Ident::new("__boltffi_closure_context", self.span)
    }

    pub fn receiver(&self) -> Ident {
        Ident::new("__boltffi_receiver", self.span)
    }

    pub fn success_out(&self) -> Ident {
        Ident::new("__boltffi_success_out", self.span)
    }

    pub fn stream_items(&self) -> Ident {
        Ident::new("__boltffi_stream_items", self.span)
    }

    pub fn stream_output_slots(&self) -> Ident {
        Ident::new("__boltffi_stream_output_slots", self.span)
    }
}

impl SourceSpelling {
    pub fn new(name: &SourceName) -> Self {
        Self {
            spelling: name.spelling().to_owned(),
        }
    }

    pub fn ident(&self, mismatch: &'static str) -> Result<Ident, Error> {
        parse_str(&self.spelling).map_err(|_| Error::SourceSyntaxMismatch(mismatch))
    }
}

impl Symbol {
    pub fn new(symbol: &NativeSymbol) -> Self {
        Self {
            spelling: symbol.name().as_str().to_owned(),
        }
    }

    pub fn ident(&self) -> Ident {
        format_ident!("{}", self.spelling)
    }
}

impl Class {
    pub fn new(source: &Ident) -> Self {
        Self {
            source: source.clone(),
        }
    }

    pub fn from_type_path(class: &Type) -> Result<Self, Error> {
        let Type::Path(path) = class else {
            return Err(Error::SourceSyntaxMismatch("class type is not a path"));
        };
        if path.qself.is_some() {
            return Err(Error::SourceSyntaxMismatch(
                "class type path is not a plain Rust path",
            ));
        }
        let Some(segment) = path.path.segments.last() else {
            return Err(Error::SourceSyntaxMismatch("class type path is empty"));
        };
        if !matches!(segment.arguments, PathArguments::None) {
            return Err(Error::SourceSyntaxMismatch(
                "class type path has generic arguments",
            ));
        }
        Ok(Self::new(&segment.ident))
    }

    pub fn handle(&self) -> Ident {
        format_ident!("__Boltffi{}Handle", self.source, span = self.source.span())
    }

    pub fn retained_handle(&self) -> Ident {
        format_ident!(
            "__Boltffi{}RetainedHandle",
            self.source,
            span = self.source.span()
        )
    }
}

pub struct ClosureRegistration {
    source: Ident,
}

impl ClosureRegistration {
    pub fn new(source: &Ident) -> Self {
        Self {
            source: source.clone(),
        }
    }

    pub fn call(&self) -> Ident {
        self.ident("call")
    }

    pub fn context(&self) -> Ident {
        self.ident("context")
    }

    pub fn release(&self) -> Ident {
        self.ident("release")
    }

    pub fn owner(&self) -> Ident {
        self.ident("owner")
    }

    fn ident(&self, role: &str) -> Ident {
        let text = self.source.to_string();
        let stem = text.strip_prefix("__boltffi_").unwrap_or(&text);
        Ident::new(&format!("__boltffi_{stem}_{role}"), self.source.span())
    }
}

pub struct NativeClosureRegistration {
    source: Ident,
}

impl NativeClosureRegistration {
    pub fn new(source: &Ident) -> Self {
        Self {
            source: source.clone(),
        }
    }

    pub fn call(&self) -> Ident {
        self.ident("call")
    }

    pub fn context(&self) -> Ident {
        self.ident("context")
    }

    pub fn release(&self) -> Ident {
        self.ident("release")
    }

    fn ident(&self, role: &str) -> Ident {
        format_ident!(
            "__boltffi_{}_{}",
            self.source,
            role,
            span = self.source.span()
        )
    }
}

pub struct ReturnedClosureRegistration {
    owner: Ident,
    channel: &'static str,
}

impl ReturnedClosureRegistration {
    pub fn new(owner: &Ident, channel: &'static str) -> Self {
        Self {
            owner: owner.clone(),
            channel,
        }
    }

    pub fn call(&self) -> Ident {
        self.ident("call")
    }

    pub fn release(&self) -> Ident {
        self.ident("release")
    }

    fn ident(&self, role: &str) -> Ident {
        format_ident!(
            "__boltffi_{}_{}_{}",
            self.owner,
            self.channel,
            role,
            span = self.owner.span()
        )
    }
}

pub struct Parameter {
    source: Ident,
}

impl Parameter {
    pub fn new(source: &Ident) -> Self {
        Self {
            source: source.clone(),
        }
    }

    pub fn pointer(&self) -> Ident {
        self.ident("ptr")
    }

    pub fn length(&self) -> Ident {
        self.ident("len")
    }

    pub fn storage(&self) -> Ident {
        self.ident("storage")
    }

    pub fn buffer(&self) -> Ident {
        self.ident("buffer")
    }

    pub fn writeback(&self) -> Ident {
        self.ident("out")
    }

    pub fn packed(&self) -> Ident {
        self.ident("packed")
    }

    pub fn handle(&self) -> Ident {
        self.ident("handle")
    }

    fn ident(&self, role: &str) -> Ident {
        let text = self.source.to_string();
        let text = text.strip_prefix("r#").unwrap_or(&text);
        let stem = text.trim_start_matches('_');
        let leading_underscores = &text[..text.len() - stem.len()];
        Ident::new(
            &format!("{leading_underscores}__boltffi_{stem}_{role}"),
            Span::mixed_site().located_at(self.source.span()),
        )
    }
}

pub struct ClosureArgument {
    index: usize,
}

impl ClosureArgument {
    pub const fn new(index: usize) -> Self {
        Self { index }
    }

    pub fn value(&self) -> Ident {
        format_ident!("__boltffi_arg{}", self.index)
    }

    pub fn ffi(&self) -> Ident {
        format_ident!("__boltffi_ffi_arg{}", self.index)
    }

    pub fn pointer(&self) -> Ident {
        format_ident!("__boltffi_arg{}_ptr", self.index)
    }

    pub fn length(&self) -> Ident {
        format_ident!("__boltffi_arg{}_len", self.index)
    }

    pub fn wire(&self) -> Ident {
        format_ident!("__boltffi_arg{}_wire", self.index)
    }
}

pub struct RecordField {
    source: Ident,
}

impl RecordField {
    pub fn new(source: &Ident) -> Self {
        Self {
            source: source.clone(),
        }
    }

    pub fn decoded(&self) -> Ident {
        self.ident("decoded")
    }

    pub fn used(&self) -> Ident {
        self.ident("used")
    }

    pub fn wire(&self) -> Ident {
        self.ident("wire")
    }

    fn ident(&self, role: &str) -> Ident {
        format_ident!(
            "__boltffi_{}_{}",
            self.source,
            role,
            span = self.source.span()
        )
    }
}

pub struct PayloadField {
    index: usize,
}

impl PayloadField {
    pub const fn new(index: usize) -> Self {
        Self { index }
    }

    pub fn value(&self) -> Ident {
        self.ident(None)
    }

    pub fn decoded(&self) -> Ident {
        self.ident(Some("decoded"))
    }

    pub fn used(&self) -> Ident {
        self.ident(Some("used"))
    }

    pub fn wire(&self) -> Ident {
        self.ident(Some("wire"))
    }

    fn ident(&self, role: Option<&str>) -> Ident {
        match role {
            Some(role) => format_ident!("__boltffi_payload{}_{}", self.index, role),
            None => format_ident!("__boltffi_payload{}", self.index),
        }
    }
}
