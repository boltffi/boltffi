use std::collections::HashSet;

use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CanonicalName, ClassDecl, ClassId, ConstantOwner, HandlePresence, Native, native,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{AuxChunk, Emitted, Error, RenderContext, Result},
    target::java::{
        JavaFile, JavaHost, JavaPackage, JavaVersion,
        admission::ClassShape,
        name_style::Name,
        primitive::Primitive,
        render::{
            AssociatedConstants, Constant, Stream,
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
    constants: AssociatedConstants,
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
            [Expression::identifier(Identifier::known(
                "__boltffi_handle",
            ))],
        )?);
        let primary = declaration.initializers().iter().find(|initializer| {
            initializer.name() == &CanonicalName::single("new")
                && initializer
                    .callable()
                    .params()
                    .iter()
                    .any(|parameter| parameter.meta().default().is_some())
        });
        let mut initializers =
            primary
                .into_iter()
                .chain(declaration.initializers().iter().filter(|initializer| {
                    primary.is_none_or(|primary| primary.id() != initializer.id())
                }));
        let constructor_name = name.identifier().clone();
        let long_constructor_signature =
            ErasedSignature::new(constructor_name.clone(), [ValueType::Primitive(handle)]);
        let mut constructor_signatures = HashSet::from([ErasedSignature::new(
            constructor_name.clone(),
            [ValueType::Reference(TypeName::qualified(
                ["java", "util", "concurrent", "atomic"]
                    .into_iter()
                    .map(Identifier::known)
                    .collect(),
                TypeIdentifier::known("AtomicLong", version),
            ))],
        )]);
        let mut constructors = Vec::new();
        let mut factories = Vec::new();
        initializers.try_for_each(|initializer| -> Result<()> {
            if initializer.callable().execution().uses_async_execution() {
                factories.push(Call::from_class_factory(
                    initializer,
                    bridge,
                    native_owner,
                    None,
                    version,
                    context,
                )?);
                return Ok(());
            }
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
            let signature = ErasedSignature::new(
                constructor_name.clone(),
                call.parameters()
                    .iter()
                    .map(|parameter| parameter.ty().clone()),
            );
            let is_primary = primary.is_some_and(|primary| primary.id() == initializer.id());
            if is_primary {
                std::iter::once(signature)
                    .chain(
                        call.overloads()
                            .iter()
                            .map(|overload| overload.erased_signature(&constructor_name)),
                    )
                    .try_for_each(|signature| {
                        if let Some(collision) = constructor_signatures.replace(signature) {
                            return Err(Error::JavaNameCollision {
                                scope: format!("{name} constructors"),
                                name: collision.to_string(),
                            });
                        }
                        Ok(())
                    })?;
                constructors.push(Constructor::new(call));
                return Ok(());
            }
            match signature != long_constructor_signature
                && constructor_signatures.insert(signature)
            {
                true => {
                    if !call.overloads().is_empty() {
                        factories.push(call.constructor_factory(
                            Name::new(initializer.name()).function(version)?,
                            name.clone(),
                        )?);
                    }
                    constructors.push(Constructor::new(call.without_default_overloads()));
                }
                false => factories.push(Call::from_class_factory(
                    initializer,
                    bridge,
                    native_owner,
                    None,
                    version,
                    context,
                )?),
            }
            Ok(())
        })?;
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
            constants: AssociatedConstants::from_owner(
                ConstantOwner::Class(declaration.id()),
                bridge,
                native_owner,
                version,
                context,
            )?,
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
        let emitted = self
            .streams
            .iter()
            .try_fold(emitted, |emitted, stream| -> Result<_> {
                let emitted = stream
                    .runtime_helpers(self.version)?
                    .into_iter()
                    .fold(emitted, Emitted::with_aux);
                Ok(stream
                    .native_forwards()?
                    .into_iter()
                    .fold(emitted, Emitted::with_aux))
            })?;
        self.constants.extend(emitted)
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

    pub fn constants(&self) -> &[Constant] {
        self.constants.as_slice()
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
            .chain(std::iter::once(ErasedSignature::new(
                Identifier::known("__boltffiFromHandle"),
                [ValueType::Primitive(self.handle)],
            )))
            .chain(
                self.constructors
                    .iter()
                    .map(Constructor::call)
                    .map(Call::signature)
                    .map(|signature| signature.erased()),
            )
            .chain(
                self.factories
                    .iter()
                    .chain(&self.static_methods)
                    .chain(&self.instance_methods)
                    .flat_map(Call::signatures),
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
            HandlePresence::Required => {
                Ok(vec![Statement::return_value(self.value_expression(value)?)])
            }
            HandlePresence::Nullable => {
                let handle = Identifier::known("__boltffi_handle");
                Ok(vec![
                    Statement::value(TypeName::primitive(self.carrier), handle.clone(), value),
                    Statement::return_value(self.value_expression(Expression::identifier(handle))?),
                ])
            }
            _ => Err(JavaHost::unsupported("class handle presence")),
        }
    }

    pub fn value_expression(&self, value: Expression) -> Result<Expression> {
        let wrapped = Expression::static_call(
            self.ty.clone(),
            Identifier::known("__boltffiFromHandle"),
            [value.clone()].into_iter().collect(),
        );
        match self.presence {
            HandlePresence::Required => Ok(wrapped),
            HandlePresence::Nullable => Ok(value
                .equal(Expression::long(0))
                .conditional(Expression::null(), wrapped)),
            _ => Err(JavaHost::unsupported("class handle presence")),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedClassArgument {
    pub parameter: Identifier,
    pub local: Identifier,
    pub release: Identifier,
    pub presence: HandlePresence,
}

#[derive(AskamaTemplate)]
#[template(path = "target/java/owned_call.java", escape = "none")]
pub struct OwnedCallTemplate<'call> {
    pub native_owner: &'call TypeIdentifier,
    pub owned: Vec<&'call OwnedClassArgument>,
    pub bindings: &'call [Statement],
    pub body: &'call [Statement],
}
