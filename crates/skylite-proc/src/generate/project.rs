use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Item, ItemFn, ItemMod, Meta, Path as SynPath};

use crate::{parse::{project::SkyliteProject, util::{change_case, IdentCase}}, SkyliteProcError};

use super::actors::{actor_type_name, generate_actors_type};

fn get_annotated_function<'a>(items: &'a [Item], attribute: &str) -> Option<&'a ItemFn> {
    let attribute_path = syn::parse_str::<SynPath>(attribute).unwrap();
    items.iter()
        // Find item with matching attribute
        .find(|item| if let Item::Fn(fun) = item {
            fun.attrs.iter()
                .any(|attr| if let Meta::Path(ref p) = attr.meta { *p == attribute_path } else { false })
        } else {
            false
        })
        // Unpack generic item as function item
        .map(|item| if let Item::Fn(fun) = item {
            fun
        } else {
            panic!("Expected function item")
        })
}

fn generate_special_function_call(items: &[Item], attribute: &str, args: TokenStream) -> TokenStream {
    match get_annotated_function(items, attribute) {
        Some(fun) => {
            let ident = &fun.sig.ident;
            quote! { #ident(#args); }
        },
        None => TokenStream::new()
    }
}

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
            draw_context: skylite_core::DrawContext<#project_name>,
            // TODO
        }
    }
}

fn generate_project_new_method(project_ident: &Ident, target_type: &TokenStream, init_call: &TokenStream) -> TokenStream {
    quote! {
        fn new(target: #target_type) -> #project_ident {
            let (w, h) = target.get_screen_size();
            let mut out = #project_ident {
                draw_context: skylite_core::DrawContext {
                    target,
                    graphics_cache: Vec::new(),
                    focus_x: w as i32 / 2,
                    focus_y: h as i32 / 2
                }
            };

            #init_call
            out
        }
    }
}

fn generate_project_implementation(project_name: &str, target_type: &TokenStream, body: &ItemMod) -> TokenStream {
    let project_ident = Ident::new(&change_case(project_name, IdentCase::UpperCamelCase), Span::call_site());
    let tile_type_name = tile_type_name(project_name);
    let actor_type_name = actor_type_name(project_name);

    let items = &body.content.as_ref().unwrap().1;

    let init = generate_special_function_call(items, "skylite_proc::init", quote!(&mut out));
    let pre_update = generate_special_function_call(items, "skylite_proc::pre_update", quote!(self));
    let post_update = generate_special_function_call(items, "skylite_proc::post_update", quote!(self));
    let pre_render = generate_special_function_call(items, "skylite_proc::pre_render", quote!(&mut self.draw_context));
    let post_render = generate_special_function_call(items, "skylite_proc::post_render", quote!(&mut self.draw_context));

    let new_method = generate_project_new_method(&project_ident, target_type, &init);

    quote! {
        impl skylite_core::SkyliteProject for #project_ident {
            type Target = #target_type;
            type TileType = #tile_type_name;
            type Actors = #actor_type_name;

            #new_method

            fn render(&self) {
                #pre_render
                // Main rendering
                #post_render
            }

            fn update(&mut self) {
                #pre_update
                // Main update
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
            Item::Verbatim(generate_project_type(&self.name, &target_type)),
            Item::Verbatim(generate_project_implementation(&self.name, &target_type, body))
        ])
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::parse_quote;

    use super::generate_project_implementation;

    #[test]
    fn test_generate_project_implementation() {
        let actual = generate_project_implementation("Test1", &quote!(MockTarget), &parse_quote! {
            mod project {
                #[skylite_proc::init]
                fn init(project: &mut Test1) {}

                #[skylite_proc::pre_update]
                fn pre_update(project: &mut Test1) {}

                #[skylite_proc::post_render]
                fn post_render(project: &mut skylite_core::DrawContext<'static, Test1>) {}
            }
        });
        let expectation = concat! {
            "impl skylite_core :: SkyliteProject for Test1 { ",
                "type Target = MockTarget ; ",
                "type TileType = Test1Tiles ; ",
                "type Actors = Test1Actors ; ",

                "fn new (target : MockTarget) -> Test1 { ",
                    "let (w , h) = target . get_screen_size () ; ",
                    "let mut out = Test1 { ",
                        "draw_context : skylite_core :: DrawContext { ",
                            "target , ",
                            "graphics_cache : Vec :: new () , ",
                            "focus_x : w as i32 / 2 , ",
                            "focus_y : h as i32 / 2 ",
                        "} ",
                    "} ; ",

                    "init (& mut out) ; ",
                    "out ",
                "} ",

                "fn render (& self) { ",
                    "post_render (& mut self . draw_context) ; ",
                "} ",

                "fn update (& mut self) { ",
                    "pre_update (self) ; ",
                "} ",

            "}"
        };
        assert_eq!(actual.to_string(), expectation);
    }
}
