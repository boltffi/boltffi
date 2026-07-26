mod completion;
mod handle;
mod method;

pub use handle::{CallbackHandle, HandleMethod};
pub use method::Method;

use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CallableDecl, CallbackDecl, CallbackId, CanonicalName, ClosureParameter as IrClosureParameter,
    DataVariantPayload, DirectValueType, Direction, ErrorChannel, ErrorPlacement, ExecutionDecl,
    ForeignBody, HandlePresence, HandleTarget, ImportedMethodDecl, IntoRust, Native, OutOfRust,
    ParamDecl, ParamPlanRender, Primitive as BindingPrimitive, ReturnPlanRender, ReturnValueSlot,
    TypeRef, VTableSlot, WritePlan, native,
};

use crate::{
    bridge::jni::{
        CallbackCompletionPayloadValue, CallbackHandleCompletion,
        CallbackHandleMethod as JniCallbackHandleMethod, CallbackMethod as JniCallbackMethod,
        ClosureRegistration, JniBridgeContract, SuccessOutArgument,
    },
    core::{Emitted, RenderContext, Result},
    target::java::{
        JavaFile, JavaHost, JavaPackage, JavaVersion,
        admission::CallbackShape,
        codec::{Reader, Runtime, WireBuffer},
        name_style::Name,
        primitive::Primitive,
        render::{
            DirectVector, Enumeration, Record,
            class::ClassHandle,
            native::Method as NativeMethod,
            signature::{ErasedSignature, Parameter, ReturnType, ValueType},
            type_name::JavaType,
        },
        syntax::{
            ArgumentList, Expression, Identifier, Javadoc, Statement, TypeIdentifier, TypeName,
        },
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/java/callback.java", escape = "none")]
struct CallbackTemplate<'callback> {
    package: &'callback JavaPackage,
    callback: &'callback Callback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Callback {
    name: TypeIdentifier,
    callbacks_name: TypeIdentifier,
    bridge_name: TypeIdentifier,
    methods: Vec<Method>,
    doc: Option<Javadoc>,
    wire_runtime: bool,
    direct_vector_runtime: bool,
    native_methods: Vec<NativeMethod>,
    handle_name: TypeIdentifier,
    handle_methods: Vec<HandleMethod>,
    handle_release: Option<Identifier>,
}

impl Callback {
    pub fn from_declaration(
        declaration: &CallbackDecl<Native>,
        bridge: &JniBridgeContract,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        CallbackShape::classify(declaration).require_supported()?;
        let registration =
            bridge
                .source_callback(declaration.id())
                .ok_or(JavaHost::broken_bridge_contract(
                    "callback declaration has no JNI registration",
                ))?;
        let methods = declaration.protocol().vtable().methods();
        if methods.len() != registration.methods().len() {
            return Err(JavaHost::broken_bridge_contract(
                "callback method count matches the JNI registration",
            ));
        }
        let name = Self::type_name(declaration, version)?;
        let methods = methods
            .iter()
            .zip(registration.methods())
            .map(|(source, method)| {
                Method::from_declaration(source, method, bridge, version, context)
            })
            .collect::<Result<Vec<_>>>()?;
        if !registration.handle_methods().is_empty()
            && registration.handle_methods().len()
                != declaration.protocol().vtable().methods().len()
        {
            return Err(JavaHost::broken_bridge_contract(
                "callback handle method count matches the callback declaration",
            ));
        }
        let handle_methods = declaration
            .protocol()
            .vtable()
            .methods()
            .iter()
            .zip(registration.handle_methods())
            .map(|(source, method)| {
                HandleMethod::from_declaration(source, method, version, context)
            })
            .collect::<Result<Vec<_>>>()?;
        let lifecycle = bridge.callback_handle_lifecycle();
        if !handle_methods.is_empty() && lifecycle.is_none() {
            return Err(JavaHost::broken_bridge_contract(
                "callback handle methods have lifecycle methods",
            ));
        }
        let native_methods = registration
            .handle_methods()
            .iter()
            .map(|method| NativeMethod::from_callback_handle_method(method, version))
            .chain(
                bridge
                    .callback_handle_lifecycle()
                    .map(|lifecycle| {
                        NativeMethod::from_callback_handle_lifecycle(lifecycle, version)
                    })
                    .transpose()?
                    .unwrap_or_default()
                    .into_iter()
                    .map(Ok),
            )
            .chain(
                bridge
                    .callback_completions()
                    .iter()
                    .map(|invoker| NativeMethod::from_callback_completion(invoker, version))
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .flatten()
                    .map(Ok),
            )
            .chain(
                bridge
                    .success_out_writers()
                    .iter()
                    .map(|writer| NativeMethod::from_success_out_writer(writer, version)),
            )
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            callbacks_name: TypeIdentifier::parse(registration.class().class_name(), version)?,
            bridge_name: TypeIdentifier::parse(format!("{name}Bridge"), version)?,
            wire_runtime: methods.iter().any(Method::requires_wire_runtime)
                || handle_methods
                    .iter()
                    .any(HandleMethod::requires_wire_runtime),
            direct_vector_runtime: methods.iter().any(Method::requires_direct_vector_runtime)
                || handle_methods
                    .iter()
                    .any(HandleMethod::requires_direct_vector_runtime),
            methods,
            handle_name: TypeIdentifier::parse(format!("{name}Handle"), version)?,
            handle_methods,
            handle_release: lifecycle
                .map(|lifecycle| {
                    Identifier::parse_for(lifecycle.release_method().as_str(), version)
                })
                .transpose()?,
            native_methods,
            doc: declaration.meta().doc().map(Javadoc::new),
            name,
        })
    }

    pub fn render(&self, package: &JavaPackage) -> Result<Emitted> {
        let emitted = Emitted::primary(
            CallbackTemplate {
                package,
                callback: self,
            }
            .render()?,
        );
        let emitted = match self.wire_runtime {
            true => emitted.with_aux(Runtime::helper()?),
            false => emitted,
        };
        let emitted = match self.direct_vector_runtime {
            true => emitted.with_aux(Runtime::direct_vector_helper()?),
            false => emitted,
        };
        let emitted = match self.methods.iter().any(Method::is_asynchronous) {
            true => emitted.with_aux(Runtime::callback_failure_helper()?),
            false => emitted,
        };
        let emitted = match self
            .handle_methods
            .iter()
            .any(HandleMethod::is_asynchronous)
        {
            true => emitted.with_aux(Runtime::callback_future_helper()?),
            false => emitted,
        };
        self.native_methods
            .iter()
            .try_fold(emitted, |emitted, method| {
                Ok(emitted.with_aux(crate::core::AuxChunk::ForwardDecl(method.render()?.into())))
            })
    }

    pub fn file_for(declaration: &CallbackDecl<Native>, version: JavaVersion) -> Result<JavaFile> {
        Self::type_name(declaration, version)
            .and_then(|name| JavaFile::parse_for(name.as_str(), version))
    }

    pub fn type_name_for(
        id: CallbackId,
        context: &RenderContext<Native>,
        version: JavaVersion,
    ) -> Result<TypeIdentifier> {
        context
            .callback(id)
            .ok_or(JavaHost::broken_bridge_contract(
                "callback handle target has no callback declaration",
            ))
            .and_then(|declaration| Self::type_name(declaration, version))
    }

    pub fn type_name(
        declaration: &CallbackDecl<Native>,
        version: JavaVersion,
    ) -> Result<TypeIdentifier> {
        Name::new(declaration.name()).type_name(version)
    }

    pub fn name(&self) -> &TypeIdentifier {
        &self.name
    }

    pub fn callbacks_name(&self) -> &TypeIdentifier {
        &self.callbacks_name
    }

    pub fn bridge_name(&self) -> &TypeIdentifier {
        &self.bridge_name
    }

    pub fn methods(&self) -> &[Method] {
        &self.methods
    }

    pub fn handle_name(&self) -> &TypeIdentifier {
        &self.handle_name
    }

    pub fn handle_methods(&self) -> &[HandleMethod] {
        &self.handle_methods
    }

    pub fn handle_release(&self) -> Option<&Identifier> {
        self.handle_release.as_ref()
    }

    pub fn signatures(&self) -> Vec<ErasedSignature> {
        // `close()` and `rawHandle()` are reserved only when the handle class is emitted.
        let reserved = match self.handle_methods.is_empty() {
            true => &[][..],
            false => &["close", "rawHandle"][..],
        };
        reserved
            .iter()
            .map(|name| ErasedSignature::new(Identifier::known(name), []))
            .chain(self.methods.iter().map(|method| {
                ErasedSignature::new(
                    method.name().clone(),
                    method
                        .public_parameters()
                        .iter()
                        .map(|parameter| parameter.ty().clone()),
                )
            }))
            .collect()
    }

    pub fn doc(&self) -> Option<&Javadoc> {
        self.doc.as_ref()
    }
}
