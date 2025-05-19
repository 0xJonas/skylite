use skylite_compress::Decoder;

use crate::{DrawContext, Ids, ProjectControls, SkyliteProject};

mod list;

pub use list::SList;

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

/// Attempts to convert a trait object to a concrete type.
pub fn try_as_type<T: TypeId + InstanceId>(node: &dyn InstanceId) -> Option<&T> {
    if node.get_id() == <T as TypeId>::get_id() {
        Some(unsafe { &*(node as *const dyn InstanceId as *const T) })
    } else {
        None
    }
}

/// Attempts to convert a trait object to a concrete type.
pub fn try_as_type_mut<T: TypeId + InstanceId>(node: &mut dyn InstanceId) -> Option<&mut T> {
    if node.get_id() == <T as TypeId>::get_id() {
        Some(unsafe { &mut *(node as *mut dyn InstanceId as *mut T) })
    } else {
        None
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
    fn get_static_nodes(&self) -> Box<[&dyn Node<P = Self::P>]>;

    /// Returns a shared references to the list of this node's dynamic children.
    fn get_dynamic_nodes(&self) -> &Vec<Box<dyn Node<P = Self::P>>>;

    /// Returns a mutable references to the list of this node's static children.
    fn get_static_nodes_mut(&mut self) -> Box<[&mut dyn Node<P = Self::P>]>;

    /// Returns a mutable references to the list of this node's dynamic
    /// children. This result of this method can be used to add or remove
    /// dynamic nodes.
    fn get_dynamic_nodes_mut(&mut self) -> &mut Vec<Box<dyn Node<P = Self::P>>>;
}

/// A collection of `Nodes`.
pub struct NodeList<P: SkyliteProject>(Vec<Box<dyn Node<P = P>>>);

pub trait NodeListIds: Ids {}

impl<P: SkyliteProject> NodeList<P> {
    /// Creates a new `NodeList`.
    pub fn new(nodes: Vec<Box<dyn Node<P = P>>>) -> NodeList<P> {
        NodeList(nodes)
    }

    /// Loads the pre-defined node list with the given id.
    pub fn load(id: P::NodeListIds) -> NodeList<P> {
        P::_private_decode_node_list(id.get())
    }

    /// Returns a shared reference to the `NodeList`'s contents.
    pub fn get_nodes(&self) -> &Vec<Box<dyn Node<P = P>>> {
        &self.0
    }

    /// Returns a mutable reference to the `NodeList`'s contents.
    pub fn get_nodes_mut(&mut self) -> &mut Vec<Box<dyn Node<P = P>>> {
        &mut self.0
    }
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
            for n in node.get_static_nodes_mut().iter_mut() {
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

pub mod _private {
    use std::marker::PhantomData;

    use skylite_compress::Decoder;

    use super::{Node, TypeId};
    use crate::{DrawContext, ProjectControls, SkyliteProject};

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

    fn insert_by_z_order<'nodes, P: SkyliteProject>(
        list: &mut Vec<&'nodes dyn Node<P = P>>,
        node: &'nodes dyn Node<P = P>,
    ) {
        for (i, n) in list.iter().enumerate() {
            if node.z_order() <= n.z_order() {
                list.insert(i, node);
                return;
            }
        }
        list.push(node);
    }

    fn insert_nodes_by_z_order_rec<'nodes, P: SkyliteProject>(
        list: &mut Vec<&'nodes dyn Node<P = P>>,
        node: &'nodes dyn Node<P = P>,
        ctx: &DrawContext<P>,
    ) {
        for n in node.get_static_nodes().iter() {
            if n.is_visible(ctx) {
                insert_by_z_order(list, *n);
            }
            insert_nodes_by_z_order_rec(list, *n, ctx);
        }

        for n in node.get_dynamic_nodes() {
            if n.is_visible(ctx) {
                insert_by_z_order(list, n.as_ref());
            }
            insert_nodes_by_z_order_rec(list, n.as_ref(), ctx);
        }
    }

    pub fn render_node<P: SkyliteProject>(node: &dyn Node<P = P>, ctx: &mut DrawContext<P>) {
        let mut z_sorted: Vec<&dyn Node<P = P>> = Vec::new();

        insert_nodes_by_z_order_rec(&mut z_sorted, node, ctx);
        insert_by_z_order(&mut z_sorted, node);

        z_sorted.iter().for_each(|a| a._private_render(ctx));
    }

    struct DummyNode<P: SkyliteProject>(PhantomData<P>);

    impl<P: SkyliteProject> TypeId for DummyNode<P> {
        fn get_id() -> usize
        where
            Self: Sized,
        {
            todo!()
        }
    }

    impl<P: SkyliteProject> Node for DummyNode<P> {
        type P = P;

        fn _private_decode(_decoder: &mut dyn Decoder) -> Self
        where
            Self: Sized,
        {
            unimplemented!()
        }

        fn _private_update(&mut self, _controls: &mut ProjectControls<Self::P>) {
            unimplemented!()
        }

        fn _private_render(&self, _ctx: &mut DrawContext<Self::P>) {
            unimplemented!()
        }

        fn z_order(&self) -> i32 {
            unimplemented!()
        }

        fn is_visible(&self, _ctx: &DrawContext<Self::P>) -> bool {
            unimplemented!()
        }

        fn get_static_nodes(&self) -> Box<[&dyn Node<P = Self::P>]> {
            unimplemented!()
        }

        fn get_dynamic_nodes(&self) -> &Vec<Box<dyn Node<P = Self::P>>> {
            unimplemented!()
        }

        fn get_static_nodes_mut(&mut self) -> Box<[&mut dyn Node<P = Self::P>]> {
            unimplemented!()
        }

        fn get_dynamic_nodes_mut(&mut self) -> &mut Vec<Box<dyn Node<P = Self::P>>> {
            unimplemented!()
        }
    }

    pub fn replace_node<P: SkyliteProject + 'static, Src: FnOnce() -> Box<dyn Node<P = P>>>(
        src: Src,
        dest: &mut Box<dyn Node<P = P>>,
    ) {
        *dest = Box::new(DummyNode(PhantomData));
        *dest = src();
    }
}
