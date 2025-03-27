use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::encode::{CompressionBuffer, Serialize};
use crate::change_case;
use crate::generate::project::project_ident;
use crate::parse::node_lists::NodeList;

fn encode_node_list(list: &NodeList) -> TokenStream {
    let data: Vec<u8> = list
        .content
        .iter()
        .flat_map(|instance| {
            let mut buffer = CompressionBuffer::new();
            buffer.write_varint(instance.node_id);
            for arg in &instance.args {
                arg.serialize(&mut buffer);
            }
            buffer.encode()
        })
        .collect();

    quote!(&[#(#data),*])
}

pub(crate) fn generate_node_lists(node_lists: &[NodeList], project_name: &str) -> TokenStream {
    let project_crate = format_ident!(
        "{}",
        change_case(project_name, crate::IdentCase::LowerSnakeCase)
    );
    let project_ident = project_ident(project_name);
    let node_list_data = node_lists.iter().map(encode_node_list);
    let num_node_lists = node_lists.len();

    quote! {
        static NODE_LIST_DATA: [&[u8]; #num_node_lists] = [
            #(#node_list_data),*
        ];

        pub(crate) fn _private_decode_node_list(id: u32) -> ::skylite_core::nodes::NodeList<#project_ident> {
            let data = crate::#project_crate::gen::NODE_LIST_DATA[id as usize];
            let mut decoder = ::skylite_compress::make_decoder(data);
            let len = ::skylite_core::decode::read_varint(decoder.as_mut());
            let nodes: Vec<Box<dyn ::skylite_core::nodes::Node<P=#project_ident>>> = (0..len)
                .map(|_| crate::#project_crate::gen::_private_decode_node(decoder.as_mut()))
                .collect();
            ::skylite_core::nodes::NodeList::new(nodes)
        }
    }
}
