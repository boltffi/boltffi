use std::borrow::Cow;

use boltffi_ast::{
    BaseTrait, ClassDef, ConstantDef, EnumDef, FnSig, FunctionDef, MapKind, MethodDef,
    ParameterDef, ParameterPassing, Path as SourcePath, RecordDef, ReturnDef, TraitBounds,
    TypeExpr,
};
use boltffi_binding::{CanonicalName, HandlePresence, HandleTarget, Primitive, Receive};
use syn::{Ident, Type, parse_str};

use super::{Closure, DecodeTarget, TypeTokens};
use crate::experimental::error::Error;

#[derive(Clone, Copy)]
pub struct Callable<'source> {
    parameters: &'source [ParameterDef],
    returns: CallableReturn<'source>,
    owner: Option<CallableOwner<'source>>,
}

#[derive(Clone, Copy)]
enum CallableOwner<'source> {
    Record(&'source RecordDef),
    Enum(&'source EnumDef),
    Class(&'source ClassDef),
}

#[derive(Clone, Copy)]
enum CallableReturn<'source> {
    Declaration(&'source ReturnDef),
    Constant(&'source TypeExpr),
}

pub struct MethodDeclarations<'source> {
    owner: CallableOwner<'source>,
}

impl<'source> Callable<'source> {
    pub fn function(function: &'source FunctionDef) -> Self {
        Self {
            parameters: &function.parameters,
            returns: CallableReturn::Declaration(&function.returns),
            owner: None,
        }
    }

    pub fn constant(constant: &'source ConstantDef) -> Self {
        Self {
            parameters: &[],
            returns: CallableReturn::Constant(&constant.type_expr),
            owner: None,
        }
    }

    pub fn method(method: &'source MethodDef) -> Self {
        Self {
            parameters: &method.parameters,
            returns: CallableReturn::Declaration(&method.returns),
            owner: None,
        }
    }

    pub fn record_method(method: &'source MethodDef, record: &'source RecordDef) -> Self {
        Self {
            parameters: &method.parameters,
            returns: CallableReturn::Declaration(&method.returns),
            owner: Some(CallableOwner::Record(record)),
        }
    }

    pub fn enum_method(method: &'source MethodDef, enumeration: &'source EnumDef) -> Self {
        Self {
            parameters: &method.parameters,
            returns: CallableReturn::Declaration(&method.returns),
            owner: Some(CallableOwner::Enum(enumeration)),
        }
    }

    pub fn class_method(method: &'source MethodDef, class: &'source ClassDef) -> Self {
        Self {
            parameters: &method.parameters,
            returns: CallableReturn::Declaration(&method.returns),
            owner: Some(CallableOwner::Class(class)),
        }
    }

    pub fn parameter_count(&self) -> usize {
        self.parameters.len()
    }

    pub fn parameters(&self) -> impl Iterator<Item = Parameter<'source>> + '_ {
        self.parameters
            .iter()
            .map(|definition| Parameter::with_owner(definition, self.owner))
    }

    pub fn returns(&self) -> Return<'source> {
        Return::with_owner(self.returns, self.owner)
    }
}

impl<'source> MethodDeclarations<'source> {
    pub fn record(record: &'source RecordDef) -> Self {
        Self {
            owner: CallableOwner::Record(record),
        }
    }

    pub fn enumeration(enumeration: &'source EnumDef) -> Self {
        Self {
            owner: CallableOwner::Enum(enumeration),
        }
    }

    pub fn class(class: &'source ClassDef) -> Self {
        Self {
            owner: CallableOwner::Class(class),
        }
    }

    pub fn resolve(&self, name: &CanonicalName) -> Result<&'source MethodDef, Error> {
        let binding_name = name.as_path_string();
        let mut matches = self
            .owner
            .methods()
            .iter()
            .filter(|method| method.name.as_path_string() == binding_name);
        match (matches.next(), matches.next()) {
            (Some(method), None) => Ok(method),
            (None, _) => Err(Error::SourceSyntaxMismatch(self.owner.missing_message())),
            (Some(_), Some(_)) => Err(Error::SourceSyntaxMismatch(self.owner.ambiguous_message())),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Parameter<'source> {
    definition: &'source ParameterDef,
    owner: Option<CallableOwner<'source>>,
}

impl<'source> Parameter<'source> {
    pub fn new(definition: &'source ParameterDef) -> Self {
        Self {
            definition,
            owner: None,
        }
    }

    fn with_owner(
        definition: &'source ParameterDef,
        owner: Option<CallableOwner<'source>>,
    ) -> Self {
        Self { definition, owner }
    }

    pub fn ident(self) -> Result<Ident, Error> {
        parse_str(self.definition.name.spelling()).map_err(|_| {
            Error::SourceSyntaxMismatch("source parameter name is not a Rust identifier")
        })
    }

    pub fn written_type(self) -> Result<Type, Error> {
        TypeTokens::new(self.type_expr().as_ref()).map(TypeTokens::into_type)
    }

    pub fn value_type(self, receive: Receive) -> Result<Type, Error> {
        DecodeTarget::new(self.definition.passing, receive, self.type_expr().as_ref())
            .map(|target| target.parameter().clone())
    }

    pub fn decode_target(self, receive: Receive) -> Result<DecodeTarget, Error> {
        DecodeTarget::new(self.definition.passing, receive, self.type_expr().as_ref())
    }

    pub fn closure(self, presence: HandlePresence) -> Result<Closure, Error> {
        Closure::new(&self.definition.type_expr, presence)
    }

    pub fn handle(self, target: &HandleTarget, presence: HandlePresence) -> Result<(), Error> {
        Handle::new(self.type_expr().as_ref()).matches(target, presence)
    }

    pub fn class_handle(
        self,
        target: &HandleTarget,
        presence: HandlePresence,
        receive: Receive,
    ) -> Result<ClassHandle, Error> {
        self.handle(target, presence)?;
        let source_type = self.type_expr();
        let handle = Handle::new(source_type.as_ref());
        let type_expr = handle.source_for_presence(presence)?;
        let ty = match (self.definition.passing, receive) {
            (ParameterPassing::Value, Receive::ByValue)
            | (ParameterPassing::Ref, Receive::ByRef)
            | (ParameterPassing::RefMut, Receive::ByMutRef) => {
                TypeTokens::new(type_expr)?.into_type()
            }
            _ => {
                return Err(Error::SourceSyntaxMismatch(
                    "source class handle passing does not match binding receive mode",
                ));
            }
        };
        Ok(ClassHandle { ty, presence })
    }

    pub fn callback_object(
        self,
        target: &HandleTarget,
        presence: HandlePresence,
    ) -> Result<CallbackObject, Error> {
        self.handle(target, presence)?;
        let source_type = self.type_expr();
        let handle = Handle::new(source_type.as_ref());
        let type_expr = handle.source_for_presence(presence)?;
        CallbackObject::new(presence, type_expr)
    }

    pub fn scalar_option(self, primitive: Primitive) -> Result<(), Error> {
        let source_type = self.type_expr();
        let TypeExpr::Option(inner) = source_type.as_ref() else {
            return Err(Error::SourceSyntaxMismatch(
                "source parameter is not an optional scalar",
            ));
        };
        let TypeExpr::Primitive(source) = inner.as_ref() else {
            return Err(Error::SourceSyntaxMismatch(
                "source optional parameter is not scalar",
            ));
        };
        (Primitive::from(*source) == primitive)
            .then_some(())
            .ok_or(Error::SourceSyntaxMismatch(
                "source optional scalar does not match binding primitive",
            ))
    }

    pub fn direct_vec(self) -> Result<(), Error> {
        match (self.definition.passing, self.type_expr().as_ref()) {
            (ParameterPassing::Value, TypeExpr::Vec(_))
            | (ParameterPassing::Ref | ParameterPassing::RefMut, TypeExpr::Slice(_)) => Ok(()),
            _ => Err(Error::SourceSyntaxMismatch(
                "source parameter is not a direct vector",
            )),
        }
    }

    pub fn direct_vec_element_type(self) -> Result<Type, Error> {
        let source_type = self.type_expr();
        let (TypeExpr::Vec(element) | TypeExpr::Slice(element)) = source_type.as_ref() else {
            return Err(Error::SourceSyntaxMismatch(
                "source direct-vector parameter is missing element type",
            ));
        };
        TypeTokens::new(element).map(TypeTokens::into_type)
    }

    fn type_expr(&self) -> Cow<'source, TypeExpr> {
        SelfTypeSubstitution::from_owner(self.owner).type_expr(&self.definition.type_expr)
    }
}

pub struct ClassHandle {
    ty: Type,
    presence: HandlePresence,
}

impl ClassHandle {
    pub fn ty(&self) -> &Type {
        &self.ty
    }

    pub const fn presence(&self) -> HandlePresence {
        self.presence
    }
}

#[derive(Clone, Copy)]
pub enum CallbackCarrier {
    BoxedDyn,
    ArcDyn,
    ImplTrait,
}

impl CallbackCarrier {
    fn from_type_expr(type_expr: &TypeExpr) -> Result<Self, Error> {
        match type_expr {
            TypeExpr::Boxed(inner) => match inner.as_ref() {
                TypeExpr::Dyn(bounds) if matches!(&bounds.base, BaseTrait::Named { .. }) => {
                    Ok(Self::BoxedDyn)
                }
                _ => Err(Error::SourceSyntaxMismatch(
                    "source callback handle is not a boxed trait object",
                )),
            },
            TypeExpr::Arc(inner) => match inner.as_ref() {
                TypeExpr::Dyn(bounds) if matches!(&bounds.base, BaseTrait::Named { .. }) => {
                    Ok(Self::ArcDyn)
                }
                _ => Err(Error::SourceSyntaxMismatch(
                    "source callback handle is not an Arc trait object",
                )),
            },
            TypeExpr::ImplTrait(bounds) if matches!(&bounds.base, BaseTrait::Named { .. }) => {
                Ok(Self::ImplTrait)
            }
            _ => Err(Error::SourceSyntaxMismatch(
                "source type is not a callback handle",
            )),
        }
    }

    fn object_inner(source: &TypeExpr) -> Option<&TypeExpr> {
        match source {
            TypeExpr::Boxed(inner) | TypeExpr::Arc(inner)
                if matches!(
                    inner.as_ref(),
                    TypeExpr::Dyn(bounds) if matches!(&bounds.base, BaseTrait::Named { .. })
                ) =>
            {
                Some(inner)
            }
            _ => None,
        }
    }

    fn value_type(self, source: &TypeExpr) -> Result<Type, Error> {
        match self {
            Self::BoxedDyn | Self::ArcDyn => TypeTokens::new(source).map(TypeTokens::into_type),
            Self::ImplTrait => self.proxy_type(source),
        }
    }

    fn proxy_type(self, source: &TypeExpr) -> Result<Type, Error> {
        let bounds = match source {
            TypeExpr::ImplTrait(bounds) => bounds,
            TypeExpr::Boxed(inner) | TypeExpr::Arc(inner) => {
                let TypeExpr::Dyn(bounds) = inner.as_ref() else {
                    return Err(Error::SourceSyntaxMismatch(
                        "source callback handle is not a trait object container",
                    ));
                };
                bounds
            }
            _ => {
                return Err(Error::SourceSyntaxMismatch(
                    "source type is not a callback handle",
                ));
            }
        };
        let BaseTrait::Named { path, .. } = &bounds.base else {
            return Err(Error::SourceSyntaxMismatch(
                "source callback handle is not a named callback trait",
            ));
        };
        let segment = path.last().ok_or(Error::SourceSyntaxMismatch(
            "source callback trait path is empty",
        ))?;
        if !segment.arguments.is_empty() {
            return Err(Error::UnsupportedExpansion(
                "generic impl-trait callback handle",
            ));
        }
        parse_str(&format!("Foreign{}", segment.name.as_str())).map_err(|_| {
            Error::SourceSyntaxMismatch("callback foreign proxy type is not Rust syntax")
        })
    }
}

pub struct CallbackObject {
    value: Type,
    proxy: Type,
    form: CallbackCarrier,
    presence: HandlePresence,
}

impl CallbackObject {
    fn new(presence: HandlePresence, type_expr: &TypeExpr) -> Result<Self, Error> {
        let form = CallbackCarrier::from_type_expr(type_expr)?;
        let value = form.value_type(type_expr)?;
        let proxy = form.proxy_type(type_expr)?;
        Ok(Self {
            value,
            proxy,
            form,
            presence,
        })
    }

    pub fn value(&self) -> &Type {
        &self.value
    }

    pub fn proxy(&self) -> &Type {
        &self.proxy
    }

    pub const fn form(&self) -> CallbackCarrier {
        self.form
    }

    pub const fn presence(&self) -> HandlePresence {
        self.presence
    }
}

pub enum HandleReturn {
    Class(Box<Type>),
    Callback(Box<CallbackReturn>),
}

pub struct CallbackReturn {
    form: CallbackCarrier,
    presence: HandlePresence,
    proxy: Type,
}

impl CallbackReturn {
    fn new(presence: HandlePresence, type_expr: &TypeExpr) -> Result<Self, Error> {
        let form = CallbackCarrier::from_type_expr(type_expr)?;
        Ok(Self {
            form,
            presence,
            proxy: form.proxy_type(type_expr)?,
        })
    }

    pub const fn form(&self) -> CallbackCarrier {
        self.form
    }

    pub const fn presence(&self) -> HandlePresence {
        self.presence
    }

    pub fn proxy(&self) -> &Type {
        &self.proxy
    }
}

#[derive(Clone, Copy)]
pub struct Return<'source> {
    definition: CallableReturn<'source>,
    owner: Option<CallableOwner<'source>>,
}

impl<'source> Return<'source> {
    pub fn new(definition: &'source ReturnDef) -> Self {
        Self {
            definition: CallableReturn::Declaration(definition),
            owner: None,
        }
    }

    fn with_owner(
        definition: CallableReturn<'source>,
        owner: Option<CallableOwner<'source>>,
    ) -> Self {
        Self { definition, owner }
    }

    pub fn written_type(self) -> Result<Option<Type>, Error> {
        if matches!(
            self.definition,
            CallableReturn::Declaration(ReturnDef::Void)
        ) {
            return Ok(None);
        }
        if self.borrowed_value()? {
            let type_expr = self.value_type()?;
            return TypeTokens::parameter(ParameterPassing::Ref, type_expr.as_ref())
                .map(TypeTokens::into_type)
                .map(Some);
        }
        match self.value_type() {
            Ok(type_expr) => TypeTokens::new(type_expr.as_ref())
                .map(TypeTokens::into_type)
                .map(Some),
            Err(error) => Err(error),
        }
    }

    pub fn borrowed_value(self) -> Result<bool, Error> {
        match self.definition {
            CallableReturn::Declaration(ReturnDef::Value(TypeExpr::Str | TypeExpr::Slice(_)))
            | CallableReturn::Constant(TypeExpr::Str | TypeExpr::Slice(_)) => Ok(true),
            CallableReturn::Declaration(ReturnDef::Value(_)) | CallableReturn::Constant(_) => {
                Ok(false)
            }
            CallableReturn::Declaration(ReturnDef::Void) => Ok(false),
        }
    }

    pub fn value_type(self) -> Result<Cow<'source, TypeExpr>, Error> {
        let substitution = SelfTypeSubstitution::from_owner(self.owner);
        match self.definition {
            CallableReturn::Declaration(ReturnDef::Value(type_expr)) => {
                Ok(substitution.type_expr(type_expr))
            }
            CallableReturn::Declaration(ReturnDef::Void) => Err(Error::SourceSyntaxMismatch(
                "source return does not have a value type",
            )),
            CallableReturn::Constant(type_expr) => Ok(substitution.type_expr(type_expr)),
        }
    }

    pub fn closure(self, presence: HandlePresence) -> Result<Closure, Error> {
        match self.value_type()? {
            Cow::Borrowed(type_expr) => Closure::new(type_expr, presence),
            Cow::Owned(_) => Err(Error::UnsupportedExpansion(
                "self-referential closure return",
            )),
        }
    }

    pub fn handle_return(
        self,
        target: &HandleTarget,
        presence: HandlePresence,
    ) -> Result<HandleReturn, Error> {
        let value = self.value_type()?;
        Handle::new(value.as_ref()).return_shape(target, presence)
    }

    pub fn scalar_option(self, primitive: Primitive) -> Result<(), Error> {
        let type_expr = self.value_type()?;
        let TypeExpr::Option(inner) = type_expr.as_ref() else {
            return Err(Error::SourceSyntaxMismatch(
                "source return is not an optional scalar",
            ));
        };
        let TypeExpr::Primitive(source) = inner.as_ref() else {
            return Err(Error::SourceSyntaxMismatch(
                "source optional return is not scalar",
            ));
        };
        (Primitive::from(*source) == primitive)
            .then_some(())
            .ok_or(Error::SourceSyntaxMismatch(
                "source optional scalar does not match binding primitive",
            ))
    }

    pub fn direct_vec(self) -> Result<(), Error> {
        match self.value_type()?.as_ref() {
            TypeExpr::Vec(_) => Ok(()),
            _ => Err(Error::SourceSyntaxMismatch(
                "source return is not a direct vector",
            )),
        }
    }

    pub fn direct_vec_element_type(self) -> Result<Type, Error> {
        let type_expr = self.value_type()?;
        let TypeExpr::Vec(element) = type_expr.as_ref() else {
            return Err(Error::SourceSyntaxMismatch(
                "source direct-vector return is missing element type",
            ));
        };
        TypeTokens::new(element.as_ref()).map(TypeTokens::into_type)
    }

    pub fn fallible(self) -> Result<Fallible<'source>, Error> {
        match self.value_type()? {
            Cow::Borrowed(TypeExpr::Result { ok, err }) => Ok(Fallible::Borrowed { ok, err }),
            Cow::Owned(TypeExpr::Result { ok, err }) => Ok(Fallible::Owned { ok, err }),
            _ => Err(Error::SourceSyntaxMismatch("source return is not a Result")),
        }
    }
}

#[derive(Clone)]
pub enum Fallible<'source> {
    Borrowed {
        ok: &'source TypeExpr,
        err: &'source TypeExpr,
    },
    Owned {
        ok: Box<TypeExpr>,
        err: Box<TypeExpr>,
    },
}

impl<'source> Fallible<'source> {
    pub fn ok(&self) -> &TypeExpr {
        match self {
            Self::Borrowed { ok, .. } => ok,
            Self::Owned { ok, .. } => ok,
        }
    }

    pub fn error(&self) -> &TypeExpr {
        match self {
            Self::Borrowed { err, .. } => err,
            Self::Owned { err, .. } => err,
        }
    }

    pub fn ok_written_type(&self) -> Result<Type, Error> {
        TypeTokens::new(self.ok()).map(TypeTokens::into_type)
    }

    pub fn error_written_type(&self) -> Result<Type, Error> {
        TypeTokens::new(self.error()).map(TypeTokens::into_type)
    }

    pub fn ok_closure(&self, presence: HandlePresence) -> Result<Closure, Error> {
        match self {
            Self::Borrowed { ok, .. } => Closure::new(ok, presence),
            Self::Owned { .. } => Err(Error::UnsupportedExpansion(
                "self-referential fallible closure return",
            )),
        }
    }

    pub fn ok_handle_return(
        &self,
        target: &HandleTarget,
        presence: HandlePresence,
    ) -> Result<HandleReturn, Error> {
        Handle::new(self.ok()).return_shape(target, presence)
    }
}

impl<'source> CallableOwner<'source> {
    fn methods(self) -> &'source [MethodDef] {
        match self {
            Self::Record(record) => &record.methods,
            Self::Enum(enumeration) => &enumeration.methods,
            Self::Class(class) => &class.methods,
        }
    }

    fn missing_message(self) -> &'static str {
        match self {
            Self::Record(_) => "source record method is missing for binding method",
            Self::Enum(_) => "source enum method is missing for binding method",
            Self::Class(_) => "source class method is missing for binding method",
        }
    }

    fn ambiguous_message(self) -> &'static str {
        match self {
            Self::Record(_) => "source record method name is ambiguous",
            Self::Enum(_) => "source enum method name is ambiguous",
            Self::Class(_) => "source class method name is ambiguous",
        }
    }

    fn self_type(self) -> TypeExpr {
        match self {
            Self::Record(record) => TypeExpr::record(
                record.id.clone(),
                SourcePath::single(record.name.spelling()),
            ),
            Self::Enum(enumeration) => TypeExpr::enumeration(
                enumeration.id.clone(),
                SourcePath::single(enumeration.name.spelling()),
            ),
            Self::Class(class) => {
                TypeExpr::class(class.id.clone(), SourcePath::single(class.name.spelling()))
            }
        }
    }
}

struct SelfTypeSubstitution {
    self_type: Option<TypeExpr>,
}

impl SelfTypeSubstitution {
    fn from_owner(owner: Option<CallableOwner<'_>>) -> Self {
        Self {
            self_type: owner.map(CallableOwner::self_type),
        }
    }

    fn type_expr<'source>(&self, type_expr: &'source TypeExpr) -> Cow<'source, TypeExpr> {
        let Some(self_type) = self.self_type.as_ref() else {
            return Cow::Borrowed(type_expr);
        };
        match type_expr {
            TypeExpr::SelfType => Cow::Owned(self_type.clone()),
            TypeExpr::Boxed(inner) => self.wrapped(TypeExpr::boxed, type_expr, inner),
            TypeExpr::Arc(inner) => self.wrapped(TypeExpr::arc, type_expr, inner),
            TypeExpr::Vec(element) => self.wrapped(TypeExpr::vec, type_expr, element),
            TypeExpr::Slice(element) => self.wrapped(TypeExpr::slice, type_expr, element),
            TypeExpr::Option(inner) => self.wrapped(TypeExpr::option, type_expr, inner),
            TypeExpr::Result { ok, err } => self.result(type_expr, ok, err),
            TypeExpr::Tuple(elements) => self.tuple(type_expr, elements),
            TypeExpr::Map { kind, key, value } => self.map(type_expr, *kind, key, value),
            TypeExpr::Dyn(bounds) => self.bounds(TypeExpr::Dyn, bounds, type_expr),
            TypeExpr::ImplTrait(bounds) => self.bounds(TypeExpr::ImplTrait, bounds, type_expr),
            TypeExpr::FnPtr(signature) => match self.signature(signature) {
                Some(signature) => Cow::Owned(TypeExpr::fn_ptr(signature)),
                None => Cow::Borrowed(type_expr),
            },
            _ => Cow::Borrowed(type_expr),
        }
    }

    fn return_def<'source>(&self, return_def: &'source ReturnDef) -> Cow<'source, ReturnDef> {
        let ReturnDef::Value(type_expr) = return_def else {
            return Cow::Borrowed(return_def);
        };
        match self.type_expr(type_expr) {
            Cow::Borrowed(_) => Cow::Borrowed(return_def),
            Cow::Owned(type_expr) => Cow::Owned(ReturnDef::Value(type_expr)),
        }
    }

    fn wrapped<'source>(
        &self,
        build: impl Fn(TypeExpr) -> TypeExpr,
        original: &'source TypeExpr,
        inner: &'source TypeExpr,
    ) -> Cow<'source, TypeExpr> {
        match self.type_expr(inner) {
            Cow::Borrowed(_) => Cow::Borrowed(original),
            Cow::Owned(inner) => Cow::Owned(build(inner)),
        }
    }

    fn result<'source>(
        &self,
        original: &'source TypeExpr,
        ok: &'source TypeExpr,
        err: &'source TypeExpr,
    ) -> Cow<'source, TypeExpr> {
        let ok = self.type_expr(ok);
        let err = self.type_expr(err);
        match (&ok, &err) {
            (Cow::Borrowed(_), Cow::Borrowed(_)) => Cow::Borrowed(original),
            _ => Cow::Owned(TypeExpr::result(ok.into_owned(), err.into_owned())),
        }
    }

    fn tuple<'source>(
        &self,
        original: &'source TypeExpr,
        elements: &'source [TypeExpr],
    ) -> Cow<'source, TypeExpr> {
        let elements = elements
            .iter()
            .map(|element| self.type_expr(element))
            .collect::<Vec<_>>();
        match elements
            .iter()
            .all(|element| matches!(element, Cow::Borrowed(_)))
        {
            true => Cow::Borrowed(original),
            false => Cow::Owned(TypeExpr::tuple(
                elements
                    .into_iter()
                    .map(Cow::into_owned)
                    .collect::<Vec<_>>(),
            )),
        }
    }

    fn map<'source>(
        &self,
        original: &'source TypeExpr,
        kind: MapKind,
        key: &'source TypeExpr,
        value: &'source TypeExpr,
    ) -> Cow<'source, TypeExpr> {
        let key = self.type_expr(key);
        let value = self.type_expr(value);
        match (&key, &value) {
            (Cow::Borrowed(_), Cow::Borrowed(_)) => Cow::Borrowed(original),
            _ => Cow::Owned(TypeExpr::map(kind, key.into_owned(), value.into_owned())),
        }
    }

    fn bounds<'source>(
        &self,
        build: impl Fn(TraitBounds) -> TypeExpr,
        bounds: &'source TraitBounds,
        original: &'source TypeExpr,
    ) -> Cow<'source, TypeExpr> {
        let BaseTrait::Function(function_trait) = &bounds.base else {
            return Cow::Borrowed(original);
        };
        let Some(signature) = self.signature(&function_trait.signature) else {
            return Cow::Borrowed(original);
        };
        let mut bounds = bounds.clone();
        let BaseTrait::Function(function_trait) = &mut bounds.base else {
            return Cow::Borrowed(original);
        };
        function_trait.signature = signature;
        Cow::Owned(build(bounds))
    }

    fn signature(&self, signature: &FnSig) -> Option<FnSig> {
        let parameters = signature
            .parameters
            .iter()
            .map(|parameter| self.type_expr(parameter))
            .collect::<Vec<_>>();
        let returns = self.return_def(&signature.returns);
        match (
            parameters
                .iter()
                .all(|parameter| matches!(parameter, Cow::Borrowed(_))),
            matches!(returns, Cow::Borrowed(_)),
        ) {
            (true, true) => None,
            _ => Some(FnSig::new(
                parameters
                    .into_iter()
                    .map(Cow::into_owned)
                    .collect::<Vec<_>>(),
                returns.into_owned(),
            )),
        }
    }
}

struct Handle<'source> {
    source: &'source TypeExpr,
}

impl<'source> Handle<'source> {
    const fn new(source: &'source TypeExpr) -> Self {
        Self { source }
    }

    fn matches(&self, target: &HandleTarget, presence: HandlePresence) -> Result<(), Error> {
        self.source_for_presence(presence)
            .and_then(|source| Self::required_matches(source, target))
    }

    fn source_for_presence(&self, presence: HandlePresence) -> Result<&'source TypeExpr, Error> {
        match presence {
            HandlePresence::Required => Ok(self.source),
            HandlePresence::Nullable => match self.source {
                TypeExpr::Option(inner) => Ok(inner),
                _ => Err(Error::SourceSyntaxMismatch("source type is not optional")),
            },
            _ => Err(Error::UnsupportedExpansion("unknown handle presence")),
        }
    }

    fn return_shape(
        &self,
        target: &HandleTarget,
        presence: HandlePresence,
    ) -> Result<HandleReturn, Error> {
        match target {
            HandleTarget::Class(_) => {
                let type_expr = self.source_for_presence(presence)?;
                self.matches(target, presence)?;
                TypeTokens::new(type_expr)
                    .map(TypeTokens::into_type)
                    .map(Box::new)
                    .map(HandleReturn::Class)
            }
            HandleTarget::Callback(_) => {
                let type_expr = self.source_for_presence(presence)?;
                self.matches(target, presence)?;
                CallbackReturn::new(presence, type_expr)
                    .map(Box::new)
                    .map(HandleReturn::Callback)
            }
            _ => Err(Error::UnsupportedExpansion("unknown handle return target")),
        }
    }

    fn required_matches(source: &TypeExpr, target: &HandleTarget) -> Result<(), Error> {
        match (source, target) {
            (TypeExpr::Class { .. }, HandleTarget::Class(_)) => Ok(()),
            (TypeExpr::ImplTrait(bounds), HandleTarget::Callback(_))
                if matches!(&bounds.base, BaseTrait::Named { .. }) =>
            {
                Ok(())
            }
            (TypeExpr::Boxed(_) | TypeExpr::Arc(_), HandleTarget::Callback(_))
                if CallbackCarrier::object_inner(source).is_some() =>
            {
                Ok(())
            }
            _ => Err(Error::SourceSyntaxMismatch(
                "source handle type does not match binding handle target",
            )),
        }
    }
}
