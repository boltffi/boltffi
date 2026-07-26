mod slot;

pub use slot::CallbackSlot;

use boltffi_binding::{CallbackDecl, CallbackId, CanonicalName, DeclarationId, Native};

use crate::core::Result;

use super::{Field, Function, Identifier, Parameter, Record, Type, names::Names};

/// A native callback vtable declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Callback {
    id: CallbackId,
    name: CanonicalName,
    vtable: Record,
    methods: Vec<CallbackSlot>,
    register: Function,
    create_handle: Function,
}

impl Callback {
    /// Returns the source callback trait id.
    pub const fn id(&self) -> CallbackId {
        self.id
    }

    /// Returns the canonical callback trait name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the callback vtable record.
    pub fn vtable(&self) -> &Record {
        &self.vtable
    }

    /// Returns callback method slots after `free` and `clone`.
    pub fn methods(&self) -> &[CallbackSlot] {
        &self.methods
    }

    /// Returns the callback registration function.
    pub fn register(&self) -> &Function {
        &self.register
    }

    /// Returns the callback handle constructor.
    pub fn create_handle(&self) -> &Function {
        &self.create_handle
    }
}

impl Callback {
    /// Creates the C callback declaration for a lowered callback trait.
    pub fn from_decl(callback: &CallbackDecl<Native>, names: &Names) -> Result<Self> {
        let declaration = DeclarationId::Callback(callback.id());
        let vtable_name = Identifier::parse(format!("{}VTable", names.callback(callback.id())?))?;
        let vtable = callback.protocol().vtable();
        let free = Field::new(
            vtable.free_slot().as_str(),
            Type::FunctionPointer {
                returns: Box::new(Type::Void),
                params: vec![Type::Uint64],
            },
        )?;
        let clone = Field::new(
            vtable.clone_slot().as_str(),
            Type::FunctionPointer {
                returns: Box::new(Type::Uint64),
                params: vec![Type::Uint64],
            },
        )?;
        let methods = vtable
            .methods()
            .iter()
            .map(|method| CallbackSlot::from_method(method, names))
            .collect::<Result<Vec<_>>>()?;
        let vtable = Record::new(
            vtable_name.clone(),
            [free, clone]
                .into_iter()
                .chain(methods.iter().map(CallbackSlot::field))
                .collect(),
        );
        let register = Function::exported(
            declaration,
            callback.protocol().register(),
            vec![Parameter::new(
                "vtable",
                Type::ConstPointer(Box::new(Type::Named(vtable_name.clone()))),
            )?],
            Type::Void,
        )?;
        let create_handle = Function::exported(
            declaration,
            callback.protocol().create_handle(),
            vec![Parameter::new("handle", Type::Uint64)?],
            Type::CallbackHandle(callback.id()),
        )?;
        Ok(Self {
            id: callback.id(),
            name: callback.name().clone(),
            vtable,
            methods,
            register,
            create_handle,
        })
    }
}
