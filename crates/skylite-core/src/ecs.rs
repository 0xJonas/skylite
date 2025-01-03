use std::{cell::UnsafeCell, mem::transmute};

use crate::actors::{InstanceId, TypeId};

/// Marks a type as a component. This trait should only
/// be implemented through `#[derive(Component)]`.
pub trait Component: TypeId + InstanceId {}

/// An `Entity` is a list of components.
pub struct Entity {
    components: Vec<Box<UnsafeCell<dyn Component>>>
}

impl Entity {
    pub fn new() -> Entity {
        Entity { components: Vec::new() }
    }

    /// Adds a component to the `Entity`. An `Entity` can only contain a single instance
    /// of any type of component, so if the same type is added multiple times, this
    /// function will panic.
    pub fn add_component(&mut self, new_component: Box<dyn Component>) {
        if self.components.iter().any(|c| unsafe { &*c.get() }.get_id() == new_component.get_id()) {
            panic!("Component already exists in entity.");
        } else {
            // SAFETY: UnsafeCell has repr(transparent) (i.e. the same
            // memory layout as its contents) so this is ok:
            self.components.push(unsafe { transmute(new_component) });
        }
    }

    fn remove_component_by_type_id(&mut self, type_id: usize) {
        self.components.retain(|c| unsafe { &*c.get() }.get_id() != type_id);
    }

    /// Removes a component of a particular type from the `Entity`.
    /// If the `Entity` does not contain this type, this function has no effect.
    pub fn remove_component<C: Component>(&mut self) {
        self.remove_component_by_type_id(<C as TypeId>::get_id());
    }

    /// Returns shared reference to the component with the given
    /// type from the `Entity`, or None if the `Entity` does not have
    /// that type of component.
    pub fn get_component<C: Component>(&self) -> Option<&C> {
        self.components.iter()
            .find(|c| unsafe { &*c.get() }.get_id() == <C as TypeId>::get_id())
            // Hopefully this pointer cast is defined behavior? It should just discard the vtable.
            .map(|c| unsafe { &*(c.get() as *const C) })
    }

    fn get_component_mut_unsafe<C: Component>(&self) -> Option<&mut C> {
        self.components.iter()
            .find(|c| unsafe { &*c.get() }.get_id() == <C as TypeId>::get_id())
            .map(|c| unsafe { &mut *(c.get() as *mut C) })
    }

    /// Returns a mutable reference to the component with the given
    /// type from the `Entity`, or None if the `Entity` does not have
    /// that type of component.
    pub fn get_component_mut<C: Component>(&mut self) -> Option<&mut C> {
        self.get_component_mut_unsafe::<C>()
    }
}

pub mod __private {
    use super::{Component, Entity};

    fn check_distinct(addresses: &[usize]) -> bool {
        for i in 0..addresses.len() {
            for j in i + 1 .. addresses.len() {
                if addresses[i] == addresses[j] {
                    return false;
                }
            }
        }
        true
    }

    macro_rules! system_fn {
        ($name:ident, $($var:ident),*) => {
            #[allow(non_snake_case)]
            pub fn $name<'a, $($var: Component),*>(entities: impl IntoIterator<Item=&'a mut Entity>, mut fun: impl FnMut($(&mut $var),*))
            {
                // The Cx-types are guaranteed to be distinct when the systemx
                // functions are called through the proc-macro.
                for e in entities {
                    $(let $var = e.get_component_mut_unsafe::<$var>();)*
                    match ($($var,)*) {
                        ($(Some($var),)*) => {
                            #[cfg(debug_assertions)]
                            if !check_distinct(&[$($var as *mut _ as usize),*]) {
                                panic!("Indistinguishable components found. This is probably a bug.");
                            }
                            fun($($var),*);
                        },
                        _ => {}
                    }
                }
            }
        };
    }

    system_fn!(system1, C1);
    system_fn!(system2, C1, C2);
    system_fn!(system3, C1, C2, C3);
    system_fn!(system4, C1, C2, C3, C4);
    system_fn!(system5, C1, C2, C3, C4, C5);
    system_fn!(system6, C1, C2, C3, C4, C5, C6);
    system_fn!(system7, C1, C2, C3, C4, C5, C6, C7);
    system_fn!(system8, C1, C2, C3, C4, C5, C6, C7, C8);
}

#[cfg(test)]
mod tests {

    use crate::{actors::TypeId, ecs::__private::system1};

    use super::{Component, Entity};

    struct Component1(usize);

    impl TypeId for Component1 {
        fn get_id() -> usize {
            <Self as TypeId>::get_id as usize
        }
    }
    impl Component for Component1 {}

    struct Component2(usize);

    impl TypeId for Component2 {
        fn get_id() -> usize {
            <Self as TypeId>::get_id as usize
        }
    }
    impl Component for Component2 {}

    fn get_test_entities() -> Vec<Entity> {
        let mut entities = vec![
            Entity { components: vec![] },
            Entity { components: vec![] }
        ];
        entities[0].add_component(Box::new(Component1(5)));
        entities[0].add_component(Box::new(Component2(10)));
        entities[1].add_component(Box::new(Component1(20)));

        entities
    }

    #[test]
    fn test_components() {
        let entities = get_test_entities();

        assert_eq!(entities[0].get_component::<Component1>().unwrap().0, 5);
        assert_eq!(entities[0].get_component::<Component2>().unwrap().0, 10);
        assert_eq!(entities[1].get_component::<Component1>().unwrap().0, 20);
        assert!(entities[1].get_component::<Component2>().is_none());
    }

    #[test]
    fn test_system() {
        let mut entities = get_test_entities();
        let mut sum = 0;
        system1(entities.iter_mut(), |c: &mut Component1| sum += c.0);

        assert_eq!(sum, 25);

        sum = 0;
        system1(entities.iter_mut(), |c: &mut Component2| sum += c.0);
        assert_eq!(sum, 10);
    }
}
