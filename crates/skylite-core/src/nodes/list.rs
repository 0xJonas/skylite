use super::{Node, NodeList, TypeId};
use crate::decode::read_varint;
use crate::SkyliteProject;

pub struct List<P: SkyliteProject> {
    nodes: NodeList<P>,
}

impl<P: SkyliteProject> List<P> {
    pub fn new(nodes: NodeList<P>) -> List<P> {
        List { nodes }
    }
}

impl<P: SkyliteProject> TypeId for List<P> {
    fn get_id() -> usize
    where
        Self: Sized,
    {
        Self::get_id as usize
    }
}

impl<P: SkyliteProject> Node for List<P> {
    type P = P;

    fn _private_decode(decoder: &mut dyn skylite_compress::Decoder) -> Self
    where
        Self: Sized,
    {
        let id = read_varint(decoder);
        List {
            nodes: P::_private_decode_node_list(id),
        }
    }

    fn _private_update(&mut self, controls: &mut crate::ProjectControls<Self::P>) {
        for node in self.nodes.0.iter_mut() {
            node._private_update(controls);
        }
    }

    fn _private_render(&self, _ctx: &mut crate::DrawContext<Self::P>) {}

    fn z_order(&self) -> i32 {
        1
    }

    fn is_visible(&self, _ctx: &crate::DrawContext<Self::P>) -> bool {
        false
    }

    fn get_static_nodes(&self) -> Box<[&dyn Node<P = Self::P>]> {
        let out: Vec<&dyn Node<P = Self::P>> = Vec::new();
        out.into_boxed_slice()
    }

    fn get_dynamic_nodes(&self) -> &Vec<Box<dyn Node<P = Self::P>>> {
        self.nodes.get_nodes()
    }

    fn get_static_nodes_mut(&mut self) -> Box<[&mut dyn Node<P = Self::P>]> {
        let out: Vec<&mut dyn Node<P = Self::P>> = Vec::new();
        out.into_boxed_slice()
    }

    fn get_dynamic_nodes_mut(&mut self) -> &mut Vec<Box<dyn Node<P = Self::P>>> {
        self.nodes.get_nodes_mut()
    }
}
