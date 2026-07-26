//! Type-level markers for boundary direction and callable scope.
//!
//! A boundary crossing has a direction. The parameter and return plans
//! ([`ParamPlan`], [`ReturnPlan`]) take a [`Direction`] type parameter and
//! describe one value crossing in that direction. A callable has a
//! scope. [`CallableDecl`] takes a [`CallableScope`] type parameter,
//! and the scope's `ParamDirection` and `ReturnDirection` fix the
//! directions of the params, the return, and the error channel.
//!
//! [`ParamPlan`]: crate::ParamPlan
//! [`ReturnPlan`]: crate::ReturnPlan
//! [`CallableDecl`]: crate::CallableDecl

use std::{collections::BTreeSet, fmt::Debug};

use crate::{
    BuiltinType, ClosureRegistrationIntrospect, CodecNode, DeclarationId, ReadPlan, Receive,
    Surface, ValueRef, WritePlan,
};

/// Marker for data flowing from foreign code into Rust.
///
/// Used as the type parameter of plans that describe parameters of a
/// Rust-exported callable and returns of a foreign-implemented
/// callback. Foreign code encodes the value; Rust decodes it. Encoded
/// crossings in this direction store a [`WritePlan`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IntoRust {}

/// Marker for data flowing from Rust out to foreign code.
///
/// Used as the type parameter of plans that describe returns of a
/// Rust-exported callable and parameters of a foreign-implemented
/// callback (where Rust supplies the args at invocation time). Rust
/// encodes the value; foreign decodes it. Encoded crossings in this
/// direction store a [`ReadPlan`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum OutOfRust {}

/// The direction of a single boundary crossing.
///
/// Implemented by [`IntoRust`] and [`OutOfRust`] only. The associated
/// types name the parts of the IR that depend on direction: the codec
/// orientation, the Rust-side receive mode, and the opposite direction
/// used at scope boundaries.
#[allow(private_bounds)]
pub trait Direction:
    sealed::DirectionSealed
    + 'static
    + Copy
    + Clone
    + Debug
    + Eq
    + std::hash::Hash
    + PartialEq
    + serde::Serialize
    + for<'de> serde::Deserialize<'de>
{
    /// The codec the IR stores at an encoded crossing in this
    /// direction.
    ///
    /// [`WritePlan`] for [`IntoRust`], where foreign code encodes the
    /// value. [`ReadPlan`] for [`OutOfRust`], where foreign code
    /// decodes the value. Renderers consume whichever side they own.
    type Codec: Clone
        + Debug
        + Eq
        + std::hash::Hash
        + PartialEq
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>;
    /// The Rust-side receive mode of a slot in this direction.
    ///
    /// [`Receive`] for [`IntoRust`], naming whether the Rust function
    /// takes the value by value, by shared reference, or by mutable
    /// reference. `()` for [`OutOfRust`], where the slot does not
    /// reach Rust as a binding.
    type Receive: Copy
        + Debug
        + Eq
        + std::hash::Hash
        + PartialEq
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>;
    /// Surface registration shape for a closure crossing in this
    /// direction.
    type ClosureRegistrationShape<S: Surface>: Clone
        + Debug
        + Eq
        + std::hash::Hash
        + PartialEq
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>
        + ClosureRegistrationIntrospect;
    /// The opposite direction.
    ///
    /// `IntoRust::Opposite = OutOfRust` and `OutOfRust::Opposite =
    /// IntoRust`. Used when crossing a callable scope boundary inverts
    /// the data flow.
    type Opposite: Direction<Opposite = Self>;
    /// The callable scope of a closure whose handle crosses in this
    /// direction.
    ///
    /// A closure that crosses [`IntoRust`] arrived from foreign code,
    /// so its body lives on the foreign side and the invoke contract is
    /// [`ForeignBody`] (`InvokeScope::ParamDirection = OutOfRust`,
    /// `InvokeScope::ReturnDirection = IntoRust`). A closure that
    /// crosses [`OutOfRust`] was created by Rust, so its body lives on
    /// the Rust side and the invoke contract is [`RustBody`]
    /// (`InvokeScope::ParamDirection = IntoRust`,
    /// `InvokeScope::ReturnDirection = OutOfRust`).
    type InvokeScope: CallableScope<ParamDirection = Self::Opposite, ReturnDirection = Self>;

    /// Constructs the direction's codec wrapper from a value reference
    /// and a codec tree.
    ///
    /// [`IntoRust`] returns `WritePlan::new(value, root)`, binding the
    /// codec tree to the named value. [`OutOfRust`] returns
    /// `ReadPlan::new(root)` and discards `value`; the read produces
    /// the value from the wire so there is nothing to bind.
    fn make_codec(value: ValueRef, root: CodecNode) -> Self::Codec;

    /// Returns whether a direction-specific codec includes a result container.
    fn codec_uses_result(codec: &Self::Codec) -> bool;

    /// Returns whether a direction-specific codec includes the given builtin value.
    fn codec_uses_builtin(codec: &Self::Codec, kind: BuiltinType) -> bool;

    /// Appends declarations referenced by a direction-specific codec.
    fn codec_append_referenced_declarations(
        codec: &Self::Codec,
        references: &mut BTreeSet<DeclarationId>,
    );

    /// Returns whether a direction-specific codec references a declaration.
    fn codec_references_declaration(codec: &Self::Codec, declaration: DeclarationId) -> bool {
        let mut references = BTreeSet::new();
        Self::codec_append_referenced_declarations(codec, &mut references);
        references.contains(&declaration)
    }

    /// Returns whether a direction-specific codec includes a tagged interned string.
    fn codec_uses_interned_string(codec: &Self::Codec) -> bool;

    /// Projects a `Receive` value into the direction's receive type.
    ///
    /// [`IntoRust`] returns the value unchanged. [`OutOfRust`] discards
    /// it and returns `()`.
    fn receive_from(receive: Receive) -> Self::Receive;
}

impl Direction for IntoRust {
    type Codec = WritePlan;
    type Receive = Receive;
    type ClosureRegistrationShape<S: Surface> = S::IncomingClosureRegistration;
    type Opposite = OutOfRust;
    type InvokeScope = ForeignBody;

    fn make_codec(value: ValueRef, root: CodecNode) -> WritePlan {
        WritePlan::new(value, root)
    }

    fn codec_uses_result(codec: &WritePlan) -> bool {
        codec.uses_result()
    }

    fn codec_uses_builtin(codec: &WritePlan, kind: BuiltinType) -> bool {
        codec.uses_builtin(kind)
    }

    fn codec_append_referenced_declarations(
        codec: &WritePlan,
        references: &mut BTreeSet<DeclarationId>,
    ) {
        codec.append_referenced_declarations(references);
    }

    fn codec_uses_interned_string(codec: &WritePlan) -> bool {
        codec.uses_interned_string()
    }

    fn receive_from(receive: Receive) -> Receive {
        receive
    }
}

impl Direction for OutOfRust {
    type Codec = ReadPlan;
    type Receive = ();
    type ClosureRegistrationShape<S: Surface> = S::OutgoingClosureRegistration;
    type Opposite = IntoRust;
    type InvokeScope = RustBody;

    fn make_codec(_value: ValueRef, root: CodecNode) -> ReadPlan {
        ReadPlan::new(root)
    }

    fn codec_uses_result(codec: &ReadPlan) -> bool {
        codec.uses_result()
    }

    fn codec_uses_builtin(codec: &ReadPlan, kind: BuiltinType) -> bool {
        codec.uses_builtin(kind)
    }

    fn codec_append_referenced_declarations(
        codec: &ReadPlan,
        references: &mut BTreeSet<DeclarationId>,
    ) {
        codec.append_referenced_declarations(references);
    }

    fn codec_uses_interned_string(codec: &ReadPlan) -> bool {
        codec.uses_interned_string()
    }

    fn receive_from(_receive: Receive) {}
}

/// Marker for a callable whose body is implemented in Rust.
///
/// Foreign code calls in. Parameters flow [`IntoRust`] and returns
/// flow [`OutOfRust`]. Used by free functions, record / enum / class
/// methods, and initializers.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RustBody {}

/// Marker for a callable whose body is implemented in foreign code.
///
/// Rust calls out. Parameters flow [`OutOfRust`] (Rust pushes args to
/// the foreign implementation) and returns flow [`IntoRust`] (foreign
/// returns back to Rust). Used by callback trait methods and inline
/// closure invocations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ForeignBody {}

/// The body scope of a callable.
///
/// Implemented by [`RustBody`] and [`ForeignBody`] only. The two
/// scopes are duals of each other: the parameter direction of one is
/// the return direction of the other, and vice versa. `CallableDecl<S,
/// K>` reads `K::ParamDirection` and `K::ReturnDirection` to pick the
/// wire directions of every contained plan.
#[allow(private_bounds)]
pub trait CallableScope:
    sealed::CallableScopeSealed
    + 'static
    + Copy
    + Clone
    + Debug
    + Eq
    + std::hash::Hash
    + PartialEq
    + serde::Serialize
    + for<'de> serde::Deserialize<'de>
{
    /// The direction parameters flow in when this scope's callable
    /// runs.
    type ParamDirection: Direction<Opposite = Self::ReturnDirection>;
    /// The direction the return value and the error channel flow in
    /// when this scope's callable runs.
    type ReturnDirection: Direction<Opposite = Self::ParamDirection>;
    /// The opposite scope.
    ///
    /// Its `ParamDirection` is this scope's `ReturnDirection` and vice
    /// versa.
    type Opposite: CallableScope<
            ParamDirection = Self::ReturnDirection,
            ReturnDirection = Self::ParamDirection,
            Opposite = Self,
        >;
}

impl CallableScope for RustBody {
    type ParamDirection = IntoRust;
    type ReturnDirection = OutOfRust;
    type Opposite = ForeignBody;
}

impl CallableScope for ForeignBody {
    type ParamDirection = OutOfRust;
    type ReturnDirection = IntoRust;
    type Opposite = RustBody;
}

mod sealed {
    pub trait DirectionSealed {}
    impl DirectionSealed for super::IntoRust {}
    impl DirectionSealed for super::OutOfRust {}

    pub trait CallableScopeSealed {}
    impl CallableScopeSealed for super::RustBody {}
    impl CallableScopeSealed for super::ForeignBody {}
}
