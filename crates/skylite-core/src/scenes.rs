use std::cell::RefMut;

use skylite_compress::Decoder;

use crate::{actors::{Actor, AnyActor, InstanceId, TypeId}, DrawContext, ProjectControls, SkyliteProject};

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
    #[doc(hidden)] fn _private_render(&self, ctx: DrawContext<Self::P>);
    #[doc(hidden)] fn _private_actors(&self) -> RefMut<'_, Vec<<Self::P as SkyliteProject>::Actors>>;
    #[doc(hidden)] fn _private_take_actors(&mut self) -> Vec<<Self::P as SkyliteProject>::Actors>;
    #[doc(hidden)] fn _private_restore_actors(&mut self, actors: Vec<<Self::P as SkyliteProject>::Actors>);
    #[doc(hidden)] fn _private_extras(&mut self) -> &mut Vec<<Self::P as SkyliteProject>::Actors>;
    #[doc(hidden)] fn _private_take_extras(&mut self) -> Vec<<Self::P as SkyliteProject>::Actors>;
    #[doc(hidden)] fn _private_restore_extras(&mut self, extras: Vec<<Self::P as SkyliteProject>::Actors>);

    /// Returns the main actors of a `Scene`. The list of main actors
    /// fixed by the scene definition and cannot be modified.
    fn get_actors(&self) -> &[<Self::P as SkyliteProject>::Actors];

    /// Returns the extras of a `Scene`. The extras are `Actors` which
    /// can be added to or removed from a `Scene` after creation, but
    /// cannot directly participate in `Plays`.
    fn get_extras(&self) -> &[<Self::P as SkyliteProject>::Actors];

    /// Adds an `Actor` as an extra to the `Scene`.
    fn add_extra(&mut self, extra: <Self::P as SkyliteProject>::Actors) {
        self._private_extras().push(extra)
    }

    /// Removes the extra at the given `index`.
    fn remove_extra(&mut self, index: usize) {
        self._private_extras().remove(index);
    }
}

/// Returns all `Actors` in a `scene` of a specific type. This includes both the main
/// actors as well as extras.
fn query_actors<'scene, P: SkyliteProject, A: Actor>(scene: &'scene dyn Scene<P=P>) -> Vec<&'scene A> {
    let mut out = Vec::new();
    for a in scene.get_actors() {
        if a.get_id() == <A as TypeId>::get_id() {
            let type_ref: &A = unsafe {
                a._private_transmute::<A>()
            };
            out.push(type_ref);
        }
    }

    for a in scene.get_extras() {
        if a.get_id() == <A as TypeId>::get_id() {
            let type_ref: &A = unsafe {
                a._private_transmute::<A>()
            };
            out.push(type_ref);
        }
    }
    out
}

/// Calls a callable for all `Actors` of a `scene` with a specific type. This includes both main
/// actors as well as extras. Each actor is passed to the callable as a mutable reference, so
/// this `apply_to_actors` can be used to modify the state of the actors.
fn apply_to_actors<P: SkyliteProject, A: Actor, F: Fn(&mut A)>(scene: &mut dyn Scene<P=P>, function: F) {
    for a in scene._private_actors().iter_mut() {
        if a.get_id() == <A as TypeId>::get_id() {
            let type_ref = unsafe {
                a._private_transmute_mut::<A>()
            };
            function(type_ref);
        }
    }
    for a in scene._private_extras() {
        if a.get_id() == <A as TypeId>::get_id() {
            let type_ref = unsafe {
                a._private_transmute_mut::<A>()
            };
            function(type_ref);
        }
    }
}

#[doc(hidden)]
pub mod _private {
    use crate::{actors::ActorBase, DrawContext, ProjectControls, SkyliteProject};

    use super::Scene;

    pub fn update_scene<P: SkyliteProject>(scene: &mut dyn Scene<P=P>, controls: &mut ProjectControls<P>) {
        // We need to take the lists of actors and scenes out of the scene here,
        // to pass the borrow checks. After each actor and extra is updated, the
        // lists are restored.
        let mut actors = scene._private_take_actors();
        let mut extras = scene._private_take_extras();

        for actor in actors.iter_mut() {
            actor._private_update(scene, controls);
        }
        scene._private_restore_actors(actors);

        for extra in extras.iter_mut() {
            extra._private_update(scene, controls);
        }
        // Restoring the extras has the additional effect, that any extras
        // added by any of the update calls to not get updated themselves in the
        // same cycle, which is intended. _private_restore_extras must make sure
        // to re-add the existing extras in the proper order, i.e. at the front.
        scene._private_restore_extras(extras);
    }

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

        scene.get_actors().iter().for_each(&mut insert_by_z_order);
        scene.get_extras().iter().for_each(&mut insert_by_z_order);

        z_sorted.iter().for_each(|a| a._private_render(ctx));
    }
}
