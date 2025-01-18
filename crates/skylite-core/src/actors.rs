use skylite_compress::Decoder;

use crate::{ecs::Entity, scenes::Scene, DrawContext, ProjectControls, SkyliteProject};

/// **For internal use only.**
///
/// Used to assign an id to a specific type.
pub trait TypeId {
    fn get_id() -> usize where Self: Sized;
}

/// **For internal use only.**
///
/// Implements the `get_id` function from the [`TypeId`]
/// trait for instances of a type via a blanket implementation.
pub trait InstanceId {
    fn get_id(&self) -> usize;
}

impl<T: TypeId> InstanceId for T {
    fn get_id(&self) -> usize {
        <T as TypeId>::get_id()
    }
}

pub trait ActorAction {
    #[doc(hidden)] fn _private_decode(decoder: &mut dyn Decoder) -> Self;
}

/// An `Actor` is any entity in a [`Scene`].
///
/// *This trait is implemented by generated code and should not
/// be implemented manually.*
///
/// An actor can have properties and perform actions. Each action is defined
/// by its own dedicated update method, which is called exactly once per `Scene`
/// update (and, by extension, once per project update). An actor must perform
/// exactly one action at a time.
pub trait Actor: TypeId + InstanceId {
    type P: SkyliteProject;
    type Action: ActorAction
        where Self: Sized;

    #[doc(hidden)] fn _private_decode(decoder: &mut dyn Decoder) -> Self
        where Self: Sized;

    #[doc(hidden)] fn _private_update(&mut self, scene: &mut dyn Scene<P=Self::P>, controls: &mut ProjectControls<Self::P>);
    #[doc(hidden)] fn _private_render(&self, ctx: &mut DrawContext<Self::P>);

    fn set_action(&mut self, action: Self::Action)
        where Self: Sized;

    /// Returns a reference to the underlying entity for this actor.
    fn get_entity(&self) -> &Entity;

    /// Returns a mutable reference to the underlying entity for this actor.
    fn get_entity_mut(&mut self) -> &mut Entity;

    /// Returns the z-order of the actor.
    ///
    /// The z-order determines the order in which actors
    /// are rendered. Actors with higher z-orders are drawn
    /// on top of actors with lower z-order. Actors with the
    /// same z-order are drawn in an unspecified but consistent
    /// order, which should avoid "z-fighting".
    ///
    /// The default Z-order is `1`.
    fn z_order(&self) -> i16 {
        1
    }
}
