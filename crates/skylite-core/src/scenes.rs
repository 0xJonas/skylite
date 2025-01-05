use std::{iter::Chain, marker::PhantomData, slice::{Iter, IterMut}};

use skylite_compress::Decoder;

use crate::{actors::{Actor, AnyActor, TypeId}, DrawContext, ProjectControls, SkyliteProject};

/// Immutable iterator over actors in a `Scene`.
pub struct ActorIterator<'scene, Type: AnyActor> {
    inner: Chain<Iter<'scene, Type>, Iter<'scene, Type>>
}

impl<'scene, Type: AnyActor> ActorIterator<'scene, Type> {
    pub fn _private_new<'s>(main: &'s [Type], extras: &'s [Type]) -> ActorIterator<'s, Type> {
        ActorIterator {
            inner: main.iter().chain(extras.iter())
        }
    }

    /// Filters the iterator to only include the actors of a particular type. The items of the
    /// returned iterator will already be converted to that actor type.
    pub fn filter_type<A: Actor>(self) -> ActorIteratorFiltered<'scene, Type, A> {
        ActorIteratorFiltered {
            inner: self,
            _unused: PhantomData
        }
    }
}

impl<'scene, Type: AnyActor> Iterator for ActorIterator<'scene, Type> {
    type Item = &'scene Type;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

/// Mutable iterator over actors in a `Scene`.
pub struct ActorIteratorMut<'scene, Type: AnyActor> {
    inner: Chain<IterMut<'scene, Type>, IterMut<'scene, Type>>
}

impl<'scene, Type: AnyActor> Iterator for ActorIteratorMut<'scene, Type> {
    type Item = &'scene mut Type;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<'scene, Type: AnyActor> ActorIteratorMut<'scene, Type> {
    pub fn _private_new<'s>(main: &'s mut [Type], extras: &'s mut [Type]) -> ActorIteratorMut<'s, Type> {
        ActorIteratorMut {
            inner: main.iter_mut().chain(extras.iter_mut())
        }
    }

    /// Filters the iterator to only include the actors of a particular type. The items of the
    /// returned iterator will already be converted to that actor type.
    pub fn filter_type<A: Actor>(self) -> ActorIteratorFilteredMut<'scene, Type, A> {
        ActorIteratorFilteredMut {
            inner: self,
            _unused: PhantomData
        }
    }
}

pub struct ActorIteratorFiltered<'scene, Type: AnyActor, Filter: Actor> {
    inner: ActorIterator<'scene, Type>,
    _unused: PhantomData<Filter>
}

impl<'scene, Type: AnyActor, Filter: Actor + 'scene> Iterator for ActorIteratorFiltered<'scene, Type, Filter> {
    type Item = &'scene Filter;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(actor) = self.inner.next() {
            if actor.get_id() == <Filter as TypeId>::get_id() {
                unsafe {
                    return Some(actor._private_transmute());
                }
            } else {
                continue;
            }
        }
        None
    }
}

pub struct ActorIteratorFilteredMut<'scene, Type: AnyActor, Filter: Actor> {
    inner: ActorIteratorMut<'scene, Type>,
    _unused: PhantomData<Filter>
}

impl<'scene, Type: AnyActor, Filter: Actor + 'scene> Iterator for ActorIteratorFilteredMut<'scene, Type, Filter> {
    type Item = &'scene mut Filter;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(actor) = self.inner.next() {
            if actor.get_id() == <Filter as TypeId>::get_id() {
                unsafe {
                    return Some(actor._private_transmute_mut());
                }
            } else {
                continue;
            }
        }
        None
    }
}

/// Parameter to `iter_actors` and `iter_actors_mut` to select which actors the
/// iterator should cover.
pub enum IterActors {
    /// Only iterate over named actors.
    Named,

    /// Only iterate over extras
    Extra,

    /// Iterate first over the named actors, and then over the extras.
    All
}

/// A `Scene` is a single screen or context of a project, e.g. an individual level or menu.
/// There are two lists of [`Actors`][Actor] which make up a `Scene`:
/// - The main actors, or just 'actors' are fixed for each scene. These are the actors which
///   are accessible to plays.
/// - The extra actors, or just 'extras', which can be added to or removed during a scene's lifetime.
///   These actors are not accessible to plays.
///
/// A `Scene` will update each actor when the `Scene` itself is updated, and render each actor when it
/// is rendered itself.
pub trait Scene {
    type P: SkyliteProject;

    #[doc(hidden)] fn _private_decode(decode: &mut dyn Decoder) -> Self where Self: Sized;
    #[doc(hidden)] fn _private_update(&mut self, controls: &mut ProjectControls<Self::P>);
    #[doc(hidden)] fn _private_render(&self, ctx: &mut DrawContext<Self::P>);

    /// Returns an iterator over all the actors in the scene.
    fn iter_actors(&self, which: IterActors) -> ActorIterator<<Self::P as SkyliteProject>::Actors>;

    /// Returns a mutable iterator over all the actors in the scene.
    fn iter_actors_mut(&mut self, which: IterActors) -> ActorIteratorMut<<Self::P as SkyliteProject>::Actors>;

    /// Adds an `Actor` as an extra to the `Scene`.
    fn add_extra(&mut self, extra: <Self::P as SkyliteProject>::Actors);

    /// Removes the extra that is currently being updated.
    /// Must be called from an `Actor` context, i.e. an action
    /// or one of the update hooks.
    fn remove_current_extra(&mut self);
}

#[doc(hidden)]
pub mod _private {
    use crate::{actors::ActorBase, DrawContext, SkyliteProject};

    use super::{IterActors, Scene};

    pub fn render_scene<'scene, P: SkyliteProject>(scene: &'scene dyn Scene<P=P>, ctx: &mut DrawContext<P>) {
        let mut z_sorted: Vec<&P::Actors> = Vec::new();
        let mut insert_by_z_order = |actor: &'scene P::Actors| {
            for (i, a) in z_sorted.iter().enumerate() {
                if actor.z_order() <= a.z_order() {
                    z_sorted.insert(i, actor);
                    return;
                }
            }
            z_sorted.push(actor);
        };

        scene.iter_actors(IterActors::All).for_each(&mut insert_by_z_order);

        z_sorted.iter().for_each(|a| a._private_render(ctx));
    }
}
