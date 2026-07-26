use boltffi_binding::{ClassDecl, ConstantOwner, Native};

use crate::{
    core::{Error, Result},
    target::python::{
        cpython::render::{class as class_render, function},
        syntax::Identifier,
    },
};

use super::{AssociatedCallable, ClassStream, ConstantStub, NameScope, Package};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Class {
    pub class_name: Identifier,
    pub release_method: Identifier,
    pub constants: Vec<ConstantStub>,
    pub init: Vec<AssociatedCallable>,
    pub constructors: Vec<AssociatedCallable>,
    pub static_methods: Vec<AssociatedCallable>,
    pub instance_methods: Vec<AssociatedCallable>,
    pub streams: Vec<ClassStream>,
}

impl Class {
    pub fn from_declaration(declaration: &ClassDecl<Native>, package: &Package) -> Result<Self> {
        let symbols = class_render::Symbols::new(declaration)?;
        let class_name = symbols.class_name().clone();
        let constructors = declaration
            .initializers()
            .iter()
            .filter(|initializer| function::Function::can_render(initializer.callable()))
            .map(|initializer| {
                AssociatedCallable::from_class_initializer(initializer, &symbols, package)
            })
            .collect::<Result<Vec<_>>>()?;
        let init = constructors
            .iter()
            .filter(|constructor| constructor.python_name.as_str() == "new")
            .cloned()
            .collect::<Vec<_>>();
        if init.iter().any(AssociatedCallable::is_async) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async __init__",
            });
        }
        let constructors = constructors
            .into_iter()
            .filter(|constructor| constructor.python_name.as_str() != "new")
            .collect::<Vec<_>>();
        let methods = declaration
            .methods()
            .iter()
            .filter(|method| function::Function::can_render(method.callable()))
            .map(|method| AssociatedCallable::from_class_method(method, &symbols, package))
            .collect::<Result<Vec<_>>>()?;
        let (instance_methods, static_methods): (Vec<_>, Vec<_>) =
            methods.into_iter().partition(|method| method.receiver);
        let streams = package
            .streams_for_class(declaration.id())
            .into_iter()
            .map(|stream| ClassStream::from_declaration(stream, &class_name, package))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            class_name,
            release_method: symbols.release()?,
            constants: package.constants_for_owner(ConstantOwner::Class(declaration.id()))?,
            init,
            constructors,
            static_methods,
            instance_methods,
            streams,
        })
    }
}

impl Class {
    pub fn uses_wire_helpers(&self) -> bool {
        self.callables().any(AssociatedCallable::uses_wire_helpers)
            || self.streams.iter().any(ClassStream::uses_wire_helpers)
    }

    pub fn uses_async_helpers(&self) -> bool {
        self.callables().any(AssociatedCallable::uses_async_helpers)
    }

    pub fn uses_sequence_annotations(&self) -> bool {
        self.callables()
            .any(AssociatedCallable::uses_sequence_annotations)
    }

    pub fn uses_callable_annotations(&self) -> bool {
        self.callables()
            .any(AssociatedCallable::uses_callable_annotations)
    }

    pub fn validate_names(&self) -> Result<()> {
        NameScope::new(format!("class `{}`", self.class_name))
            .insert_all(self.constants.iter().map(ConstantStub::member_name))
            .and_then(|scope| {
                scope.insert_all(self.callables().map(AssociatedCallable::member_name))
            })
            .and_then(|scope| scope.insert_all(self.streams.iter().map(ClassStream::member_name)))
            .map(|_| ())?;
        self.callables()
            .try_for_each(|callable| callable.validate_names(&self.class_name))
    }

    pub fn top_level_name(&self) -> (String, String) {
        (
            self.class_name.to_string(),
            format!("class `{}`", self.class_name),
        )
    }

    pub fn subscription_names(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.streams.iter().map(ClassStream::top_level_name)
    }

    fn callables(&self) -> impl Iterator<Item = &AssociatedCallable> {
        self.init
            .iter()
            .chain(&self.constructors)
            .chain(&self.static_methods)
            .chain(&self.instance_methods)
    }
}
