use std::{iter::Chain, marker::PhantomData, slice::{Iter, IterMut}};

use skylite_compress::Decoder;

use crate::{actors::{Actor, InstanceId, TypeId}, DrawContext, ProjectControls, SkyliteProject};

/// Immutable iterator over actors in a `Scene`.
pub struct ActorIterator<'scene, P: SkyliteProject> {
    inner: Chain<Iter<'scene, Box<dyn Actor<P=P>>>, Iter<'scene, Box<dyn Actor<P=P>>>>
}

impl<'scene, P: SkyliteProject> ActorIterator<'scene, P> {
    pub fn _private_new<'s>(main: &'s [Box<dyn Actor<P=P>>], extras: &'s [Box<dyn Actor<P=P>>]) -> ActorIterator<'s, P> {
        ActorIterator {
            inner: main.iter().chain(extras.iter())
        }
    }

    /// Filters the iterator to only include the actors of a particular type. The items of the
    /// returned iterator will already be converted to that actor type.
    pub fn filter_type<A: Actor>(self) -> ActorIteratorFiltered<'scene, P, A> {
        ActorIteratorFiltered {
            inner: self,
            _unused: PhantomData
        }
    }
}

impl<'scene, P: SkyliteProject> Iterator for ActorIterator<'scene, P> {
    type Item = &'scene dyn Actor<P=P>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(AsRef::as_ref)
    }
}

/// Mutable iterator over actors in a `Scene`.
pub struct ActorIteratorMut<'scene, P: SkyliteProject> {
    inner: Chain<IterMut<'scene, Box<dyn Actor<P=P>>>, IterMut<'scene, Box<dyn Actor<P=P>>>>
}

impl<'scene, P: SkyliteProject> Iterator for ActorIteratorMut<'scene, P> {
    type Item = &'scene mut dyn Actor<P=P>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.next() {
            Some(i) => Some(i.as_mut()),
            None => None
        }
    }
}

impl<'scene, P: SkyliteProject> ActorIteratorMut<'scene, P> {
    pub fn _private_new<'s>(main: &'s mut [Box<dyn Actor<P=P>>], extras: &'s mut [Box<dyn Actor<P=P>>]) -> ActorIteratorMut<'s, P> {
        ActorIteratorMut {
            inner: main.iter_mut().chain(extras.iter_mut())
        }
    }

    /// Filters the iterator to only include the actors of a particular type. The items of the
    /// returned iterator will already be converted to that actor type.
    pub fn filter_type<A: Actor>(self) -> ActorIteratorFilteredMut<'scene, P, A> {
        ActorIteratorFilteredMut {
            inner: self,
            _unused: PhantomData
        }
    }
}

pub struct ActorIteratorFiltered<'scene, P: SkyliteProject, Filter: TypeId + InstanceId> {
    inner: ActorIterator<'scene, P>,
    _unused: PhantomData<Filter>
}

impl<'scene, P: SkyliteProject, Filter: TypeId + InstanceId + 'scene> Iterator for ActorIteratorFiltered<'scene, P, Filter> {
    type Item = &'scene Filter;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(actor) = self.inner.next() {
            if actor.get_id() == <Filter as TypeId>::get_id() {
                unsafe {
                    return Some(&*(actor as *const dyn Actor<P=P> as *const Filter));
                }
            } else {
                continue;
            }
        }
        None
    }
}

pub struct ActorIteratorFilteredMut<'scene, P: SkyliteProject, Filter: Actor> {
    inner: ActorIteratorMut<'scene, P>,
    _unused: PhantomData<Filter>
}

impl<'scene, P: SkyliteProject, Filter: Actor + 'scene> Iterator for ActorIteratorFilteredMut<'scene, P, Filter> {
    type Item = &'scene mut Filter;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(actor) = self.inner.next() {
            if actor.get_id() == <Filter as TypeId>::get_id() {
                unsafe {
                    return Some(&mut *(actor as *mut dyn Actor<P=P> as *mut Filter));
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
    type ActorNames: Into<usize> where Self: Sized;

    #[doc(hidden)] fn _private_decode(decode: &mut dyn Decoder) -> Self where Self: Sized;
    #[doc(hidden)] fn _private_update(&mut self, controls: &mut ProjectControls<Self::P>);
    #[doc(hidden)] fn _private_render(&self, ctx: &mut DrawContext<Self::P>);
    #[doc(hidden)] fn _private_get_named_actor_mut_usize(&mut self, name: usize) -> &mut dyn Actor<P=Self::P>;

    /// Returns an iterator over a set of actors in the `Scene`.
    fn iter_actors(&self, which: IterActors) -> ActorIterator<Self::P>;

    /// Returns a mutable iterator over a set of actors in the `Scene`.
    fn iter_actors_mut(&mut self, which: IterActors) -> ActorIteratorMut<Self::P>;

    /// Adds an `Actor` as an extra to the `Scene`.
    fn add_extra(&mut self, extra: Box<dyn Actor<P=Self::P>>);

    /// Removes the extra that is currently being updated.
    /// Must be called from an `Actor` context, i.e. an action
    /// or one of the update hooks.
    fn remove_current_extra(&mut self);

    /// Returns a shared reference to a named actor in the `Scene`, or `None`
    /// if the name does not exist.
    fn get_named_actor(&self, name: Self::ActorNames) -> &dyn Actor<P=Self::P> where Self: Sized;

    /// Returns a mutable reference to a named actor in the `Scene`, or `None`
    /// if the name does not exist.
    fn get_named_actor_mut(&mut self, name: Self::ActorNames) -> &mut dyn Actor<P=Self::P> where Self: Sized;
}

/// Parameters for instantiating a scene.
///
/// This trait is implemented automatically for a type by `skylite_project!`. The
/// type that implements this trait will be an enum with a variant for each scene in
/// the project, with fields for each parameter for a scene.
///
/// ```ignore
/// let params = MyProjectSceneParams::MyScene { param1: 5, param2: 10 };
/// ```
pub trait SceneParams {
    type P: SkyliteProject;

    fn load(self) -> Box<dyn Scene<P=Self::P>>;
}

#[doc(hidden)]
pub mod _private {
    use std::marker::PhantomData;

    use crate::{actors::Actor, DrawContext, SkyliteProject};

    use super::{IterActors, Scene, SceneParams};

    pub fn render_scene<'scene, P: SkyliteProject>(scene: &'scene dyn Scene<P=P>, ctx: &mut DrawContext<P>) {
        let mut z_sorted: Vec<&dyn Actor<P=P>> = Vec::new();
        let mut insert_by_z_order = |actor: &'scene dyn Actor<P=P>| {
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

    /// Dummy Scene to be used as a temporary value inside replace_scene.
    struct DummyScene<P: SkyliteProject>(PhantomData<P>);

    impl<P: SkyliteProject> Scene for DummyScene<P> {
        type P = P;
        type ActorNames = usize;

        fn _private_decode(_decode: &mut dyn skylite_compress::Decoder) -> Self where Self: Sized { unimplemented!() }
        fn _private_update(&mut self, _controls: &mut crate::ProjectControls<Self::P>) { unimplemented!() }
        fn _private_render(&self, _ctx: &mut DrawContext<Self::P>) { unimplemented!() }
        fn _private_get_named_actor_mut_usize(&mut self, _name: usize) -> &mut dyn Actor<P=Self::P> { unimplemented!() }
        fn iter_actors(&self, _which: IterActors) -> super::ActorIterator<Self::P> { unimplemented!() }
        fn iter_actors_mut(&mut self, _which: IterActors) -> super::ActorIteratorMut<Self::P> { unimplemented!() }
        fn add_extra(&mut self, _extra: Box<dyn Actor<P=P>>) { unimplemented!() }
        fn remove_current_extra(&mut self) { unimplemented!() }
        fn get_named_actor(&self, _name: Self::ActorNames) -> &dyn Actor<P=Self::P> where Self: Sized { unimplemented!() }
        fn get_named_actor_mut(&mut self, _name: Self::ActorNames) -> &mut dyn Actor<P=Self::P> where Self: Sized { unimplemented!() }
    }

    /// This function ensures that the old Scene in `dst` is gone before
    /// creating the new Scene. Having two Scenes in memory at the same time
    /// should be avoided as Scenes are potentially very large.
    pub fn replace_scene<P: SkyliteProject + 'static>(src: P::SceneParams, dst: &mut Box<dyn Scene<P=P>>) {
        *dst = Box::new(DummyScene::<P>(PhantomData));
        *dst = src.load();
    }
}
