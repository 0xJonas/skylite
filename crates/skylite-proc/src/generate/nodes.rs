use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, Item, ItemFn};

use super::util::{generate_deserialize_statements, generate_member_list};
use crate::generate::util::{
    generate_argument_list, generate_param_list, get_annotated_function, typed_value_to_rust,
};
use crate::parse::nodes::Node;
use crate::{change_case, get_macro_item, IdentCase, SkyliteProcError};

pub fn node_type_name(name: &str) -> Ident {
    format_ident!("{}", change_case(name, IdentCase::UpperCamelCase))
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
    let node_param_list = generate_param_list(&node.parameters);
    let node_args = generate_argument_list(&node.parameters);
    let properties_type_name = properties_type_name(&node.name);

    let asset_properties = generate_member_list(&node.properties, quote!(pub));
    let extra_properties = match get_macro_item("skylite_proc::extra_properties", items)? {
        Some(tokens) => tokens.clone(),
        None => TokenStream::new(),
    };

    let create_properties_call = if !asset_properties.is_empty() || !extra_properties.is_empty() {
        get_annotated_function(items, "skylite_proc::create_properties")
            .map(get_fn_name)
            .map(|ident| quote!(super::#ident(#node_args)))
            .ok_or(SkyliteProcError::DataError(format!("Missing required special function `create_properties`. Function is required because the node has properties.")))?
    } else {
        quote!(super::#properties_type_name {})
    };

    Ok(quote! {
        pub struct #properties_type_name {
            #asset_properties,
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
    let static_nodes_type_name = static_nodes_type_name(&node.name);
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
    let node_name = node_type_name(&node.name);
    let project_name = format_ident!("{}", change_case(project_name, IdentCase::UpperCamelCase));
    let properties_type_name = properties_type_name(&node.name);
    let static_nodes_type_name = static_nodes_type_name(&node.name);
    quote! {
        pub struct #node_name {
            pub properties: #properties_type_name,
            pub static_nodes: #static_nodes_type_name,
            pub dynamic_nodes: Vec<Box<dyn ::skylite_core::nodes::Node<P=#project_name>>>
        }
    }
}

fn gen_node_new_fn(node: &Node, project_name: &str, items: &[Item]) -> TokenStream {
    let node_name = node_type_name(&node.name);
    let project_name = format_ident!("{}", change_case(project_name, IdentCase::UpperCamelCase));
    let node_param_list = generate_param_list(&node.parameters);
    let node_args = generate_argument_list(&node.parameters);
    let properties_type_name = properties_type_name(&node.name);
    let static_nodes_type_name = static_nodes_type_name(&node.name);

    let static_nodes = node.static_nodes.iter().map(|(name, instance)| {
        let member_name = format_ident!("{}", change_case(name, IdentCase::LowerSnakeCase));
        let node_type = format_ident!("{}", change_case(&instance.name, IdentCase::UpperCamelCase));
        let args = instance.args.iter().map(typed_value_to_rust);
        quote!(#member_name: #node_type::new(#(#args),*))
    });

    let dynamic_nodes = node.dynamic_nodes.iter().map(|instance| {
        let node_type = format_ident!("{}", change_case(&instance.name, IdentCase::UpperCamelCase));
        let args = instance.args.iter().map(typed_value_to_rust);
        quote!(#node_type::new(#(#args),*))
    });

    let init_call = get_annotated_function(items, "skylite_proc::init")
        .map(get_fn_name)
        .map(|name| quote!(super::#name(&mut out, #node_args);))
        .unwrap_or(TokenStream::new());

    quote! {
        pub fn new(#node_param_list) -> #node_name {
            let properties = #properties_type_name::_private_create_properties(#node_args);
            let static_nodes = #static_nodes_type_name {
                #(#static_nodes),*
            };
            let dynamic_nodes: Vec<Box<dyn ::skylite_core::nodes::Node<P=#project_name>>> = vec! [
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
    let node_name = node_type_name(&node.name);
    let project_name = format_ident!("{}", change_case(project_name, IdentCase::UpperCamelCase));
    let static_node_names: Vec<Ident> = node
        .static_nodes
        .iter()
        .map(|(n, _)| format_ident!("{}", change_case(n, IdentCase::LowerSnakeCase)))
        .collect();

    let decode_statements = generate_deserialize_statements(&node.parameters);
    let args = generate_argument_list(&node.parameters);

    let pre_update_call = get_annotated_function(items, "skylite_proc::pre_update")
        .map(get_fn_name)
        .map_or(
            TokenStream::new(),
            |item| quote!(super::#item(self, controls)),
        );

    let update_call_opt = get_annotated_function(items, "skylite_proc::update")
        .map(get_fn_name)
        .map(|item| quote!(super::#item(self, controls)));

    let post_update_call_opt = get_annotated_function(items, "skylite_proc::post_update")
        .map(get_fn_name)
        .map(|item| quote!(super::#item(self, controls)));

    if update_call_opt.is_some() && post_update_call_opt.is_some() {
        return Err(SkyliteProcError::DataError(format!("skylite_proc::update and skylite_proc::post_update have the same meaning, only one must be given.")));
    }

    let post_update_call = update_call_opt.or(post_update_call_opt).unwrap_or_default();

    let render_call_opt = get_annotated_function(items, "skylite_proc::render")
        .map(get_fn_name)
        .map(|item| quote!(super::#item(self, ctx)));

    let is_visible_call = get_annotated_function(items, "skylite_proc::is_visible")
        .map(get_fn_name)
        .map_or(
            if render_call_opt.is_some() {
                quote!(true)
            } else {
                quote!(false)
            },
            |item| quote!(super::#item(self, ctx)),
        );

    let render_call = render_call_opt.unwrap_or_default();

    let z_order_call = get_annotated_function(items, "skylite_proc::z_order")
        .map(get_fn_name)
        .map_or(quote!(1), |item| quote!(super::#item(self)));

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

            fn _private_render(&self, ctx: &mut ::skylite_core::DrawContext<Self::P>) {
                #render_call;
            }

            fn z_order(&self) -> i32 {
                #z_order_call
            }

            fn is_visible(&self, ctx: &::skylite_core::DrawContext<Self::P>) -> bool {
                #is_visible_call
            }

            fn get_static_nodes(&self) -> Box<[&dyn ::skylite_core::nodes::Node<P = Self::P>]> {
                let out: Vec<&dyn ::skylite_core::nodes::Node<P = Self::P>> = vec![
                    #(&self.static_nodes.#static_node_names),*
                ];
                out.into_boxed_slice()
            }

            fn get_dynamic_nodes(&self) -> &Vec<Box<dyn ::skylite_core::nodes::Node<P = Self::P>>> {
                &self.dynamic_nodes
            }

            fn get_static_nodes_mut(&mut self) -> Box<[&mut dyn ::skylite_core::nodes::Node<P = Self::P>]> {
                let out: Vec<&mut dyn ::skylite_core::nodes::Node<P = Self::P>> = vec![
                    #(&mut self.static_nodes.#static_node_names),*
                ];
                out.into_boxed_slice()
            }

            fn get_dynamic_nodes_mut(&mut self) -> &mut Vec<Box<dyn ::skylite_core::nodes::Node<P = Self::P>>> {
                &mut self.dynamic_nodes
            }
        }
    })
}

pub(crate) fn generate_node_definition(
    node: &Node,
    node_id: usize,
    project_name: &str,
    items: &[Item],
    body_raw: &TokenStream,
) -> Result<TokenStream, SkyliteProcError> {
    let node_module_name = format_ident!("{}", change_case(&node.name, IdentCase::LowerSnakeCase));

    let imports = items.iter().filter_map(|item| {
        if let Item::Use(import) = item {
            Some(import.to_owned())
        } else {
            None
        }
    });

    let node_name = node_type_name(&node.name);
    let properties_type = gen_properties_type(node, items)?;
    let static_nodes_type = gen_static_nodes_type(node);
    let node_type = gen_node_type(node, project_name);
    let node_new_fn = gen_node_new_fn(node, project_name, items);
    let node_impl = gen_node_impl(node, project_name, items)?;

    // This module arrangement has the following goals:
    // - Non-public members inside the generated Node type are not accessible by
    //   user code, since they are only visible in the gen module.
    // - The node_definition! macro opens a new scope, so that multiple
    //   node_definitions in the same enclosing module can use the same name for
    //   their callbacks.
    Ok(quote! {
        mod #node_module_name {
            mod gen {
                #![allow(unused_imports)]
                #(
                    #imports
                )*

                #properties_type

                #static_nodes_type

                #node_type

                impl #node_name {
                    #node_new_fn
                }

                impl ::skylite_core::nodes::TypeId for #node_name {
                    fn get_id() -> usize {
                        #node_id
                    }
                }

                #node_impl
            }

            pub use gen::*;

            #body_raw
        }

        pub use #node_module_name::*;
    })
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::{parse_quote, File, Item};

    use super::gen_node_new_fn;
    use crate::generate::nodes::gen_node_impl;
    use crate::parse::nodes::NodeInstance;
    use crate::parse::values::{Type, TypedValue, Variable};
    use crate::Node;

    fn create_test_node() -> Node {
        Node {
            name: "TestNode".to_owned(),
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
                    name: "TestNode2".to_owned(),
                    args: vec![TypedValue::Bool(false)],
                },
            )],
            dynamic_nodes: vec![NodeInstance {
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
            fn render(&self, ctx: &mut DrawContext<MyProject>) {}
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
                super::init(&mut out, param1, param2);
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
                    super::pre_update(self, controls);

                    ::skylite_core::nodes::_private::update_node_rec(self, controls);

                    super::update(self, controls);
                }

                fn _private_render(&self, ctx: &mut ::skylite_core::DrawContext<Self::P>) {
                    super::render(self, ctx);
                }

                fn z_order(&self) -> i32 {
                    1
                }

                fn is_visible(&self, ctx: &::skylite_core::DrawContext<Self::P>) -> bool {
                    true
                }

                fn get_static_nodes(&self) -> Box<[&dyn ::skylite_core::nodes::Node<P = Self::P>]> {
                    let out: Vec<&dyn ::skylite_core::nodes::Node<P = Self::P>> = vec![
                        &self.static_nodes.static1
                    ];
                    out.into_boxed_slice()
                }

                fn get_dynamic_nodes(&self) -> &Vec<Box<dyn ::skylite_core::nodes::Node<P = Self::P>>> {
                    &self.dynamic_nodes
                }

                fn get_static_nodes_mut(&mut self) -> Box<[&mut dyn ::skylite_core::nodes::Node<P = Self::P>]> {
                    let out: Vec<&mut dyn ::skylite_core::nodes::Node<P = Self::P>> = vec![
                        &mut self.static_nodes.static1
                    ];
                    out.into_boxed_slice()
                }

                fn get_dynamic_nodes_mut(&mut self) -> &mut Vec<Box<dyn ::skylite_core::nodes::Node<P = Self::P>>> {
                    &mut self.dynamic_nodes
                }
            }
        };

        assert_eq!(actual.to_string(), expected.to_string());
    }
}
