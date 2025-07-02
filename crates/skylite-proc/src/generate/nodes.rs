use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, Item, ItemFn};

use super::encode::{CompressionBuffer, Serialize};
use crate::assets::AssetSource;
use crate::generate::project::project_ident;
use crate::generate::util::{
    generate_argument_list, generate_deserialize_statements, generate_field_list,
    get_annotated_function, typed_value_to_rust,
};
use crate::parse::node_lists::NodeList;
use crate::parse::nodes::{Node, NodeInstance};
use crate::{change_case, get_macro_item, IdentCase, SkyliteProcError};

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

fn properties_type_name(node_name: &str) -> Ident {
    format_ident!(
        "{}Properties",
        change_case(node_name, IdentCase::UpperCamelCase)
    )
}

fn gen_properties_type(node: &Node, items: &[Item]) -> Result<TokenStream, SkyliteProcError> {
    let node_param_list = generate_field_list(&node.parameters, TokenStream::new());
    let node_args = generate_argument_list(&node.parameters);
    let properties_type_name = properties_type_name(&node.meta.name);

    let asset_properties = generate_field_list(&node.properties, quote!(pub));
    let extra_properties = match get_macro_item("skylite_proc::extra_properties", items)? {
        Some(tokens) => tokens.clone(),
        None => TokenStream::new(),
    };
    let delimiter = if !extra_properties.is_empty() && !asset_properties.is_empty() {
        quote!(,)
    } else {
        TokenStream::new()
    };

    let create_properties_call = if !asset_properties.is_empty() || !extra_properties.is_empty() {
        get_annotated_function(items, "skylite_proc::create_properties")
            .map(get_fn_name)
            .map(|ident| quote!(#ident(#node_args)))
            .ok_or(data_err!("Missing required special function `create_properties`. Function is required because the node has properties."))?
    } else {
        quote!(#properties_type_name {})
    };

    Ok(quote! {
        pub struct #properties_type_name {
            #asset_properties
            #delimiter
            #extra_properties
        }

        impl #properties_type_name {
            fn _private_create_properties(#node_param_list) -> #properties_type_name {
                #create_properties_call
            }
        }
    })
}

fn static_nodes_type_name(node_name: &str) -> Ident {
    format_ident!(
        "{}StaticNodes",
        change_case(node_name, IdentCase::UpperCamelCase)
    )
}

fn gen_static_nodes_type(node: &Node) -> TokenStream {
    let static_nodes_type_name = static_nodes_type_name(&node.meta.name);
    let members = node.static_nodes.iter().map(|(name, instance)| {
        let member_name = format_ident!("{}", change_case(name, IdentCase::LowerSnakeCase));
        let node_type = format_ident!("{}", change_case(&instance.name, IdentCase::UpperCamelCase));
        quote!(#member_name: #node_type)
    });
    quote! {
        pub struct #static_nodes_type_name {
            #(pub #members),*
        }
    }
}

fn gen_node_type(node: &Node, project_name: &str) -> TokenStream {
    let node_name = node_type_name(&node.meta.name);
    let project_name = format_ident!("{}", change_case(project_name, IdentCase::UpperCamelCase));
    let properties_type_name = properties_type_name(&node.meta.name);
    let static_nodes_type_name = static_nodes_type_name(&node.meta.name);
    quote! {
        pub struct #node_name {
            pub properties: #properties_type_name,
            pub static_nodes: #static_nodes_type_name,
            pub dynamic_nodes: Vec<Box<dyn ::skylite_core::nodes::Node<P=#project_name>>>
        }
    }
}

fn gen_node_new_fn(node: &Node, project_name: &str, items: &[Item]) -> TokenStream {
    let node_name = node_type_name(&node.meta.name);
    let project_ident = format_ident!("{}", change_case(project_name, IdentCase::UpperCamelCase));
    let node_param_list = generate_field_list(&node.parameters, TokenStream::new());
    let node_args = generate_argument_list(&node.parameters);
    let properties_type_name = properties_type_name(&node.meta.name);
    let static_nodes_type_name = static_nodes_type_name(&node.meta.name);

    let static_nodes = node.static_nodes.iter().map(|(name, instance)| {
        let member_name = format_ident!("{}", change_case(name, IdentCase::LowerSnakeCase));
        let node_type = format_ident!("{}", change_case(&instance.name, IdentCase::UpperCamelCase));
        let args = instance
            .args
            .iter()
            .map(|arg| typed_value_to_rust(arg, project_name));
        quote!(#member_name: #node_type::new(#(#args),*))
    });

    let dynamic_nodes = node.dynamic_nodes.iter().map(|instance| {
        let node_type = format_ident!("{}", change_case(&instance.name, IdentCase::UpperCamelCase));
        let args = instance
            .args
            .iter()
            .map(|arg| typed_value_to_rust(arg, project_name));
        quote!(#node_type::new(#(#args),*))
    });

    let init_call = get_annotated_function(items, "skylite_proc::init")
        .map(get_fn_name)
        .map(|name| quote!(#name(&mut out);))
        .unwrap_or(TokenStream::new());

    quote! {
        pub fn new(#node_param_list) -> #node_name {
            let properties = #properties_type_name::_private_create_properties(#node_args);
            let static_nodes = #static_nodes_type_name {
                #(#static_nodes),*
            };
            let dynamic_nodes: Vec<Box<dyn ::skylite_core::nodes::Node<P=#project_ident>>> = vec! [
                #(Box::new(#dynamic_nodes)),*
            ];
            let mut out = #node_name {
                properties,
                static_nodes,
                dynamic_nodes
            };
            #init_call
            out
        }
    }
}

fn gen_node_impl(
    node: &Node,
    project_name: &str,
    items: &[Item],
) -> Result<TokenStream, SkyliteProcError> {
    let node_name = node_type_name(&node.meta.name);
    let project_name = format_ident!("{}", change_case(project_name, IdentCase::UpperCamelCase));
    let mut static_node_names: Vec<Ident> = node
        .static_nodes
        .iter()
        .map(|(n, _)| format_ident!("{}", change_case(n, IdentCase::LowerSnakeCase)))
        .collect();
    // Sub-nodes need to be added in reverse order to the NodeIterator in iter_nodes
    // and iter_nodes_mut, because NodeIterator again reverses the order.
    static_node_names.reverse();

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

    Ok(quote! {
        impl ::skylite_core::nodes::Node for #node_name {
            type P = #project_name;

            fn _private_decode(decoder: &mut dyn ::skylite_compress::Decoder) -> Self
            where
                Self: Sized
            {
                use ::skylite_core::decode::Deserialize;
                #decode_statements
                #node_name::new(#args)
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
                iter._private_push_sub_iterator(self.dynamic_nodes.get_iterator());
                #(
                    iter._private_push_single(&self.static_nodes.#static_node_names);
                )*
                iter
            }

            fn iter_nodes_mut<'node>(&'node mut self) -> ::skylite_core::nodes::NodeIteratorMut<'node, Self::P> {
                use ::skylite_core::nodes::NodeIterableMut;
                let mut iter = ::skylite_core::nodes::NodeIteratorMut::new();
                iter._private_push_sub_iterator(self.dynamic_nodes.get_iterator_mut());
                #(
                    iter._private_push_single(&mut self.static_nodes.#static_node_names);
                )*
                iter
            }
        }
    })
}

pub(crate) fn generate_node_definition(
    node: &Node,
    project_name: &str,
    items: &[Item],
) -> Result<TokenStream, SkyliteProcError> {
    let node_name = node_type_name(&node.meta.name);
    let properties_type = gen_properties_type(node, items)?;
    let static_nodes_type = gen_static_nodes_type(node);
    let node_type = gen_node_type(node, project_name);
    let node_new_fn = gen_node_new_fn(node, project_name, items);
    let node_impl = gen_node_impl(node, project_name, items)?;

    Ok(quote! {
        #properties_type

        #static_nodes_type

        #node_type

        impl #node_name {
            #node_new_fn
        }

        impl ::skylite_core::nodes::TypeId for #node_name {
            fn get_id() -> usize {
                <Self as ::skylite_core::nodes::TypeId>::get_id as usize
            }
        }

        #node_impl
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use quote::quote;
    use syn::{parse_quote, File, Item};

    use super::gen_node_new_fn;
    use crate::assets::{AssetMetaData, AssetSource, AssetType};
    use crate::generate::nodes::gen_node_impl;
    use crate::parse::nodes::{Node, NodeInstance};
    use crate::parse::values::{Type, TypedValue, Variable};

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
            static_nodes: vec![(
                "static1".to_owned(),
                NodeInstance {
                    node_id: 1,
                    name: "TestNode2".to_owned(),
                    args: vec![TypedValue::Bool(false)],
                },
            )],
            dynamic_nodes: vec![NodeInstance {
                node_id: 2,
                name: "TestNode2".to_owned(),
                args: vec![TypedValue::Bool(true)],
            }],
        }
    }

    fn create_test_items() -> Vec<Item> {
        let file: File = parse_quote! {
            skylite_proc::extra_properties! {
                pub extra: bool
            }

            #[skylite_proc::create_properties]
            fn create_properties(id: &str) -> BasicNode1Properties {
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
    fn test_node_new_fn() {
        let node = create_test_node();
        let items = create_test_items();

        let actual = gen_node_new_fn(&node, "TestProject", &items);
        let expected = quote! {
            pub fn new(param1: u8, param2: u16) -> TestNode {
                let properties = TestNodeProperties::_private_create_properties(param1, param2);
                let static_nodes = TestNodeStaticNodes {
                    static1: TestNode2::new(false)
                };
                let dynamic_nodes: Vec<Box<dyn ::skylite_core::nodes::Node<P=TestProject>>> = vec! [
                    Box::new(TestNode2::new(true))
                ];
                let mut out = TestNode {
                    properties,
                    static_nodes,
                    dynamic_nodes
                };
                init(&mut out);
                out
            }
        };

        assert_eq!(actual.to_string(), expected.to_string());
    }

    #[test]
    fn test_node_impl() {
        let node = create_test_node();
        let items = create_test_items();

        let actual = gen_node_impl(&node, "TestProject", &items).unwrap();
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
                    TestNode::new(param1, param2)
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
                    iter._private_push_sub_iterator(self.dynamic_nodes.get_iterator());
                    iter._private_push_single(&self.static_nodes.static1);
                    iter
                }

                fn iter_nodes_mut<'node>(&'node mut self) -> ::skylite_core::nodes::NodeIteratorMut<'node, Self::P> {
                    use ::skylite_core::nodes::NodeIterableMut;
                    let mut iter = ::skylite_core::nodes::NodeIteratorMut::new();
                    iter._private_push_sub_iterator(self.dynamic_nodes.get_iterator_mut());
                    iter._private_push_single(&mut self.static_nodes.static1);
                    iter
                }
            }
        };

        assert_eq!(actual.to_string(), expected.to_string());
    }
}
