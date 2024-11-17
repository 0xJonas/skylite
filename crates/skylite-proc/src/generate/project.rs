use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Item, ItemFn, ItemMod};

use crate::{generate::util::{get_annotated_function, typed_value_to_rust}, parse::{project::SkyliteProject, scenes::SceneInstance, util::{change_case, IdentCase}}, SkyliteProcError};

use super::{actors::{any_actor_type_name, generate_actors_type}, scenes::generate_scenes_type};

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

fn generate_project_type(project_name: &str, target_type: &TokenStream) -> TokenStream {
    let project_name = Ident::new(&change_case(project_name, IdentCase::UpperCamelCase), Span::call_site());
    quote! {
        pub struct #project_name {
            draw_context: ::skylite_core::DrawContext<#project_name>,
            scene: ::std::boxed::Box<dyn ::skylite_core::scenes::Scene<P=Self>>,
            controls: ::skylite_core::ProjectControls<#project_name>
            // TODO
        }
    }
}

fn generate_project_new_method(project_ident: &Ident, target_type: &TokenStream, init_call: &TokenStream, initial_scene: &SceneInstance) -> TokenStream {
    let initial_scene_name = format_ident!("{}", initial_scene.name);
    let initial_scene_params = initial_scene.args.iter().map(typed_value_to_rust);
    quote! {
        fn new(target: #target_type) -> #project_ident {
            let (w, h) = target.get_screen_size();
            let mut out = #project_ident {
                draw_context: skylite_core::DrawContext {
                    target,
                    graphics_cache: Vec::new(),
                    focus_x: w as i32 / 2,
                    focus_y: h as i32 / 2
                },
                scene: ::std::boxed::Box::new(#initial_scene_name::create_new(#(#initial_scene_params),*)),
                controls: ::skylite_core::ProjectControls { pending_scene: None }
            };

            #init_call
            out
        }
    }
}

fn generate_project_implementation(project_name: &str, target_type: &TokenStream, initial_scene: &SceneInstance, body: &ItemMod) -> TokenStream {
    fn get_name(fun: &ItemFn) -> Ident { fun.sig.ident.clone() }

    let project_ident = Ident::new(&change_case(project_name, IdentCase::UpperCamelCase), Span::call_site());
    let tile_type_name = tile_type_name(project_name);
    let actors_type_name = any_actor_type_name(project_name);

    let items = &body.content.as_ref().unwrap().1;

    let init = get_annotated_function(items, "skylite_proc::init")
        .map(get_name)
        .map(|name| quote!(#name(&mut out);))
        .unwrap_or(TokenStream::new());

    let pre_update = get_annotated_function(items, "skylite_proc::pre_update")
        .map(get_name)
        .map(|name| quote!(#name(self);))
        .unwrap_or(TokenStream::new());

    let post_update = get_annotated_function(items, "skylite_proc::post_update")
        .map(get_name)
        .map(|name| quote!(#name(self);))
        .unwrap_or(TokenStream::new());

    let pre_render = get_annotated_function(items, "skylite_proc::pre_render")
        .map(get_name)
        .map(|name| quote!(#name(&mut self.draw_context);))
        .unwrap_or(TokenStream::new());

    let post_render = get_annotated_function(items, "skylite_proc::post_render")
        .map(get_name)
        .map(|name| quote!(#name(&mut self.draw_context);))
        .unwrap_or(TokenStream::new());

    let new_method = generate_project_new_method(&project_ident, target_type, &init, initial_scene);

    quote! {
        impl skylite_core::SkyliteProject for #project_ident {
            type Target = #target_type;
            type TileType = #tile_type_name;
            type Actors = #actors_type_name;

            #new_method

            fn render(&mut self) {
                #pre_render

                // Main rendering
                ::skylite_core::scenes::_private::render_scene(self.scene.as_ref(), &mut self.draw_context);

                #post_render
            }

            fn update(&mut self) {
                if let Some(scene) = self.controls.pending_scene.take() {
                    self.scene = scene;
                }

                #pre_update

                // Main update
                ::skylite_core::scenes::_private::update_scene(self.scene.as_mut(), &mut self.controls);

                #post_update
            }
        }
    }
}


impl SkyliteProject {

    pub(crate) fn generate(&self, target_type: &TokenStream, body: &ItemMod) -> Result<Vec<Item>, SkyliteProcError> {
        Ok(vec![
            Item::Verbatim(generate_tile_type_enum(&self.name, &self.tile_types)),
            Item::Verbatim(generate_actors_type(&self.name, &self.actors)?),
            Item::Verbatim(generate_scenes_type(&self.name, &self.scenes, &self.actors)),
            Item::Verbatim(generate_project_type(&self.name, &target_type)),
            Item::Verbatim(generate_project_implementation(&self.name, &target_type, &self.initial_scene, body))
        ])
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::parse_quote;

    use crate::parse::{scenes::SceneInstance, values::TypedValue};

    use super::generate_project_implementation;

    #[test]
    fn test_generate_project_implementation() {
        let actual = generate_project_implementation(
            "Test1",
            &quote!(MockTarget),
            &SceneInstance { name: "TestScene".to_owned(), args: vec![TypedValue::Bool(false), TypedValue::U8(5)]},
            &parse_quote! {
                mod project {
                    #[skylite_proc::init]
                    fn init(project: &mut Test1) {}

                    #[skylite_proc::pre_update]
                    fn pre_update(project: &mut Test1) {}

                    #[skylite_proc::post_render]
                    fn post_render(project: &mut skylite_core::DrawContext<'static, Test1>) {}
                }
            }
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
                        scene: ::std::boxed::Box::new(TestScene::create_new(false, 5u8)),
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
                    ::skylite_core::scenes::_private::update_scene(self.scene.as_mut(), &mut self.controls);
                }
            }
        };
        assert_eq!(actual.to_string(), expectation.to_string());
    }
}
