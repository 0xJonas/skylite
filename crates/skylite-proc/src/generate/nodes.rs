use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Field, Ident, Item, ItemFn, ItemStruct, Meta};

use super::encode::{CompressionBuffer, Serialize};
use crate::assets::AssetSource;
use crate::generate::project::project_ident;
use crate::generate::util::{
    generate_argument_list, generate_deserialize_statements, generate_field_list,
    get_annotated_function, validate_type,
};
use crate::parse::node_lists::NodeList;
use crate::parse::nodes::{Node, NodeInstance};
use crate::{change_case, IdentCase, SkyliteProcError};

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

fn get_fn_name(item: &ItemFn) -> &Ident {
    &item.sig.ident
}

enum ChildNode {
    Single(Ident),
    Iterable(Ident),
}

struct NodeType {
    properties: Vec<Ident>,
    child_nodes: Vec<ChildNode>,
}

/// Removes the specified annotation from the field's attributes.
/// Returns true if the annotation was present and removed, false otherwise.
fn remove_annotation(field: &mut Field, attr: &str) -> bool {
    let meta = syn::parse_str::<Meta>(attr).unwrap();
    let org_size = field.attrs.len();
    field.attrs.retain(|a| a.meta != meta);
    field.attrs.len() != org_size
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

fn parse_node_struct(
    node: &Node,
    node_struct: &mut ItemStruct,
) -> Result<NodeType, SkyliteProcError> {
    let node_type = match node_struct.fields {
        syn::Fields::Unnamed(_) => NodeType {
            properties: vec![],
            child_nodes: vec![],
        },
        syn::Fields::Unit => NodeType {
            properties: vec![],
            child_nodes: vec![],
        },
        syn::Fields::Named(ref mut fields_named) => {
            let mut properties = vec![];
            let mut child_nodes = vec![];
            for field in &mut fields_named.named {
                if remove_annotation(field, "skylite_proc::property") {
                    validate_property(node, &field)?;
                    properties.push(field.ident.clone().unwrap());
                }
                if remove_annotation(field, "skylite_proc::node") {
                    child_nodes.push(ChildNode::Single(field.ident.clone().unwrap()));
                }
                if remove_annotation(field, "skylite_proc::nodes") {
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

    let new_fn = get_annotated_function(items, "skylite_proc::new")
        .map(get_fn_name)
        .ok_or(syntax_err!(
            "Missing required function `#[skylite_proc::new]`"
        ))?;

    Ok(quote! {
        impl #node_name {
            pub(crate) fn _private_new(#params) -> Self {
                #new_fn(#args)
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

    let pre_update_call = get_annotated_function(items, "skylite_proc::pre_update")
        .map(get_fn_name)
        .map_or(TokenStream::new(), |item| quote!(#item(self, controls)));

    let update_call_opt = get_annotated_function(items, "skylite_proc::update")
        .map(get_fn_name)
        .map(|item| quote!(#item(self, controls)));

    let post_update_call_opt = get_annotated_function(items, "skylite_proc::post_update")
        .map(get_fn_name)
        .map(|item| quote!(#item(self, controls)));

    if update_call_opt.is_some() && post_update_call_opt.is_some() {
        return Err(data_err!("skylite_proc::update and skylite_proc::post_update have the same meaning, only one must be given."));
    }

    let post_update_call = update_call_opt.or(post_update_call_opt).unwrap_or_default();

    let render_call_opt = get_annotated_function(items, "skylite_proc::render")
        .map(get_fn_name)
        .map(|item| quote!(#item(self, ctx)));

    let is_visible_call = get_annotated_function(items, "skylite_proc::is_visible")
        .map(get_fn_name)
        .map_or(
            if render_call_opt.is_some() {
                quote!(true)
            } else {
                quote!(false)
            },
            |item| quote!(#item(self, ctx)),
        );

    let render_call = render_call_opt.unwrap_or_default();

    let z_order_call = get_annotated_function(items, "skylite_proc::z_order")
        .map(get_fn_name)
        .map_or(quote!(1), |item| quote!(#item(self)));

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

            fn z_order(&self) -> i32 {
                #z_order_call
            }

            fn is_visible(&self, ctx: &::skylite_core::RenderControls<Self::P>) -> bool {
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
    items: &'b mut [Item],
) -> Result<&'b mut ItemStruct, SkyliteProcError> {
    let name = node_type_name(&node.meta.name);
    items
        .iter_mut()
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
    mut items: Vec<Item>,
) -> Result<TokenStream, SkyliteProcError> {
    let node_name = node_type_name(&node.meta.name);
    let node_struct = find_node_struct(node, &mut items)?;
    let node_type = parse_node_struct(node, node_struct)?;
    let node_new_method = gen_node_new_fn(node, &items)?;
    let node_impl = gen_node_impl(node, &node_type, project_name, &items)?;

    Ok(quote! {
        #(#items)
        *

        impl ::skylite_core::nodes::TypeId for #node_name {
            fn get_id() -> usize {
                <Self as ::skylite_core::nodes::TypeId>::get_id as usize
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

            #[skylite_proc::new]
            fn new(param1: u8, param2: u16) -> TestNode {
                todo!()
            }

            #[skylite_proc::init]
            fn init(&mut self, id: &str) {}

            #[skylite_proc::pre_update]
            fn pre_update(&mut self, controls: &mut ProjectControls<MyProject>) {}

            #[skylite_proc::update]
            fn update(&mut self, controls: &mut ProjectControls<MyProject>) {}

            #[skylite_proc::render]
            fn render(&self, ctx: &mut RenderControls<MyProject>) {}
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
                    pre_update(self, controls);

                    ::skylite_core::nodes::_private::update_node_rec(self, controls);

                    update(self, controls);
                }

                fn _private_render(&self, ctx: &mut ::skylite_core::RenderControls<Self::P>) {
                    render(self, ctx);
                }

                fn z_order(&self) -> i32 {
                    1
                }

                fn is_visible(&self, ctx: &::skylite_core::RenderControls<Self::P>) -> bool {
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
