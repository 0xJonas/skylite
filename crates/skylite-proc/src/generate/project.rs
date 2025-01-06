use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Item, ItemFn};

use crate::{generate::{scenes::{generate_scene_decode_funs, scene_params_type_name, scene_type_name}, util::{get_annotated_function, typed_value_to_rust}}, parse::{project::SkyliteProject, scenes::SceneInstance, util::{change_case, IdentCase}}, SkyliteProcError};

use super::{actors::{any_actor_type_name, generate_actors_type}, scenes::{generate_scene_data, generate_scene_params_type}};

fn tile_type_name(project_name: &str) -> Ident {
    format_ident!("{}Tiles", change_case(project_name, IdentCase::UpperCamelCase))
}

fn generate_tile_type_enum<S: AsRef<str>>(project_name: &str, tile_types: &[S]) -> TokenStream {
    let tile_type_name = tile_type_name(project_name);
    let tile_types = tile_types.iter()
        .map(|tt| Ident::new(&change_case(tt.as_ref(), IdentCase::UpperCamelCase), Span::call_site()));
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

pub(crate) fn project_type_name(project_name: &str) -> TokenStream {
    let project_ident = project_ident(project_name);
    quote!(crate::#project_ident)
}

fn generate_project_type(project_name: &str, target_type: &TokenStream) -> TokenStream {
    let project_ident = project_ident(project_name);
    quote! {
        pub struct #project_ident {
            target: #target_type,
            scene: ::std::boxed::Box<dyn ::skylite_core::scenes::Scene<P=Self>>,
            graphics_cache: ::std::vec::Vec<::std::rc::Weak<u8>>,
            focus_x: i32,
            focus_y: i32
        }
    }
}

fn generate_project_new_method(project_name: &str, target_type: &TokenStream, init_call: &TokenStream, initial_scene: &SceneInstance) -> TokenStream {
    let project_ident = project_ident(project_name);
    let initial_scene_name = scene_type_name(&initial_scene.name);
    let initial_scene_params = initial_scene.args.iter().map(typed_value_to_rust);
    quote! {
        fn new(target: #target_type) -> #project_ident {
            let (w, h) = target.get_screen_size();
            let mut out = #project_ident {
                target,
                scene: ::std::boxed::Box::new(#initial_scene_name::new(#(#initial_scene_params),*)),
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
    let scene_decode_funs = generate_scene_decode_funs(project_name);
    let project_ident = project_ident(project_name);

    quote! {
        impl #project_ident {
            #scene_decode_funs

            #[cfg(debug_assertions)]
            pub fn _private_target(&mut self) -> &mut <#project_ident as ::skylite_core::SkyliteProject>::Target {
                &mut self.target
            }
        }
    }
}

fn generate_project_trait_impl(project_name: &str, target_type: &TokenStream, initial_scene: &SceneInstance, items: &[Item]) -> TokenStream {
    fn get_name(fun: &ItemFn) -> Ident { fun.sig.ident.clone() }

    let project_ident = project_ident(project_name);
    let tile_type_name = tile_type_name(project_name);
    let actors_type_name = any_actor_type_name(project_name);
    let scene_params_type_name = scene_params_type_name(project_name);

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

    let new_method = generate_project_new_method(project_name, target_type, &init, initial_scene);

    quote! {
        impl skylite_core::SkyliteProject for #project_ident {
            type Target = #target_type;
            type TileType = #tile_type_name;
            type Actors = #actors_type_name;
            type SceneParams = #scene_params_type_name;

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
                self.scene._private_render(&mut draw_context);

                #post_render
            }

            fn update(&mut self) {
                let mut controls = ::skylite_core::ProjectControls {
                    target: &mut self.target,
                    pending_scene: None
                };

                #pre_update

                // Main update
                self.scene._private_update(&mut controls);

                #post_update

                if let Some(params) = controls.pending_scene.take() {
                    self.set_scene(params);
                }
            }

            fn set_scene(&mut self, params: Self::SceneParams) {
                ::skylite_core::scenes::_private::replace_scene(params, &mut self.scene);
            }
        }
    }
}


impl SkyliteProject {

    pub(crate) fn generate(&self, target_type: &TokenStream, items: &[Item]) -> Result<Vec<Item>, SkyliteProcError> {
        Ok(vec![
            Item::Verbatim(generate_tile_type_enum(&self.name, &self.tile_types)),
            Item::Verbatim(generate_actors_type(&self.name, &self.actors)?),
            Item::Verbatim(generate_scene_data(&self.scenes, &self.actors)),
            Item::Verbatim(generate_scene_params_type(&self.name, &self.scenes)),
            Item::Verbatim(generate_project_type(&self.name, &target_type)),
            Item::Verbatim(generate_project_impl(&self.name)),
            Item::Verbatim(generate_project_trait_impl(&self.name, &target_type, &self.initial_scene, items))
        ])
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::parse_quote;

    use crate::parse::{scenes::SceneInstance, values::TypedValue};

    use super::generate_project_trait_impl;

    #[test]
    fn test_generate_project_implementation() {
        let body_parsed: syn::File = parse_quote! {
            #[skylite_proc::init]
            fn init(project: &mut Test1) {}

            #[skylite_proc::pre_update]
            fn pre_update(project: &mut Test1) {}

            #[skylite_proc::post_render]
            fn post_render(project: &mut skylite_core::DrawContext<'static, Test1>) {}
        };

        let actual = generate_project_trait_impl(
            "Test1",
            &quote!(MockTarget),
            &SceneInstance { name: "TestScene".to_owned(), args: vec![TypedValue::Bool(false), TypedValue::U8(5)]},
            &body_parsed.items
        );
        let expectation = quote! {
            impl skylite_core::SkyliteProject for Test1 {
                type Target = MockTarget;
                type TileType = Test1Tiles;
                type Actors = Test1Actors;

                fn new(target: MockTarget) -> Test1 {
                    let (w, h) = target.get_screen_size();
                    let mut out = Test1 {
                        draw_context: skylite_core::DrawContext {
                            target,
                            graphics_cache: Vec::new(),
                            focus_x: w as i32 / 2,
                            focus_y: h as i32 / 2
                        },
                        scene: ::std::boxed::Box::new(TestScene::new(false, 5u8)),
                        controls: ::skylite_core::ProjectControls { pending_scene: None }
                    };
                    init(&mut out);
                    out
                }

                fn render(&mut self) {
                    ::skylite_core::scenes::_private::render_scene(self.scene.as_ref(), &mut self.draw_context);
                    post_render(&mut self.draw_context);
                }

                fn update(&mut self) {
                    if let Some(scene) = self.controls.pending_scene.take() {
                        self.scene = scene;
                    }

                    pre_update(self);
                    self.scene._private_update(&mut self.controls);
                }
            }
        };
        assert_eq!(actual.to_string(), expectation.to_string());
    }
}
