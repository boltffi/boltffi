use std::{collections::BTreeSet, fmt};

use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CanonicalName, ClassDecl, ClassId, ExportedMethodDecl, HandlePresence, InitializerDecl, Native,
    NativeSymbol, Receive,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{Emitted, RenderContext, Result},
    target::kotlin::{
        KotlinFactoryStyle, KotlinHost,
        name_style::Name,
        render::{
            function::{ExportedCall, ExportedCallRenderer, ReceiverCarrier},
            signature::validate_reserved_members,
        },
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/class.kt", escape = "none")]
struct ClassTemplate {
    class: Class,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Class {
    name: TypeName,
    release: Identifier,
    initializers: Vec<Initializer>,
    static_methods: Vec<ExportedCall>,
    instance_methods: Vec<ExportedCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Initializer {
    call: ExportedCall,
    constructor: bool,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ConstructorSignature(Vec<String>);

/// Handle read through the generated closed-check accessor. Check-then-act:
/// a `close()` racing the native call is not covered.
fn guarded_handle(receiver: impl fmt::Display, presence: HandlePresence) -> Result<Expression> {
    let accessor = Identifier::parse("boltffiHandle")?;
    match presence {
        HandlePresence::Required => Ok(Expression::call(
            receiver,
            accessor,
            ArgumentList::from_iter([]),
        )),
        HandlePresence::Nullable => {
            Ok(
                Expression::safe_call(receiver, accessor, ArgumentList::from_iter([]))
                    .or_else(Expression::long(0)),
            )
        }
        _ => Err(KotlinHost::unsupported("unknown class handle presence")),
    }
}

impl Class {
    pub fn from_declaration(
        decl: &ClassDecl<Native>,
        host: &KotlinHost,
        factory_style: KotlinFactoryStyle,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let initializers = decl
            .initializers()
            .iter()
            .map(|initializer| Initializer::from_declaration(initializer, host, bridge, context))
            .collect::<Result<Vec<_>>>()?;
        let name = Self::type_name(decl.name())?;
        let instance_methods = Self::methods(
            decl.methods(),
            Some(guarded_handle("this", HandlePresence::Required)?),
            host,
            bridge,
            context,
        )?;
        validate_reserved_members(
            &name,
            &["close", "boltffiHandle"],
            instance_methods
                .iter()
                .filter(|method| method.parameters().is_empty())
                .map(ExportedCall::name),
        )?;
        Ok(Self {
            release: Identifier::escape(decl.release().name().as_str())?,
            initializers: Initializer::apply_factory_style(initializers, factory_style),
            static_methods: Self::methods(decl.methods(), None, host, bridge, context)?,
            instance_methods,
            name,
        })
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(ClassTemplate { class: self }.render()?))
    }

    pub fn type_name_from_id(id: ClassId, context: &RenderContext<Native>) -> Result<TypeName> {
        context
            .class(id)
            .ok_or(KotlinHost::broken_bridge_contract(
                "class handle target has no class declaration",
            ))
            .and_then(|decl| Self::type_name(decl.name()))
    }

    pub fn name(&self) -> &TypeName {
        &self.name
    }

    pub fn release(&self) -> &Identifier {
        &self.release
    }

    pub fn initializers(&self) -> &[Initializer] {
        &self.initializers
    }

    pub fn static_methods(&self) -> &[ExportedCall] {
        &self.static_methods
    }

    pub fn instance_methods(&self) -> &[ExportedCall] {
        &self.instance_methods
    }

    fn type_name(name: &CanonicalName) -> Result<TypeName> {
        Ok(Name::new(name).type_name())
    }

    fn methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        receiver: Option<Expression>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Vec<ExportedCall>> {
        let calls = ExportedCallRenderer::new(host, bridge, context);
        methods
            .iter()
            .filter(|method| method.callable().receiver().is_some() == receiver.is_some())
            .map(|method| {
                let receiver = match (method.callable().receiver(), receiver.clone()) {
                    (Some(Receive::ByRef | Receive::ByMutRef | Receive::ByValue), Some(handle)) => {
                        Some(ReceiverCarrier::direct(handle))
                    }
                    (None, None) => None,
                    _ => {
                        return Err(KotlinHost::unsupported("class method receiver"));
                    }
                };
                calls.exported(
                    Name::new(method.name()).function()?,
                    method.target(),
                    method.callable(),
                    receiver,
                )
            })
            .collect()
    }
}

impl Initializer {
    pub fn from_declaration(
        initializer: &InitializerDecl<Native>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        ExportedCallRenderer::new(host, bridge, context)
            .exported(
                Name::new(initializer.name()).function()?,
                initializer.symbol(),
                initializer.callable(),
                None,
            )
            .map(|call| Self {
                call,
                constructor: true,
            })
    }

    pub fn call(&self) -> &ExportedCall {
        &self.call
    }

    pub fn constructor(&self) -> bool {
        self.constructor
    }

    fn apply_factory_style(initializers: Vec<Self>, style: KotlinFactoryStyle) -> Vec<Self> {
        match style {
            KotlinFactoryStyle::Constructors => Self::dedupe_constructors(initializers),
            KotlinFactoryStyle::CompanionMethods => initializers
                .into_iter()
                .map(Self::companion_method)
                .collect(),
        }
    }

    fn dedupe_constructors(initializers: Vec<Self>) -> Vec<Self> {
        let (_, initializers) = initializers.into_iter().fold(
            (BTreeSet::new(), Vec::new()),
            |(mut signatures, mut initializers), mut initializer| {
                if initializer.constructor {
                    initializer.constructor =
                        signatures.insert(ConstructorSignature::from_call(initializer.call()));
                }
                initializers.push(initializer);
                (signatures, initializers)
            },
        );
        initializers
    }

    fn companion_method(mut self) -> Self {
        self.constructor = false;
        self
    }
}

impl ConstructorSignature {
    fn from_call(call: &ExportedCall) -> Self {
        Self(
            call.parameters()
                .iter()
                .map(|parameter| parameter.ty().to_string())
                .collect(),
        )
    }
}

pub struct ClassHandle {
    ty: TypeName,
    presence: HandlePresence,
}

impl ClassHandle {
    pub fn new(
        id: ClassId,
        presence: HandlePresence,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Ok(Self {
            ty: Class::type_name_from_id(id, context)?,
            presence,
        })
    }

    pub fn ty(&self) -> Result<TypeName> {
        match self.presence {
            HandlePresence::Required => Ok(self.ty.clone()),
            HandlePresence::Nullable => Ok(self.ty.clone().nullable()),
            _ => Err(KotlinHost::unsupported("unknown class handle presence")),
        }
    }

    pub fn parameter_argument(&self, value: Expression) -> Result<Expression> {
        guarded_handle(value, self.presence)
    }

    pub fn value_expression(&self, value: Expression) -> Result<Expression> {
        match self.presence {
            HandlePresence::Required => Ok(Expression::construct(
                self.ty.clone(),
                [value].into_iter().collect(),
            )),
            HandlePresence::Nullable => Ok(Expression::conditional(
                value.clone().equal(Expression::long(0)),
                Expression::null(),
                Expression::construct(self.ty.clone(), [value].into_iter().collect()),
            )),
            _ => Err(KotlinHost::unsupported("unknown class handle presence")),
        }
    }

    pub fn value_statements(&self, value: Expression) -> Result<Vec<Statement>> {
        match self.presence {
            HandlePresence::Required => self
                .value_expression(value)
                .map(Statement::expression)
                .map(|statement| vec![statement]),
            HandlePresence::Nullable => {
                let raw_handle = Identifier::parse("__boltffi_handle")?;
                Ok(vec![
                    Statement::value(raw_handle, value),
                    Statement::expression(self.value_expression(Expression::identifier(
                        Identifier::parse("__boltffi_handle")?,
                    ))?),
                ])
            }
            _ => Err(KotlinHost::unsupported("unknown class handle presence")),
        }
    }
}
