use crate::{
    bridge::jni::{
        CallbackCompletionInvoker, CallbackCompletionPayloadValue,
        CallbackMethod as JniCallbackMethod, JniBridgeContract,
    },
    core::{Error, Result},
    target::java::{
        JavaVersion,
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeIdentifier, TypeName},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Completion {
    callback: Identifier,
    context: Identifier,
    success: Identifier,
    failure: Identifier,
    error: Option<Identifier>,
    payload: Option<CallbackCompletionPayloadValue>,
}

impl Completion {
    pub fn from_method(
        method: &JniCallbackMethod,
        bridge: &JniBridgeContract,
        version: JavaVersion,
    ) -> Result<Self> {
        let completion = match method.completions().as_slice() {
            [completion] => completion.clone(),
            [] => {
                return Err(Error::BrokenBridgeContract {
                    bridge: "jni",
                    invariant: "async callback method has a completion argument",
                });
            }
            _ => {
                return Err(Error::BrokenBridgeContract {
                    bridge: "jni",
                    invariant: "async callback method has one completion argument",
                });
            }
        };
        let invoker = bridge
            .callback_completions()
            .iter()
            .find(|invoker| invoker.payload() == completion.payload())
            .ok_or(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "async callback completion has a shared JNI invoker",
            })?;
        Self::from_invoker(invoker, version)
    }

    pub fn parameters(&self) -> [super::Parameter<super::ValueType>; 2] {
        [
            super::Parameter::new(
                self.callback.clone(),
                super::ValueType::Primitive(super::Primitive::Long),
            ),
            super::Parameter::new(
                self.context.clone(),
                super::ValueType::Primitive(super::Primitive::Long),
            ),
        ]
    }

    pub fn payload(&self) -> Option<CallbackCompletionPayloadValue> {
        self.payload
    }

    pub fn success(&self, payload: Option<Expression>) -> Statement {
        self.invoke(&self.success, payload)
    }

    pub fn failure(&self) -> Statement {
        self.invoke(&self.failure, None)
    }

    pub fn error(&self, payload: Expression) -> Result<Statement> {
        self.error
            .as_ref()
            .map(|method| self.invoke(method, Some(payload)))
            .ok_or(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "fallible async callback has an error completion invoker",
            })
    }

    fn from_invoker(invoker: &CallbackCompletionInvoker, version: JavaVersion) -> Result<Self> {
        Ok(Self {
            callback: Identifier::known("callbackToken"),
            context: Identifier::known("callbackContext"),
            success: Identifier::parse_for(invoker.success_method().as_str(), version)?,
            failure: Identifier::parse_for(invoker.failure_method().as_str(), version)?,
            error: invoker
                .error_method()
                .map(|method| Identifier::parse_for(method.as_str(), version))
                .transpose()?,
            payload: invoker.payload().map(|payload| payload.value()),
        })
    }

    fn invoke(&self, method: &Identifier, payload: Option<Expression>) -> Statement {
        Statement::expression(Expression::static_call(
            TypeName::named(TypeIdentifier::known("Native", JavaVersion::JAVA_8)),
            method.clone(),
            [
                Expression::identifier(self.callback.clone()),
                Expression::identifier(self.context.clone()),
            ]
            .into_iter()
            .chain(payload)
            .collect::<ArgumentList>(),
        ))
    }
}
