use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Item, ItemFn};

use super::node_lists::generate_node_list_ids;
use crate::generate::node_lists::{
    generate_decode_node_list_fn, generate_node_list_data, node_list_ids_type,
};
use crate::generate::nodes::{generate_decode_node_fn, node_type_name};
use crate::generate::util::{get_annotated_function, typed_value_to_rust};
use crate::parse::nodes::NodeInstance;
use crate::parse::project::SkyliteProject;
use crate::parse::util::{change_case, IdentCase};
use crate::SkyliteProcError;

fn tile_type_name(project_name: &str) -> Ident {
    format_ident!(
        "{}Tiles",
        change_case(project_name, IdentCase::UpperCamelCase)
    )
}

fn generate_tile_type_enum<S: AsRef<str>>(project_name: &str, tile_types: &[S]) -> TokenStream {
    let tile_type_name = tile_type_name(project_name);
    let tile_types = tile_types.iter().map(|tt| {
        Ident::new(
            &change_case(tt.as_ref(), IdentCase::UpperCamelCase),
            Span::call_site(),
        )
    });
    quote! {
        #[derive(Clone, Copy)]
        pub enum #tile_type_name {
            #(#tile_types),*
        }
    }
}

pub(crate) fn project_ident(project_name: &str) -> Ident {
    format_ident!("{}", change_case(project_name, IdentCase::UpperCamelCase))
}

fn generate_project_type(project_name: &str, target_type: &TokenStream) -> TokenStream {
    let project_ident = project_ident(project_name);
    quote! {
        pub struct #project_ident {
            target: #target_type,
            root_node: ::std::boxed::Box<dyn ::skylite_core::nodes::Node<P=Self>>,
            graphics_cache: ::std::vec::Vec<::std::rc::Weak<u8>>,
            focus_x: i32,
            focus_y: i32
        }
    }
}

fn generate_project_new_method(
    project_name: &str,
    target_type: &TokenStream,
    init_call: &TokenStream,
    root_node: &NodeInstance,
) -> TokenStream {
    let project_ident = project_ident(project_name);
    let root_node_name = node_type_name(&root_node.name);
    let root_node_params = root_node
        .args
        .iter()
        .map(|arg| typed_value_to_rust(arg, project_name));
    quote! {
        fn new(target: #target_type) -> #project_ident {
            let (w, h) = target.get_screen_size();
            let mut out = #project_ident {
                target,
                root_node: ::std::boxed::Box::new(#root_node_name::new(#(#root_node_params),*)),
                graphics_cache: ::std::vec::Vec::new(),
                focus_x: w as i32 / 2,
                focus_y: h as i32 / 2
            };

            #init_call
            out
        }
    }
}

fn generate_project_impl(project_name: &str) -> TokenStream {
    let project_ident = project_ident(project_name);

    quote! {
        impl #project_ident {
            #[cfg(debug_assertions)]
            pub fn _private_target(&mut self) -> &mut <#project_ident as ::skylite_core::SkyliteProject>::Target {
                &mut self.target
            }
        }
    }
}

fn generate_project_trait_impl(
    project: &SkyliteProject,
    target_type: &TokenStream,
    items: &[Item],
) -> TokenStream {
    fn get_name(fun: &ItemFn) -> Ident {
        fun.sig.ident.clone()
    }

    let project_ident = project_ident(&project.name);
    let tile_type_name = tile_type_name(&project.name);
    let node_list_ids_type = node_list_ids_type(&project.name);

    let init = get_annotated_function(items, "skylite_proc::init")
        .map(get_name)
        .map(|name| quote!(#name(&mut out);))
        .unwrap_or(TokenStream::new());

    let pre_update = get_annotated_function(items, "skylite_proc::pre_update")
        .map(get_name)
        .map(|name| quote!(#name(&mut controls);))
        .unwrap_or(TokenStream::new());

    let post_update = get_annotated_function(items, "skylite_proc::post_update")
        .map(get_name)
        .map(|name| quote!(#name(&mut controls);))
        .unwrap_or(TokenStream::new());

    let pre_render = get_annotated_function(items, "skylite_proc::pre_render")
        .map(get_name)
        .map(|name| quote!(#name(&mut draw_context);))
        .unwrap_or(TokenStream::new());

    let post_render = get_annotated_function(items, "skylite_proc::post_render")
        .map(get_name)
        .map(|name| quote!(#name(&mut draw_context);))
        .unwrap_or(TokenStream::new());

    let new_method =
        generate_project_new_method(&project.name, target_type, &init, &project.root_node);
    let decode_node_fn =
        generate_decode_node_fn(&project.name, &project.nodes, &project.node_lists);
    let decode_node_list_fn = generate_decode_node_list_fn(&project.name);

    quote! {
        impl skylite_core::SkyliteProject for #project_ident {
            type Target = #target_type;
            type TileType = #tile_type_name;
            type NodeListIds = #node_list_ids_type;

            #new_method

            fn render(&mut self) {
                let mut draw_context = ::skylite_core::DrawContext {
                    target: &mut self.target,
                    graphics_cache: &mut self.graphics_cache,
                    focus_x: self.focus_x,
                    focus_y: self.focus_y
                };
                #pre_render

                // Main rendering
                ::skylite_core::nodes::_private::render_node(self.root_node.as_ref(), &mut draw_context);

                #post_render
            }

            fn update(&mut self) {
                let mut controls = ::skylite_core::ProjectControls {
                    target: &mut self.target,
                    pending_root_node: None
                };

                #pre_update

                // Main update
                self.root_node._private_update(&mut controls);

                #post_update

                if let Some(get_fn) = controls.pending_root_node.take() {
                    self.set_root_node(get_fn);
                }
            }

            fn set_root_node(&mut self, get_fn: Box<dyn FnOnce() -> Box<dyn ::skylite_core::nodes::Node<P=Self>>>) {
                ::skylite_core::nodes::_private::replace_node(get_fn, &mut self.root_node);
            }

            #decode_node_fn

            #decode_node_list_fn
        }
    }
}

impl SkyliteProject {
    pub(crate) fn generate(
        &self,
        target_type: &TokenStream,
        items: &[Item],
    ) -> Result<Vec<Item>, SkyliteProcError> {
        Ok(vec![
            Item::Verbatim(generate_tile_type_enum(&self.name, &self.tile_types)),
            Item::Verbatim(generate_node_list_data(&self.node_lists)),
            Item::Verbatim(generate_node_list_ids(&self.node_lists, &self.name)),
            Item::Verbatim(generate_project_type(&self.name, &target_type)),
            Item::Verbatim(generate_project_impl(&self.name)),
            Item::Verbatim(generate_project_trait_impl(self, &target_type, items)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::parse_quote;

    use super::generate_project_trait_impl;
    use crate::assets::tests::create_tmp_fs;
    use crate::{SkyliteProject, SkyliteProjectStub};

    #[test]
    fn test_generate_project_implementation() {
        let tmp_fs = create_tmp_fs(&[
            (
                "project.scm",
                r#"'((name . Test1) (root-node . (test-node #f 5)) (tile-types . (solid)))"#,
            ),
            (
                "nodes/test-node.scm",
                "'((parameters . ((p1 bool) (p2 u8))))",
            ),
        ])
        .unwrap();

        let body_parsed: syn::File = parse_quote! {
            #[skylite_proc::init]
            fn init(project: &mut Test1) {}

            #[skylite_proc::pre_update]
            fn pre_update(project: &mut Test1) {}

            #[skylite_proc::post_render]
            fn post_render(project: &mut skylite_core::DrawContext<'static, Test1>) {}
        };

        let project = SkyliteProject::from_stub(
            SkyliteProjectStub::from_file(&tmp_fs.path().join("project.scm")).unwrap(),
        )
        .unwrap();

        let actual = generate_project_trait_impl(&project, &quote!(MockTarget), &body_parsed.items);
        let expectation = quote! {
            impl skylite_core::SkyliteProject for Test1 {
                type Target = MockTarget;
                type TileType = Test1Tiles;
                type NodeListIds = Test1NodeListIds;

                fn new(target: MockTarget) -> Test1 {
                    let (w, h) = target.get_screen_size();
                    let mut out = Test1 {
                        target,
                        root_node: ::std::boxed::Box::new(TestNode::new(false, 5u8)),
                        graphics_cache: ::std::vec::Vec::new(),
                        focus_x: w as i32 / 2,
                        focus_y: h as i32 / 2
                    };

                    init(&mut out);
                    out
                }

                fn render(&mut self) {
                    let mut draw_context = ::skylite_core::DrawContext {
                        target: &mut self.target,
                        graphics_cache: &mut self.graphics_cache,
                        focus_x: self.focus_x,
                        focus_y: self.focus_y
                    };

                    ::skylite_core::nodes::_private::render_node(self.root_node.as_ref(), &mut draw_context);
                    post_render(&mut draw_context);
                }

                fn update(&mut self) {
                    let mut controls = ::skylite_core::ProjectControls {
                        target: &mut self.target,
                        pending_root_node: None
                    };

                    pre_update(&mut controls);

                    // Main update
                    self.root_node._private_update(&mut controls);

                    if let Some(get_fn) = controls.pending_root_node.take() {
                        self.set_root_node(get_fn);
                    }
                }

                fn set_root_node(&mut self, get_fn: Box<dyn FnOnce() -> Box<dyn ::skylite_core::nodes::Node<P=Self>>>) {
                    ::skylite_core::nodes::_private::replace_node(get_fn, &mut self.root_node);
                }

                fn _private_decode_node(decoder: &mut dyn ::skylite_compress::Decoder) -> Box<dyn ::skylite_core::nodes::Node<P=Test1>> {
                    use ::skylite_core::nodes::Node;
                    let id = ::skylite_core::decode::read_varint(decoder);
                    match id {
                        _ => unreachable!()
                    }
                }

                fn _private_decode_node_list(id: usize) -> ::skylite_core::nodes::NodeList<Test1> {
                    let data = crate::test1::gen::NODE_LIST_DATA[id as usize];
                    let mut decoder = ::skylite_compress::make_decoder(data);
                    let len = ::skylite_core::decode::read_varint(decoder.as_mut());
                    let nodes: Vec<Box<dyn ::skylite_core::nodes::Node<P=Test1>>> = (0 .. len)
                        .map(|_| Test1::_private_decode_node(decoder.as_mut()))
                        .collect();
                    ::skylite_core::nodes::NodeList::new(nodes)
                }
            }
        };
        assert_eq!(actual.to_string(), expectation.to_string());
    }
}
