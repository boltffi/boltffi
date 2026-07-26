use crate::{
    Bindings, ClosureParameter, ClosureReturn, ClosureSignature, Decl, ExportedCallable,
    ImportSymbol, ImportedCallable, IncomingParam, IntoRust, OutgoingParam, ReturnPlan, Wasm32,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// A foreign closure implementation Rust may invoke through Wasm imports.
pub enum WasmIncomingClosure<'bindings> {
    /// A closure passed into a Rust callable.
    Parameter(&'bindings ClosureParameter<Wasm32, IntoRust>),
    /// A closure returned by a foreign callable.
    Return(&'bindings ClosureReturn<Wasm32, IntoRust>),
}

impl<'bindings> WasmIncomingClosure<'bindings> {
    /// Returns the stable closure signature.
    pub fn signature(self) -> &'bindings ClosureSignature {
        match self {
            Self::Parameter(closure) => closure.signature(),
            Self::Return(closure) => closure.signature(),
        }
    }

    /// Returns the Wasm registration shape.
    pub fn registration(self) -> &'bindings crate::wasm32::IncomingClosureRegistration {
        match self {
            Self::Parameter(closure) => closure.registration().shape(),
            Self::Return(closure) => closure.registration().shape(),
        }
    }

    /// Returns the closure invocation contract.
    pub fn invoke(self) -> &'bindings ImportedCallable<Wasm32> {
        match self {
            Self::Parameter(closure) => closure.invoke(),
            Self::Return(closure) => closure.invoke(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// The imports required to instantiate one Wasm binding contract.
pub struct WasmImports<'bindings> {
    symbols: Vec<&'bindings ImportSymbol>,
    closures: Vec<WasmIncomingClosure<'bindings>>,
}

impl<'bindings> WasmImports<'bindings> {
    /// Collects every callback and closure import in declaration order.
    pub fn from_bindings(bindings: &'bindings Bindings<Wasm32>) -> Self {
        bindings
            .decls()
            .iter()
            .fold(Self::default(), |mut imports, declaration| {
                declaration
                    .exported_callables()
                    .for_each(|callable| imports.insert_exported(callable));
                declaration
                    .imported_callables()
                    .for_each(|callable| imports.insert_imported(callable));
                if let Decl::Callback(callback) = declaration {
                    let protocol = callback.protocol();
                    imports.insert(protocol.free());
                    imports.insert(protocol.clone_import());
                    protocol
                        .methods()
                        .iter()
                        .for_each(|method| imports.insert(method.target()));
                }
                imports
            })
    }

    /// Returns the imports in first-use order.
    pub fn iter(&self) -> impl Iterator<Item = &'bindings ImportSymbol> + '_ {
        self.symbols.iter().copied()
    }

    /// Returns foreign closure crossings in first-use order.
    pub fn closures(&self) -> impl Iterator<Item = WasmIncomingClosure<'bindings>> + '_ {
        self.closures.iter().copied()
    }

    fn insert_exported(&mut self, callable: &'bindings ExportedCallable<Wasm32>) {
        callable.params().iter().for_each(|parameter| {
            if let IncomingParam::Closure(closure) = parameter.payload() {
                self.insert_closure(WasmIncomingClosure::Parameter(closure));
                let registration = closure.registration().shape();
                self.insert(registration.call());
                self.insert(registration.free());
                self.insert_imported(closure.invoke());
            }
        });
        if let ReturnPlan::ClosureViaOutPointer(closure) = callable.returns().plan() {
            self.insert_exported(closure.invoke());
        }
    }

    fn insert_imported(&mut self, callable: &'bindings ImportedCallable<Wasm32>) {
        callable.params().iter().for_each(|parameter| {
            if let OutgoingParam::Closure(closure) = parameter.payload() {
                self.insert_exported(closure.invoke());
            }
        });
        if let ReturnPlan::ClosureViaOutPointer(closure) = callable.returns().plan() {
            let registration = closure.registration().shape();
            self.insert_closure(WasmIncomingClosure::Return(closure));
            self.insert(registration.call());
            self.insert(registration.free());
            self.insert_imported(closure.invoke());
        }
    }

    fn insert(&mut self, symbol: &'bindings ImportSymbol) {
        if !self.symbols.iter().any(|existing| {
            existing.module().as_str() == symbol.module().as_str()
                && existing.name().as_str() == symbol.name().as_str()
        }) {
            self.symbols.push(symbol);
        }
    }

    fn insert_closure(&mut self, closure: WasmIncomingClosure<'bindings>) {
        if !self
            .closures
            .iter()
            .any(|existing| existing.signature() == closure.signature())
        {
            self.closures.push(closure);
        }
    }
}
