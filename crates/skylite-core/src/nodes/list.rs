use crate::decode::read_varint;
use crate::nodes::{Node, NodeIterator, NodeIteratorMut, NodeList, TypeId};
use crate::SkyliteProject;

/// A Node that integrates the contents of a `NodeList` into the Node tree
/// by adding the nodes to the `SList`'s dynamic nodes.
pub struct SList<P: SkyliteProject> {
    nodes: NodeList<P>,
}

impl<P: SkyliteProject> SList<P> {
    /// Creates a new `SList` for adding the given `NodeList` to the node tree.
    pub fn new(nodes: NodeList<P>) -> SList<P> {
        SList { nodes }
    }
}

impl<P: SkyliteProject> TypeId for SList<P> {
    fn get_id() -> usize
    where
        Self: Sized,
    {
        Self::get_id as usize
    }
}

impl<P: SkyliteProject> Node for SList<P> {
    type P = P;

    fn _private_decode(decoder: &mut dyn skylite_compress::Decoder) -> Self
    where
        Self: Sized,
    {
        let id = read_varint(decoder);
        SList {
            nodes: P::_private_decode_node_list(id),
        }
    }

    fn _private_update(&mut self, controls: &mut crate::ProjectControls<Self::P>) {
        for node in self.nodes.0.iter_mut() {
            node._private_update(controls);
        }
    }

    fn _private_render(&self, _ctx: &mut crate::RenderControls<Self::P>) {}

    fn z_order(&self) -> i32 {
        1
    }

    fn is_visible(&self, _ctx: &crate::RenderControls<Self::P>) -> bool {
        false
    }

    fn iter_nodes<'node>(&'node self) -> NodeIterator<'node, Self::P> {
        use crate::nodes::NodeIterable;
        let mut iter = NodeIterator::new();
        iter._private_push_sub_iterator(self.nodes.0.get_iterator());
        iter
    }

    fn iter_nodes_mut<'node>(&'node mut self) -> super::NodeIteratorMut<'node, Self::P> {
        use crate::nodes::NodeIterableMut;

        let mut iter = NodeIteratorMut::new();
        iter._private_push_sub_iterator(self.nodes.0.get_iterator_mut());
        iter
    }
}
