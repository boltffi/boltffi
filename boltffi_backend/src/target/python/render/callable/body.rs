use boltffi_binding::{
    ErrorChannel, ErrorPlacement, ExecutionDecl, ExportedCallable, Native, native,
};

use crate::{
    core::{Error, Result},
    target::python::{
        codec::AdapterKey,
        syntax::{CallExpression, Expression, Identifier, Statement},
    },
};

use super::{future::NativeFutureMethods, parameter::Parameters, return_value::ReturnedValue};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallableBody {
    asynchronous: bool,
    lines: Vec<Statement>,
}

impl CallableBody {
    pub fn from_callable(
        callable: &ExportedCallable<Native>,
        native_name: &Identifier,
        native_call: Expression,
        returned: &ReturnedValue,
    ) -> Result<Self> {
        let error_decoder = ErrorDecoder::from_callable(callable)?;
        match callable.execution() {
            ExecutionDecl::Synchronous(_) => Self::sync(native_call, returned, error_decoder),
            ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle { .. }) => {
                Self::native_future(native_name, native_call, returned, error_decoder)
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown async callable",
            }),
        }
    }
}

impl CallableBody {
    pub fn is_async(&self) -> bool {
        self.asynchronous
    }

    pub fn uses_async_helpers(&self) -> bool {
        self.asynchronous
    }

    pub fn into_lines(self, parameters: &Parameters) -> Vec<Statement> {
        parameters.initializations().chain(self.lines).collect()
    }

    fn sync(
        native_call: Expression,
        returned: &ReturnedValue,
        error_decoder: ErrorDecoder,
    ) -> Result<Self> {
        let native_call = error_decoder.sync_call(native_call)?;
        Ok(Self {
            asynchronous: false,
            lines: vec![returned.statement(native_call)?],
        })
    }

    fn native_future(
        native_name: &Identifier,
        native_call: Expression,
        returned: &ReturnedValue,
        error_decoder: ErrorDecoder,
    ) -> Result<Self> {
        let methods = NativeFutureMethods::new(native_name.clone())?;
        let future = Identifier::parse("__boltffi_future")?;
        let native_module = Expression::identifier(Identifier::parse("_native")?);
        let constructor = Expression::identifier(Identifier::parse("_BoltFfiNativeFuture")?);
        let future_call = CallExpression::new(constructor)
            .keyword(Identifier::parse("handle")?, native_call)
            .keyword(
                Identifier::parse("poll")?,
                Expression::attribute(native_module.clone(), methods.poll().clone()),
            )
            .keyword(
                Identifier::parse("complete")?,
                Expression::attribute(native_module.clone(), methods.complete().clone()),
            )
            .keyword(
                Identifier::parse("cancel")?,
                Expression::attribute(native_module.clone(), methods.cancel().clone()),
            )
            .keyword(
                Identifier::parse("free")?,
                Expression::attribute(native_module.clone(), methods.free().clone()),
            )
            .keyword(
                Identifier::parse("panic_message")?,
                Expression::attribute(native_module, methods.panic_message().clone()),
            );
        let future_call = error_decoder.apply(future_call)?;
        let wait_call = Expression::call(CallExpression::new(Expression::attribute(
            Expression::identifier(future.clone()),
            Identifier::parse("wait")?,
        )));
        Ok(Self {
            asynchronous: true,
            lines: Statement::assign_call(future, future_call)
                .into_iter()
                .chain(returned.awaited_statement(wait_call)?)
                .collect(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ErrorDecoder {
    None,
    Encoded(Identifier),
}

impl ErrorDecoder {
    fn from_callable(callable: &ExportedCallable<Native>) -> Result<Self> {
        match callable.error().channel() {
            ErrorChannel::None => Ok(Self::None),
            ErrorChannel::Encoded {
                placement: ErrorPlacement::ReturnSlot,
                shape: native::BufferShape::Buffer,
                codec,
                ..
            } => Ok(Self::Encoded(AdapterKey::read(codec).python_function()?)),
            ErrorChannel::Encoded {
                placement: ErrorPlacement::ReturnSlot,
                ..
            } => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible async error buffer shape",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible async callable",
            }),
        }
    }

    fn apply(&self, call: CallExpression) -> Result<CallExpression> {
        match self {
            Self::None => Ok(call),
            Self::Encoded(decoder) => Ok(call.keyword(
                Identifier::parse("error_decoder")?,
                Expression::identifier(decoder.clone()),
            )),
        }
    }

    fn sync_call(&self, native_call: Expression) -> Result<Expression> {
        match self {
            Self::None => Ok(native_call),
            Self::Encoded(decoder) => Ok(Expression::call(
                CallExpression::new(Expression::identifier(Identifier::parse("_boltffi_call")?))
                    .positional(Expression::identifier(decoder.clone()))
                    .positional(Expression::no_arg_lambda(native_call)),
            )),
        }
    }
}
