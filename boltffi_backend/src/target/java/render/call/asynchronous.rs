use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsyncCall {
    create_body: Vec<Statement>,
    poll: Expression,
    complete: Vec<Statement>,
    cancel: Expression,
    free_body: Vec<Statement>,
    native_methods: Vec<Method>,
}

pub struct BoundArguments<'call> {
    receiver: Option<&'call Receiver>,
    parameters: &'call [BoundParameter],
}

impl<'call> BoundArguments<'call> {
    pub fn new(receiver: Option<&'call Receiver>, parameters: &'call [BoundParameter]) -> Self {
        Self {
            receiver,
            parameters,
        }
    }
}

impl AsyncCall {
    pub fn new(
        protocol: &native::AsyncProtocol,
        start: Method,
        create: Expression,
        returned: &CallReturn,
        error: &ErrorConversion,
        arguments: BoundArguments<'_>,
        scope: CallScope<'_, '_>,
    ) -> Result<Self> {
        let native::AsyncProtocol::PollHandle {
            poll,
            complete,
            cancel,
            free,
            panic_message,
            ..
        } = protocol
        else {
            return Err(JavaHost::unsupported("asynchronous function protocol"));
        };
        if arguments
            .receiver
            .and_then(|receiver| receiver.mutation.as_ref())
            .is_some()
        {
            return Err(JavaHost::unsupported(
                "mutable receiver with async execution",
            ));
        }
        start.validate_return(&ReturnType::Value(ValueType::Primitive(Primitive::Long)))?;
        let poll = Method::from_symbol(poll, scope.bridge, scope.version)?;
        let complete = Method::from_symbol(complete, scope.bridge, scope.version)?;
        let cancel = Method::from_symbol(cancel, scope.bridge, scope.version)?;
        let free = Method::from_symbol(free, scope.bridge, scope.version)?;
        let panic_message = Method::from_symbol(panic_message, scope.bridge, scope.version)?;
        complete.validate_return(&returned.native)?;
        cancel.validate_return(&ReturnType::Void)?;
        free.validate_return(&ReturnType::Void)?;
        panic_message.validate_return(&ReturnType::Value(ValueType::Reference(
            TypeName::array(TypeName::primitive(Primitive::Byte)),
        )))?;
        let future = Expression::identifier(Identifier::known("future"));
        let continuation = Expression::identifier(Identifier::known("continuation"));
        let complete_call = complete.call(scope.native_owner, [future.clone()])?;
        let mut complete_body = error.clone().wrap(
            returned.statements(complete_call, scope.version, scope.context, scope.package)?,
            scope.version,
            scope.context,
            scope.package,
            scope.return_context,
        )?;
        if matches!(returned.ty, ReturnType::Void) {
            complete_body.push(Statement::return_value(Expression::null()));
        }
        let failure = Identifier::known("__boltffi_failure");
        let panic_call = panic_message.call(scope.native_owner, [future.clone()])?;
        let failure_call = Expression::static_call(
            TypeName::named(TypeIdentifier::known("BoltFfiAsync", scope.version)),
            Identifier::known("failure"),
            [
                Expression::identifier(failure.clone()),
                Expression::lambda([], panic_call),
            ]
            .into_iter()
            .collect(),
        );
        let complete_body = vec![Statement::try_catch(
            complete_body,
            TypeName::named(TypeIdentifier::known("Throwable", scope.version)),
            failure,
            vec![Statement::throw_value(failure_call)],
        )];
        let free_body = std::iter::once(Statement::expression(
            free.call(scope.native_owner, [future.clone()])?,
        ))
        .chain(
            arguments
                .receiver
                .iter()
                .flat_map(|receiver| receiver.native.cleanup.iter().cloned()),
        )
        .collect();
        Ok(Self {
            create_body: guarded_create_body(
                arguments.receiver,
                arguments.parameters,
                vec![Statement::return_value(create)],
                scope.version,
            ),
            poll: poll.call(scope.native_owner, [future.clone(), continuation])?,
            complete: complete_body,
            cancel: cancel.call(scope.native_owner, [future])?,
            free_body,
            native_methods: vec![start, poll, complete, cancel, free, panic_message],
        })
    }

    pub fn create_body(&self) -> &[Statement] {
        &self.create_body
    }

    pub fn poll(&self) -> &Expression {
        &self.poll
    }

    pub fn complete(&self) -> &[Statement] {
        &self.complete
    }

    pub fn cancel(&self) -> &Expression {
        &self.cancel
    }

    pub fn free_body(&self) -> &[Statement] {
        &self.free_body
    }

    pub fn native_methods(&self) -> &[Method] {
        &self.native_methods
    }
}
