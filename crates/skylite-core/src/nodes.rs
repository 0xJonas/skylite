use skylite_compress::Decoder;

use crate::{DrawContext, ProjectControls, SkyliteProject};

/// **For internal use only.**
///
/// Used to assign an id to a specific type.
pub trait TypeId {
    fn get_id() -> usize
    where
        Self: Sized;
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

/// Nodes are the primary elements from which a Skylite project is constructed.
///
/// Each node contains two sets of children:
/// - The set of static child nodes is the one which is declared in the asset
///   file and never changes throughout the livetime of the node. This means
///   that properties of static child nodes can be animated by Sequences.
/// - The set of dynamic child nodes can be changed during a Node's livetime,
///   but properties of dynamic nodes cannot be animated by sequences.
pub trait Node: TypeId + InstanceId {
    type P: SkyliteProject;

    fn _private_decode(decoder: &mut dyn Decoder) -> Self
    where
        Self: Sized;

    fn _private_update(&mut self, controls: &mut ProjectControls<Self::P>);

    fn _private_render(&self, ctx: &mut DrawContext<Self::P>);

    fn z_order(&self) -> i32;

    fn is_visible(&self, ctx: &DrawContext<Self::P>) -> bool;

    /// Returns a shared references to the list of this node's static children.
    fn get_static_nodes(&self) -> &[&dyn Node<P = Self::P>];

    /// Returns a shared references to the list of this node's dynamic children.
    fn get_dynamic_nodes(&self) -> &Vec<Box<dyn Node<P = Self::P>>>;

    /// Returns a mutable references to the list of this node's static children.
    fn get_static_nodes_mut(&mut self) -> &mut [&mut dyn Node<P = Self::P>];

    /// Returns a mutable references to the list of this node's dynamic
    /// children. This result of this method can be used to add or remove
    /// dynamic nodes.
    fn get_dynamic_nodes_mut(&mut self) -> &mut Vec<Box<dyn Node<P = Self::P>>>;
}

macro_rules! system_fn {
    ($name:ident, $($vars:ident : $types:ident),+) => {
        pub fn $name<P: SkyliteProject, $($types: Node<P=P>),+>(node: &mut dyn Node<P=P>, func: &mut impl FnMut($(&mut $types),+)) {
            // Variables which hold mutable references to each node of a matching type.
            $(
                let mut $vars: Option<&mut $types> = None;
            )+

            // Iterate over the static child nodes and fill the references as matching
            // nodes are found. Also invoke the system on each child node recursively.
            for n in node.get_static_nodes_mut() {
                $(
                    if n.get_id() == <$types as TypeId>::get_id() {
                        $vars = Some(unsafe {&mut *((*n) as *mut dyn Node<P=P> as *mut $types) })
                    }
                )+

                $name::<P, $($types),+>(*n, func);
            }

            // Same as above, but for the dynamic nodes.
            for n in node.get_dynamic_nodes_mut() {
                $(
                    if n.get_id() == <$types as TypeId>::get_id() {
                        $vars = Some( unsafe { &mut *(n.as_mut() as *mut dyn Node<P=P> as *mut $types) })
                    }
                )+

                $name::<P, $($types),+>(n.as_mut(), func);
            }

            // If a node for each parameter type of the system function was found, call the system.
            if $($vars.is_some())&&+ {
                func($($vars.unwrap()),+);
            }
        }
    };
}

system_fn!(system1, n1:N1);
system_fn!(system2, n1:N1, n2:N2);
system_fn!(system3, n1:N1, n2:N2, n3:N3);
system_fn!(system4, n1:N1, n2:N2, n3:N3, n4:N4);
system_fn!(system5, n1:N1, n2:N2, n3:N3, n4:N4, n5:N5);
system_fn!(system6, n1:N1, n2:N2, n3:N3, n4:N4, n5:N5, n6:N6);
system_fn!(system7, n1:N1, n2:N2, n3:N3, n4:N4, n5:N5, n6:N6, n7:N7);
system_fn!(system8, n1:N1, n2:N2, n3:N3, n4:N4, n5:N5, n6:N6, n7:N7, n8:N8);

mod _private {
    use super::Node;
    use crate::{ProjectControls, SkyliteProject};

    pub fn update_node_rec<P: SkyliteProject>(
        node: &mut dyn Node<P = P>,
        controls: &mut ProjectControls<P>,
    ) {
        node.get_static_nodes_mut()
            .iter_mut()
            .for_each(|sub| sub._private_update(controls));
        node.get_dynamic_nodes_mut()
            .iter_mut()
            .for_each(|sub| sub._private_update(controls));
    }
}
