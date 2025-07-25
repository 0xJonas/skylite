use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Field, Ident, Item, ItemStruct, Meta};

use super::encode::{CompressionBuffer, Serialize};
use crate::assets::AssetSource;
use crate::generate::project::project_ident;
use crate::generate::util::{
    generate_argument_list, generate_deserialize_statements, generate_field_list,
    get_annotated_method_name, validate_type,
};
use crate::generate::{
    ANNOTATION_IS_VISIBLE, ANNOTATION_NEW, ANNOTATION_NODE, ANNOTATION_NODES,
    ANNOTATION_POST_UPDATE, ANNOTATION_PRE_UPDATE, ANNOTATION_PROPERTY, ANNOTATION_RENDER,
    ANNOTATION_UPDATE, ANNOTATION_Z_ORDER,
};
use crate::parse::node_lists::NodeList;
use crate::parse::nodes::{Node, NodeInstance};
use crate::parse::util::{change_case, IdentCase};
use crate::SkyliteProcError;

pub fn node_type_name(name: &str) -> Ident {
    format_ident!("{}", change_case(name, IdentCase::UpperCamelCase))
}

pub(crate) fn encode_node_instance(instance: &NodeInstance, buffer: &mut CompressionBuffer) {
    buffer.write_varint(instance.node_id);
    instance.args.iter().for_each(|v| v.serialize(buffer));
}

pub(crate) fn generate_decode_node_fn(
    project_name: &str,
    nodes: &[&Node],
    node_lists: &[&NodeList],
) -> TokenStream {
    // Only include nodes which are actually encoded,
    // i.e. those which appear as NodeInstances in NodeLists or Node properties.
    // This is so that unused nodes can be removed by LTO.

    let used_nodes = node_lists
        .iter()
        .flat_map(|node_list| node_list.content.iter())
        .map(|i| i.node_id)
        .collect::<HashSet<usize>>();

    let match_arms = used_nodes.iter().map(|id| {
        let node = &nodes[*id];
        let id = node.meta.id;
        let ident = node_type_name(&node.meta.name);
        match node.meta.source {
            AssetSource::BuiltIn(_) => {
                // Use full path for built-in nodes, since it is known.
                quote!(#id => Box::new(::skylite_core::nodes::#ident::_private_decode(decoder)))
            }
            _ => quote!(#id => Box::new(#ident::_private_decode(decoder))),
        }
    });

    let project_ident = project_ident(project_name);

    quote! {
        fn _private_decode_node(
            decoder: &mut dyn ::skylite_compress::Decoder
        ) -> Box<dyn ::skylite_core::nodes::Node<P=#project_ident>> {
            use ::skylite_core::nodes::Node;
            let id = ::skylite_core::decode::read_varint(decoder);
            match id {
                #(#match_arms,)*
                _ => unreachable!()
            }
        }
    }
}

enum ChildNode {
    Single(Ident),
    Iterable(Ident),
}

struct NodeType {
    properties: Vec<Ident>,
    child_nodes: Vec<ChildNode>,
}

fn has_annotation(field: &Field, attr: &str) -> bool {
    let meta = syn::parse_str::<Meta>(attr).unwrap();
    field.attrs.iter().any(|a| a.meta == meta)
}

fn validate_property(node: &Node, field: &syn::Field) -> Result<(), SkyliteProcError> {
    let name = field.ident.as_ref().unwrap().to_string();

    // Ensure property is pub or pub(crate).
    if matches!(&field.vis, syn::Visibility::Inherited)
        || matches!(&field.vis, syn::Visibility::Restricted(vis_restricted) if !vis_restricted.path.is_ident("crate"))
    {
        return Err(data_err!("Property {name} must be pub or pub(crate)"));
    }

    // Ensure the type matches the declaration in Scheme.
    let variable = node
        .properties
        .iter()
        .find(|var| change_case(&var.name, IdentCase::LowerSnakeCase) == name)
        .ok_or(data_err!("Property {name} not found in node definition."))?;
    if validate_type(&variable.typename, &field.ty) {
        Ok(())
    } else {
        Err(data_err!(
            "Type for field {name} does not match node declaration."
        ))
    }
}

fn parse_node_struct(node: &Node, node_struct: &ItemStruct) -> Result<NodeType, SkyliteProcError> {
    let node_type = match node_struct.fields {
        syn::Fields::Unnamed(_) => NodeType {
            properties: vec![],
            child_nodes: vec![],
        },
        syn::Fields::Unit => NodeType {
            properties: vec![],
            child_nodes: vec![],
        },
        syn::Fields::Named(ref fields_named) => {
            let mut properties = vec![];
            let mut child_nodes = vec![];
            for field in &fields_named.named {
                if has_annotation(field, ANNOTATION_PROPERTY) {
                    validate_property(node, &field)?;
                    properties.push(field.ident.clone().unwrap());
                }
                if has_annotation(field, ANNOTATION_NODE) {
                    child_nodes.push(ChildNode::Single(field.ident.clone().unwrap()));
                }
                if has_annotation(field, ANNOTATION_NODES) {
                    child_nodes.push(ChildNode::Iterable(field.ident.clone().unwrap()));
                }
            }

            NodeType {
                properties,
                child_nodes,
            }
        }
    };

    // All properties that are declared in Scheme must be marked on the Node type.
    // Since the name of the property on the Node type is already checked by
    // validate_property() and structs cannot have multiple fields with the same
    // name, we only need to check that the list of declared properties and
    // marked properties have the same length.
    if node_type.properties.len() != node.properties.len() {
        return Err(data_err!(
            "properties in node declaration do not match properties in Node struct."
        ));
    }

    Ok(node_type)
}

fn gen_node_new_fn(node: &Node, items: &[Item]) -> Result<TokenStream, SkyliteProcError> {
    let node_name = node_type_name(&node.meta.name);
    let params = generate_field_list(&node.parameters, TokenStream::new());
    let args = generate_argument_list(&node.parameters);

    let new_fn = get_annotated_method_name(items, ANNOTATION_NEW, &node_name)?.ok_or(
        syntax_err!("Missing required function with `#[{ANNOTATION_NEW}]`"),
    )?;

    Ok(quote! {
        impl #node_name {
            pub(crate) fn _private_new(#params) -> Self {
                #node_name::#new_fn(#args)
            }
        }
    })
}

fn gen_node_impl(
    node: &Node,
    node_type: &NodeType,
    project_name: &str,
    items: &[Item],
) -> Result<TokenStream, SkyliteProcError> {
    let node_name = node_type_name(&node.meta.name);
    let project_name = format_ident!("{}", change_case(project_name, IdentCase::UpperCamelCase));
    let decode_statements = generate_deserialize_statements(&node.parameters);
    let args = generate_argument_list(&node.parameters);

    let pre_update_call = get_annotated_method_name(items, ANNOTATION_PRE_UPDATE, &node_name)?
        .map_or(TokenStream::new(), |method| quote!(self.#method(controls)));

    let update_call_opt = get_annotated_method_name(items, ANNOTATION_UPDATE, &node_name)?
        .map(|method| quote!(self.#method(controls)));

    let post_update_call_opt =
        get_annotated_method_name(items, ANNOTATION_POST_UPDATE, &node_name)?
            .map(|method| quote!(self.#method(controls)));

    if update_call_opt.is_some() && post_update_call_opt.is_some() {
        return Err(data_err!("Annotations {ANNOTATION_UPDATE} and {ANNOTATION_POST_UPDATE} have the same meaning, only one must be given."));
    }

    let post_update_call = update_call_opt.or(post_update_call_opt).unwrap_or_default();

    let render_call_opt = get_annotated_method_name(items, ANNOTATION_RENDER, &node_name)?
        .map(|method| quote!(self.#method(ctx)));

    let is_visible_call = get_annotated_method_name(items, ANNOTATION_IS_VISIBLE, &node_name)?
        .map_or(
            if render_call_opt.is_some() {
                quote!(true)
            } else {
                quote!(false)
            },
            |method| quote!(self.#method(ctx)),
        );

    let render_call = render_call_opt.unwrap_or_default();

    let z_order_call = get_annotated_method_name(items, ANNOTATION_Z_ORDER, &node_name)?
        .map_or(quote!(1), |method| quote!(self.#method()));

    let push_child_nodes = node_type
        .child_nodes
        .iter()
        .map(|child| match child {
            ChildNode::Single(ident) => quote!(iter._private_push_single(&self.#ident);),
            ChildNode::Iterable(ident) => {
                quote!(iter._private_push_sub_iterator(self.#ident.get_iterator());)
            }
        })
        .rev(); // NodeIterator returns the elements pushed into it in reverse order.

    let push_child_nodes_mut = node_type
        .child_nodes
        .iter()
        .map(|child| match child {
            ChildNode::Single(ident) => quote!(iter._private_push_single(&mut self.#ident);),
            ChildNode::Iterable(ident) => {
                quote!(iter._private_push_sub_iterator(self.#ident.get_iterator_mut());)
            }
        })
        .rev();

    Ok(quote! {
        impl ::skylite_core::nodes::Node for #node_name {
            type P = #project_name;

            fn _private_decode(decoder: &mut dyn ::skylite_compress::Decoder) -> Self
            where
                Self: Sized
            {
                use ::skylite_core::decode::Deserialize;
                #decode_statements
                #node_name::_private_new(#args)
            }

            fn _private_update(&mut self, controls: &mut ::skylite_core::ProjectControls<Self::P>) {
                #pre_update_call;

                ::skylite_core::nodes::_private::update_node_rec(self, controls);

                #post_update_call;
            }

            fn _private_render(&self, ctx: &mut ::skylite_core::RenderControls<Self::P>) {
                #render_call;
            }

            fn _private_z_order(&self) -> i32 {
                #z_order_call
            }

            fn _private_is_visible(&self, ctx: &::skylite_core::RenderControls<Self::P>) -> bool {
                #is_visible_call
            }

            fn iter_nodes<'node>(&'node self) -> ::skylite_core::nodes::NodeIterator<'node, Self::P> {
                use ::skylite_core::nodes::NodeIterable;
                let mut iter = ::skylite_core::nodes::NodeIterator::new();
                #(
                    #push_child_nodes
                )*
                iter
            }

            fn iter_nodes_mut<'node>(&'node mut self) -> ::skylite_core::nodes::NodeIteratorMut<'node, Self::P> {
                use ::skylite_core::nodes::NodeIterableMut;
                let mut iter = ::skylite_core::nodes::NodeIteratorMut::new();
                #(
                    #push_child_nodes_mut
                )*
                iter
            }
        }
    })
}

fn find_node_struct<'a, 'b>(
    node: &'a Node,
    items: &'b [Item],
) -> Result<&'b ItemStruct, SkyliteProcError> {
    let name = node_type_name(&node.meta.name);
    items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(item_struct) if item_struct.ident == name => Some(item_struct),
            _ => None,
        })
        .next()
        .ok_or(syntax_err!("module must define a struct called {name}"))
}

pub(crate) fn generate_node_definition(
    node: &Node,
    project_name: &str,
    items: &[Item],
) -> Result<TokenStream, SkyliteProcError> {
    let id = node.meta.id;
    let node_name = node_type_name(&node.meta.name);
    let node_struct = find_node_struct(node, &items)?;
    let node_type = parse_node_struct(node, node_struct)?;
    let node_new_method = gen_node_new_fn(node, &items)?;
    let node_impl = gen_node_impl(node, &node_type, project_name, &items)?;

    Ok(quote! {
        impl ::skylite_core::nodes::TypeId for #node_name {
            fn get_id() -> usize {
                #id
            }
        }

        #node_new_method

        #node_impl
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use quote::quote;
    use syn::{parse_quote, File, Item};

    use crate::assets::{AssetMetaData, AssetSource, AssetType};
    use crate::generate::nodes::{find_node_struct, gen_node_impl, parse_node_struct};
    use crate::parse::nodes::Node;
    use crate::parse::values::{Type, Variable};

    fn create_test_node() -> Node {
        Node {
            meta: AssetMetaData {
                atype: AssetType::Node,
                name: "TestNode".to_owned(),
                id: 0,
                source: AssetSource::Path(PathBuf::new()),
            },
            parameters: vec![
                Variable {
                    name: "param1".to_owned(),
                    typename: Type::U8,
                    documentation: None,
                    default: None,
                },
                Variable {
                    name: "param2".to_owned(),
                    typename: Type::U16,
                    documentation: None,
                    default: None,
                },
            ],
            properties: vec![Variable {
                name: "sum".to_owned(),
                typename: Type::U16,
                documentation: None,
                default: None,
            }],
        }
    }

    fn create_test_items() -> Vec<Item> {
        let file: File = parse_quote! {
            struct TestNode {
                #[skylite_proc::property]
                pub sum: u16,

                #[skylite_proc::node] sub_node1: TestNode2,
                #[skylite_proc::nodes] sub_nodes2: Vec<TestNode2>,

                extra: bool
            }

            impl TestNode {
                #[skylite_proc::new]
                fn new(param1: u8, param2: u16) -> TestNode {
                    todo!()
                }

                #[skylite_proc::pre_update]
                fn pre_update(&mut self, controls: &mut ProjectControls<MyProject>) {}

                #[skylite_proc::update]
                fn update(&mut self, controls: &mut ProjectControls<MyProject>) {}

                #[skylite_proc::render]
                fn render(&self, ctx: &mut RenderControls<MyProject>) {}
            }
        };
        file.items
    }

    #[test]
    fn test_node_impl() {
        let node = create_test_node();
        let mut items = create_test_items();

        let node_struct = find_node_struct(&node, &mut items).unwrap();
        let node_type = parse_node_struct(&node, node_struct).unwrap();

        let actual = gen_node_impl(&node, &node_type, "TestProject", &items).unwrap();
        let expected = quote! {
            impl ::skylite_core::nodes::Node for TestNode {
                type P = TestProject;

                fn _private_decode(decoder: &mut dyn ::skylite_compress::Decoder) -> Self
                where
                    Self: Sized
                {
                    use ::skylite_core::decode::Deserialize;
                    let param1 = u8::deserialize(decoder);
                    let param2 = u16::deserialize(decoder);
                    TestNode::_private_new(param1, param2)
                }

                fn _private_update(&mut self, controls: &mut ::skylite_core::ProjectControls<Self::P>) {
                    self.pre_update(controls);

                    ::skylite_core::nodes::_private::update_node_rec(self, controls);

                    self.update(controls);
                }

                fn _private_render(&self, ctx: &mut ::skylite_core::RenderControls<Self::P>) {
                    self.render(ctx);
                }

                fn _private_z_order(&self) -> i32 {
                    1
                }

                fn _private_is_visible(&self, ctx: &::skylite_core::RenderControls<Self::P>) -> bool {
                    true
                }

                fn iter_nodes<'node>(&'node self) -> ::skylite_core::nodes::NodeIterator<'node, Self::P> {
                    use ::skylite_core::nodes::NodeIterable;
                    let mut iter = ::skylite_core::nodes::NodeIterator::new();
                    iter._private_push_sub_iterator(self.sub_nodes2.get_iterator());
                    iter._private_push_single(&self.sub_node1);
                    iter
                }

                fn iter_nodes_mut<'node>(&'node mut self) -> ::skylite_core::nodes::NodeIteratorMut<'node, Self::P> {
                    use ::skylite_core::nodes::NodeIterableMut;
                    let mut iter = ::skylite_core::nodes::NodeIteratorMut::new();
                    iter._private_push_sub_iterator(self.sub_nodes2.get_iterator_mut());
                    iter._private_push_single(&mut self.sub_node1);
                    iter
                }
            }
        };

        assert_eq!(actual.to_string(), expected.to_string());
    }
}
