use std::collections::{BTreeMap, btree_map::Entry};

use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ClosureParameter, DeclarationRef, ExportedCallable, HandlePresence, IncomingParam, IntoRust,
    Native,
};

use crate::{
    bridge::jni::{ClosureRegistration, JniBridgeContract},
    core::{Emitted, Error, RenderContext, RenderedDeclaration, Result},
    target::java::{
        JavaFile, JavaHost, JavaPackage, JavaVersion,
        codec::Runtime,
        render::{callback::Method, native::Method as NativeMethod},
        syntax::{ArgumentList, Expression, Identifier, TypeIdentifier, TypeName},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/java/closure.java", escape = "none")]
struct ClosureTemplate<'closure> {
    package: &'closure JavaPackage,
    closure: &'closure Closure,
}

pub struct Closure {
    name: TypeIdentifier,
    map_name: TypeIdentifier,
    callbacks_name: TypeIdentifier,
    method: Method,
    native_methods: Vec<NativeMethod>,
}

pub struct ClosureHandle {
    bridge: TypeName,
    presence: HandlePresence,
}

pub struct Closures {
    closures: Vec<Closure>,
}

impl Closure {
    pub fn from_parameter(
        closure: &ClosureParameter<Native, IntoRust>,
        registration: &ClosureRegistration,
        bridge: &JniBridgeContract,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Self::type_identifier(closure, version)?;
        let native_methods = registration
            .success_out()
            .map(|success_out| {
                bridge
                    .success_out_writers()
                    .iter()
                    .find(|writer| writer.method() == success_out.writer())
                    .ok_or(JavaHost::broken_bridge_contract(
                        "closure success out argument has a JNI writer",
                    ))
                    .and_then(|writer| NativeMethod::from_success_out_writer(writer, version))
            })
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            map_name: TypeIdentifier::parse(format!("{name}Map"), version)?,
            callbacks_name: TypeIdentifier::parse(registration.class().class_name(), version)?,
            method: Method::from_closure(closure, registration, version, context)?,
            native_methods,
            name,
        })
    }

    pub fn render(&self, package: &JavaPackage) -> Result<Emitted> {
        let emitted = Emitted::primary(
            ClosureTemplate {
                package,
                closure: self,
            }
            .render()?,
        );
        let emitted = match self.method.requires_wire_runtime() {
            true => emitted.with_aux(Runtime::helper()?),
            false => emitted,
        };
        let emitted = match self.method.requires_direct_vector_runtime() {
            true => emitted.with_aux(Runtime::direct_vector_helper()?),
            false => emitted,
        };
        self.native_methods
            .iter()
            .try_fold(emitted, |emitted, method| {
                Ok(emitted.with_aux(crate::core::AuxChunk::ForwardDecl(method.render()?.into())))
            })
    }

    pub fn file(&self, version: JavaVersion) -> Result<JavaFile> {
        JavaFile::parse_for(self.name.as_str(), version)
    }

    pub fn name(&self) -> &TypeIdentifier {
        &self.name
    }

    pub fn map_name(&self) -> &TypeIdentifier {
        &self.map_name
    }

    pub fn callbacks_name(&self) -> &TypeIdentifier {
        &self.callbacks_name
    }

    pub fn method(&self) -> &Method {
        &self.method
    }

    fn type_identifier(
        closure: &ClosureParameter<Native, IntoRust>,
        version: JavaVersion,
    ) -> Result<TypeIdentifier> {
        TypeIdentifier::parse(format!("Closure{}", closure.signature().as_str()), version)
    }
}

impl ClosureHandle {
    pub fn new(
        closure: &ClosureParameter<Native, IntoRust>,
        bridge: &JniBridgeContract,
        version: JavaVersion,
    ) -> Result<Self> {
        let registration = bridge
            .closures()
            .iter()
            .find(|registration| registration.signature() == closure.signature())
            .ok_or(JavaHost::broken_bridge_contract(
                "closure parameter has a JNI registration",
            ))?;
        Ok(Self {
            bridge: TypeName::named(TypeIdentifier::parse(
                registration.class().class_name(),
                version,
            )?),
            presence: closure.presence(),
        })
    }

    pub fn type_name(
        closure: &ClosureParameter<Native, IntoRust>,
        version: JavaVersion,
        package: Option<&JavaPackage>,
    ) -> Result<TypeName> {
        let name = Closure::type_identifier(closure, version)?;
        Ok(package
            .map(|package| package.type_name(name.clone()))
            .unwrap_or_else(|| TypeName::named(name)))
    }

    pub fn native_argument(&self, value: Expression) -> Result<Expression> {
        let insert = |value| {
            Expression::static_call(
                self.bridge.clone(),
                Identifier::known("insert"),
                [value].into_iter().collect::<ArgumentList>(),
            )
        };
        match self.presence {
            HandlePresence::Required => Ok(insert(value)),
            HandlePresence::Nullable => Ok(value
                .clone()
                .equal(Expression::null())
                .conditional(Expression::long(0), insert(value))),
            _ => Err(JavaHost::unsupported("closure parameter presence")),
        }
    }
}

impl Closures {
    pub fn from_declarations(
        declarations: &[RenderedDeclaration<'_, Native>],
        bridge: &JniBridgeContract,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let closures = declarations
            .iter()
            .filter(|declaration| !declaration.emitted().primary_chunk().is_empty())
            .map(RenderedDeclaration::declaration)
            .flat_map(ClosureSource::from_declaration)
            .try_fold(BTreeMap::new(), |mut closures, closure| {
                match closures.entry(closure.signature().clone()) {
                    Entry::Occupied(_) => Ok::<_, Error>(closures),
                    Entry::Vacant(entry) => {
                        let registration = bridge
                            .closures()
                            .iter()
                            .find(|registration| registration.signature() == closure.signature())
                            .ok_or(JavaHost::broken_bridge_contract(
                                "closure parameter has a JNI registration",
                            ))?;
                        entry.insert(Closure::from_parameter(
                            closure,
                            registration,
                            bridge,
                            version,
                            context,
                        )?);
                        Ok::<_, Error>(closures)
                    }
                }
            })?
            .into_values()
            .collect();
        Ok(Self { closures })
    }

    pub fn render(
        self,
        package: &JavaPackage,
        version: JavaVersion,
    ) -> Result<Vec<(JavaFile, Emitted)>> {
        self.closures
            .into_iter()
            .map(|closure| {
                let file = closure.file(version)?;
                let emitted = closure.render(package)?;
                Ok((file, emitted))
            })
            .collect()
    }
}

struct ClosureSource;

impl ClosureSource {
    fn from_declaration<'binding>(
        declaration: DeclarationRef<'binding, Native>,
    ) -> Box<dyn Iterator<Item = &'binding ClosureParameter<Native, IntoRust>> + 'binding> {
        match declaration {
            DeclarationRef::Function(function) => Self::from_callable(function.callable()),
            DeclarationRef::Class(class) => Box::new(
                class
                    .initializers()
                    .iter()
                    .flat_map(|initializer| Self::from_callable(initializer.callable()))
                    .chain(
                        class
                            .methods()
                            .iter()
                            .flat_map(|method| Self::from_callable(method.callable())),
                    ),
            ),
            _ => Box::new(std::iter::empty()),
        }
    }

    fn from_callable<'binding>(
        callable: &'binding ExportedCallable<Native>,
    ) -> Box<dyn Iterator<Item = &'binding ClosureParameter<Native, IntoRust>> + 'binding> {
        Box::new(callable.params().iter().filter_map(|parameter| {
            let IncomingParam::Closure(closure) = parameter.payload() else {
                return None;
            };
            Some(closure)
        }))
    }
}
