use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};

use super::encode::CompressionBuffer;
use crate::generate::nodes::encode_node_instance;
use crate::generate::project::project_ident;
use crate::parse::node_lists::NodeList;
use crate::{change_case, IdentCase};

fn encode_node_list(list: &NodeList) -> TokenStream {
    let mut buffer = CompressionBuffer::new();
    buffer.write_varint(list.content.len());
    for instance in &list.content {
        encode_node_instance(instance, &mut buffer)
    }
    let data = buffer.encode();

    quote!(&[#(#data),*])
}

pub(crate) fn generate_node_list_data(node_lists: &[NodeList]) -> TokenStream {
    let node_list_data = node_lists.iter().map(encode_node_list);
    let num_node_lists = node_lists.len();

    quote! {
        static NODE_LIST_DATA: [&[u8]; #num_node_lists] = [
            #(#node_list_data),*
        ];
    }
}

pub(crate) fn node_list_ids_type(project_name: &str) -> Ident {
    format_ident!(
        "{}NodeListIds",
        change_case(project_name, IdentCase::UpperCamelCase)
    )
}

pub(crate) fn generate_node_list_ids(node_lists: &[NodeList], project_name: &str) -> TokenStream {
    let node_list_ids_type = node_list_ids_type(project_name);
    let names = node_lists.iter().map(|list| {
        format_ident!(
            "{}",
            change_case(&list.meta.name, IdentCase::UpperCamelCase)
        )
    });
    let ids = node_lists.iter().map(|list| list.meta.id);

    quote! {
        #[repr(usize)]
        #[derive(Clone, Copy)]
        pub enum #node_list_ids_type {
            #(#names = #ids),*
        }

        impl ::skylite_core::Ids for #node_list_ids_type {
            fn get(self) -> usize {
                self as usize
            }
        }
        impl ::skylite_core::nodes::NodeListIds for #node_list_ids_type {}
    }
}

pub(crate) fn generate_decode_node_list_fn(project_name: &str) -> TokenStream {
    let project_crate = format_ident!(
        "{}",
        change_case(project_name, crate::IdentCase::LowerSnakeCase)
    );
    let project_ident = project_ident(project_name);

    quote! {
        fn _private_decode_node_list(id: usize) -> ::skylite_core::nodes::NodeList<#project_ident> {
            let data = crate::#project_crate::gen::NODE_LIST_DATA[id as usize];
            let mut decoder = ::skylite_compress::make_decoder(data);
            let len = ::skylite_core::decode::read_varint(decoder.as_mut());
            let nodes: Vec<Box<dyn ::skylite_core::nodes::Node<P=#project_ident>>> = (0..len)
                .map(|_| #project_ident::_private_decode_node(decoder.as_mut()))
                .collect();
            ::skylite_core::nodes::NodeList::new(nodes)
        }
    }
}
