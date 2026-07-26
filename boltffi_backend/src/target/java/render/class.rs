use std::collections::HashSet;

use askama::Template as AskamaTemplate;
use boltffi_binding::{ClassDecl, ClassId, HandlePresence, Native, native};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{AuxChunk, Emitted, RenderContext, Result},
    target::java::{
        JavaFile, JavaHost, JavaPackage, JavaVersion,
        admission::ClassShape,
        name_style::Name,
        primitive::Primitive,
        render::{
            Stream,
            call::{AssociatedCallContext, Call, Receiver},
            native::Method,
            signature::{ErasedSignature, ReturnType, ValueType},
        },
        syntax::{
            ArgumentList, Expression, Identifier, Javadoc, Statement, TypeIdentifier, TypeName,
        },
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/java/class.java", escape = "none")]
struct ClassTemplate<'class> {
    package: &'class JavaPackage,
    class: &'class Class,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Class {
    name: TypeIdentifier,
    version: JavaVersion,
    handle: Primitive,
    release: Statement,
    release_native: Method,
    constructors: Vec<Constructor>,
    factories: Vec<Call>,
    static_methods: Vec<Call>,
    instance_methods: Vec<Call>,
    streams: Vec<Stream>,
    doc: Option<Javadoc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Constructor {
    call: Call,
    arguments: ArgumentList,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ConstructorSignature(Vec<ValueType>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassHandle {
    ty: TypeName,
    carrier: Primitive,
    presence: HandlePresence,
}

impl Class {
    pub fn from_declaration(
        declaration: &ClassDecl<Native>,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        ClassShape::classify(declaration).require_supported()?;
        let name = Name::new(declaration.name()).type_name(version)?;
        let handle = Primitive::from_handle_carrier(declaration.handle())?;
        let release_native = Method::from_symbol(declaration.release(), bridge, version)?;
        release_native.validate_return(&ReturnType::Void)?;
        let release = Statement::expression(release_native.call(
            native_owner,
            [Expression::this().member(Identifier::known("handle"))],
        )?);
        let (_, constructors, factories) = declaration.initializers().iter().try_fold(
            (HashSet::new(), Vec::new(), Vec::new()),
            |(mut signatures, mut constructors, mut factories), initializer| -> Result<_> {
                let helper = Identifier::parse_for(
                    format!("__boltffiCreateHandle{}", initializer.id().raw()),
                    version,
                )?;
                let call = Call::from_class_initializer(
                    initializer,
                    declaration.id(),
                    helper,
                    AssociatedCallContext::local(bridge, native_owner, version, context),
                )?;
                match signatures.insert(ConstructorSignature::from_call(&call)) {
                    true => constructors.push(Constructor::new(call)),
                    false => factories.push(Call::from_class_factory(
                        initializer,
                        bridge,
                        native_owner,
                        None,
                        version,
                        context,
                    )?),
                }
                Ok((signatures, constructors, factories))
            },
        )?;
        let static_methods = declaration
            .methods()
            .iter()
            .filter(|method| method.callable().receiver().is_none())
            .map(|method| {
                Call::from_method(method, None, bridge, native_owner, None, version, context)
            })
            .collect::<Result<Vec<_>>>()?;
        let instance_methods = declaration
            .methods()
            .iter()
            .filter_map(|method| {
                method
                    .callable()
                    .receiver()
                    .map(|receive| (method, receive))
            })
            .map(|(method, receive)| {
                Receiver::class(name.clone(), declaration.handle(), receive).and_then(|receiver| {
                    Call::from_method(
                        method,
                        Some(receiver),
                        bridge,
                        native_owner,
                        None,
                        version,
                        context,
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let streams = Stream::for_class(declaration.id(), bridge, native_owner, version, context)?;
        Ok(Self {
            name,
            version,
            handle,
            release,
            release_native,
            constructors,
            factories,
            static_methods,
            instance_methods,
            streams,
            doc: declaration.meta().doc().map(Javadoc::new),
        })
    }

    pub fn render(&self, package: &JavaPackage) -> Result<Emitted> {
        let emitted = Emitted::primary(
            ClassTemplate {
                package,
                class: self,
            }
            .render()?,
        )
        .with_aux(AuxChunk::ForwardDecl(self.release_native.render()?.into()));
        let emitted = match self.calls().any(Call::requires_wire_runtime) {
            true => emitted.with_aux(crate::target::java::codec::Runtime::helper()?),
            false => emitted,
        };
        let emitted = match self.calls().any(Call::requires_direct_vector_runtime) {
            true => emitted.with_aux(crate::target::java::codec::Runtime::direct_vector_helper()?),
            false => emitted,
        };
        let emitted = match self.calls().any(Call::requires_async_runtime) {
            true => emitted.with_aux(crate::target::java::codec::Runtime::async_helper()?),
            false => emitted,
        };
        let emitted = self
            .calls()
            .try_fold(emitted, |emitted, call| -> Result<_> {
                Ok(call
                    .native_forwards()?
                    .into_iter()
                    .fold(emitted, Emitted::with_aux))
            })?;
        self.streams.iter().try_fold(emitted, |emitted, stream| {
            let emitted = stream
                .runtime_helpers(self.version)?
                .into_iter()
                .fold(emitted, Emitted::with_aux);
            Ok(stream
                .native_forwards()?
                .into_iter()
                .fold(emitted, Emitted::with_aux))
        })
    }

    pub fn file_for(declaration: &ClassDecl<Native>, version: JavaVersion) -> Result<JavaFile> {
        Name::new(declaration.name())
            .type_name(version)
            .and_then(|name| JavaFile::parse_for(name.as_str(), version))
    }

    pub fn type_name_for(
        id: ClassId,
        context: &RenderContext<Native>,
        version: JavaVersion,
    ) -> Result<TypeIdentifier> {
        context
            .class(id)
            .ok_or(JavaHost::broken_bridge_contract(
                "class handle target has no class declaration",
            ))
            .and_then(|declaration| Name::new(declaration.name()).type_name(version))
    }

    pub fn name(&self) -> &TypeIdentifier {
        &self.name
    }

    pub const fn handle(&self) -> Primitive {
        self.handle
    }

    pub fn release(&self) -> &Statement {
        &self.release
    }

    pub fn constructors(&self) -> &[Constructor] {
        &self.constructors
    }

    pub fn factories(&self) -> &[Call] {
        &self.factories
    }

    pub fn static_methods(&self) -> &[Call] {
        &self.static_methods
    }

    pub fn instance_methods(&self) -> &[Call] {
        &self.instance_methods
    }

    pub fn streams(&self) -> &[Stream] {
        &self.streams
    }

    pub fn doc(&self) -> Option<&Javadoc> {
        self.doc.as_ref()
    }

    pub fn calls(&self) -> impl Iterator<Item = &Call> {
        self.constructors
            .iter()
            .map(Constructor::call)
            .chain(&self.factories)
            .chain(&self.static_methods)
            .chain(&self.instance_methods)
    }

    pub fn signatures(&self) -> Vec<ErasedSignature> {
        ["close", "rawHandle"]
            .into_iter()
            .map(|name| ErasedSignature::new(Identifier::known(name), []))
            .chain(
                self.calls()
                    .map(Call::signature)
                    .map(|signature| signature.erased()),
            )
            .chain(
                self.streams
                    .iter()
                    .map(|stream| stream.signature(self.version)),
            )
            .collect()
    }
}

impl Constructor {
    fn new(call: Call) -> Self {
        let arguments = call
            .parameters()
            .iter()
            .map(|parameter| Expression::identifier(parameter.name().clone()))
            .collect();
        Self { call, arguments }
    }

    pub fn call(&self) -> &Call {
        &self.call
    }

    pub fn arguments(&self) -> &ArgumentList {
        &self.arguments
    }
}

impl ConstructorSignature {
    fn from_call(call: &Call) -> Self {
        Self(
            call.parameters()
                .iter()
                .map(|parameter| parameter.ty().clone())
                .collect(),
        )
    }
}

impl ClassHandle {
    pub fn new(
        id: ClassId,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
        version: JavaVersion,
        context: &RenderContext<Native>,
        package: Option<&JavaPackage>,
    ) -> Result<Self> {
        let name = Class::type_name_for(id, context, version)?;
        Ok(Self {
            ty: match package {
                Some(package) => package.type_name(name),
                None => TypeName::named(name),
            },
            carrier: Primitive::from_handle_carrier(carrier)?,
            presence,
        })
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    pub const fn carrier(&self) -> Primitive {
        self.carrier
    }

    pub fn native_argument(&self, value: Expression) -> Result<Expression> {
        match self.presence {
            HandlePresence::Required => {
                Ok(value.call(Identifier::known("rawHandle"), ArgumentList::default()))
            }
            HandlePresence::Nullable => Ok(value.clone().equal(Expression::null()).conditional(
                Expression::long(0),
                value.call(Identifier::known("rawHandle"), ArgumentList::default()),
            )),
            _ => Err(JavaHost::unsupported("class handle presence")),
        }
    }

    pub fn value_statements(&self, value: Expression) -> Result<Vec<Statement>> {
        match self.presence {
            HandlePresence::Required => Ok(vec![Statement::return_value(Expression::construct(
                self.ty.clone(),
                [value].into_iter().collect(),
            ))]),
            HandlePresence::Nullable => {
                let handle = Identifier::known("__boltffi_handle");
                Ok(vec![
                    Statement::value(TypeName::primitive(self.carrier), handle.clone(), value),
                    Statement::return_value(
                        Expression::identifier(handle.clone())
                            .equal(Expression::long(0))
                            .conditional(
                                Expression::null(),
                                Expression::construct(
                                    self.ty.clone(),
                                    [Expression::identifier(handle)].into_iter().collect(),
                                ),
                            ),
                    ),
                ])
            }
            _ => Err(JavaHost::unsupported("class handle presence")),
        }
    }

    pub fn value_expression(&self, value: Expression) -> Result<Expression> {
        let wrapped = Expression::construct(self.ty.clone(), [value.clone()].into_iter().collect());
        match self.presence {
            HandlePresence::Required => Ok(wrapped),
            HandlePresence::Nullable => Ok(value
                .equal(Expression::long(0))
                .conditional(Expression::null(), wrapped)),
            _ => Err(JavaHost::unsupported("class handle presence")),
        }
    }
}
