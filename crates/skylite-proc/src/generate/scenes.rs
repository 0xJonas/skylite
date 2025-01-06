use quote::{format_ident, quote};
use syn::{Item, ItemFn};
use std::collections::HashMap;

use proc_macro2::{Literal, TokenStream, Ident};

use crate::{parse::{actors::Actor, scenes::{Scene, SceneStub}, util::{change_case, IdentCase}, values::Variable}, SkyliteProcError};

use super::{actors::any_actor_type_name, encode::{CompressionBuffer, Serialize}, project::project_type_name, util::{generate_param_list, get_annotated_function, get_macro_item, skylite_type_to_rust}};

fn get_parameter_name(var: &Variable) -> Ident { format_ident!("{}", change_case(&var.name, IdentCase::LowerSnakeCase)) }

// region: skylite_project stuff

pub(crate) fn scene_params_type_name(project_name: &str) -> Ident { format_ident!("{}SceneParams", change_case(project_name, IdentCase::UpperCamelCase)) }

fn encode_scene(scene: &Scene, actor_ids: &HashMap<String, usize>, buffer: &mut CompressionBuffer) {
    buffer.write_varint(scene.actors.len());
    for a in &scene.actors {
        buffer.write_varint(*actor_ids.get(&a.1.actor_name).unwrap());
        for p in &a.1.args {
            p.serialize(buffer);
        }
    }

    buffer.write_varint(scene.extras.len());
    for e in &scene.extras {
        buffer.write_varint(*actor_ids.get(&e.actor_name).unwrap());
        for p in &e.args {
            p.serialize(buffer);
        }
    }
}

pub(crate) fn generate_scene_data(scenes: &[Scene], actors: &[Actor]) -> TokenStream {
    let actor_ids = actors.iter()
        .enumerate()
        .map(|(i, actor)| (actor.name.clone(), i))
        .collect::<HashMap<String, usize>>();
    let mut scene_buffer = CompressionBuffer::new();
    let offsets = scenes.iter()
        .map(|s| {
            let out = scene_buffer.len();
            encode_scene(s, &actor_ids, &mut scene_buffer);
            out
        })
        .map(|offset| Literal::usize_unsuffixed(offset))
        .collect::<Vec<Literal>>();

    let scene_data = scene_buffer.encode()
        .into_iter()
        .map(|b| Literal::u8_unsuffixed(b));

    quote! {
        static SCENE_DATA: &[u8] = &[#(#scene_data),*];
        static SCENE_OFFSETS: &[usize] = &[#(#offsets),*];
    }
}

pub(crate) fn generate_scene_decode_funs(project_name: &str) -> TokenStream {
    let any_actor_type_name = any_actor_type_name(project_name);

    quote! {
        pub fn _private_get_decoder_for_scene(id: u32) -> ::std::boxed::Box<dyn ::skylite_compress::Decoder> {
            let mut out = ::skylite_compress::make_decoder(SCENE_DATA);
            for _ in 0..SCENE_OFFSETS[id as usize] { out.decode_u8(); }
            out
        }

        pub fn _private_decode_actor_list(decoder: &mut dyn ::skylite_compress::Decoder) -> Vec<#any_actor_type_name> {
            use ::skylite_core::actors::ActorBase;
            let len = ::skylite_core::decode::read_varint(decoder);
            (0..len).map(|_| #any_actor_type_name::_private_decode(decoder)).collect()
        }
    }
}

pub(crate) fn generate_scene_params_type(project_name: &str, scenes: &[Scene]) -> TokenStream {
    let project_ident = format_ident!("{}", change_case(project_name, IdentCase::UpperCamelCase));
    let scenes_type_name = scene_params_type_name(project_name);
    let scene_names = scenes.iter()
        .map(|s| format_ident!("{}", change_case(&s.name, IdentCase::UpperCamelCase)))
        .collect::<Vec<_>>();
    let param_lists = scenes.iter().map(|s| generate_param_list(&s.parameters));
    let param_names = scenes.iter().map(|s| {
            let names = s.parameters.iter().map(get_parameter_name);
            quote!(#(#names),*)
        })
        .collect::<Vec<_>>();

    quote! {
        pub enum #scenes_type_name {
            #(
                #scene_names { #param_lists },
            )
            *
        }

        impl ::skylite_core::scenes::SceneParams for #scenes_type_name {
            type P = #project_ident;

            fn load(self) -> Box<dyn ::skylite_core::scenes::Scene<P=Self::P>> {
                match self {
                    #(
                        #scenes_type_name::#scene_names { #param_names } => Box::new(#scene_names::new(#param_names))
                    ),*
                }
            }
        }
    }
}

// endregion

pub(crate) fn scene_type_name(name: &str) -> Ident { format_ident!("{}", change_case(name, IdentCase::UpperCamelCase)) }

fn gen_named_actors_type(scene: &SceneStub) -> TokenStream {
    let typename = format_ident!("{}Actors", change_case(&scene.name, IdentCase::UpperCamelCase));
    let actor_names = scene.actor_names.iter().map(|name| format_ident!("{}", change_case(name, IdentCase::UpperCamelCase)));

    // Only use repr(usize) when there are actually named actors in the scene,
    // since it does not work on empty enums. The type should still be generated,
    // even when it is empty.
    let repr = if scene.actor_names.len() > 0 {
        quote!(#[repr(usize)])
    } else {
        TokenStream::new()
    };
    quote! {
        #repr
        pub enum #typename {
            #(#actor_names),*
        }
    }
}

fn properties_type_name(name: &str) -> Ident { format_ident!("{}Properties", change_case(name, IdentCase::UpperCamelCase)) }

fn gen_properties_type(scene: &SceneStub, items: &[Item]) -> Result<TokenStream, SkyliteProcError> {
    let scene_param_list = generate_param_list(&scene.parameters);
    let scene_param_names: Vec<Ident> = scene.parameters.iter().map(get_parameter_name).collect();
    let properties_type_name = properties_type_name(&scene.name);

    // The properties are copied directly from the `skylite_proc::properties!` function macro.
    let properties = match get_macro_item("skylite_proc::properties", items)? {
        Some(tokens) => tokens.clone(),
        None => TokenStream::new()
    };

    // For an actor that has neither parameters nor properties, the create_properties special function is optional.
    let create_properties_call = if scene.parameters.len() > 0 || !properties.is_empty() {
        match get_annotated_function(items, "skylite_proc::create_properties") {
            Some(fun) => {
                let ident = &fun.sig.ident;
                quote! { super::#ident(#(#scene_param_names),*) }
            },
            None => return Err(SkyliteProcError::DataError(format!("Missing required special function `create_properties`. Function is required because the actor has parameters or properties.")))
        }
    } else {
        quote!(#properties_type_name {})
    };

    Ok(quote! {
        pub struct #properties_type_name {
            #properties
        }

        impl #properties_type_name {
            fn _private_create_properties(#scene_param_list) -> #properties_type_name {
                #create_properties_call
            }
        }
    })
}

fn gen_scene_type(scene: &SceneStub, type_id: u32, project_name: &str, items: &[Item]) -> Result<TokenStream, SkyliteProcError> {
    let type_name = scene_type_name(&scene.name);
    let properties_type_name = properties_type_name(&scene.name);
    let project_type_name = project_type_name(project_name);
    let any_actor_type = quote!(<#project_type_name as ::skylite_core::SkyliteProject>::Actors);
    let scene_param_list = generate_param_list(&scene.parameters);
    let scene_param_names: Vec<Ident> = scene.parameters.iter().map(get_parameter_name).collect();
    let init_call = match get_annotated_function(items, "skylite_proc::init") {
        Some(ident) => quote!(#ident(&mut out, #(#scene_param_names),*);),
        None => TokenStream::new()
    };

    Ok(quote! {
        pub struct #type_name {
            pub properties: #properties_type_name,
            actors: Vec<#any_actor_type>,
            extras: Vec<#any_actor_type>,
            remove_extra: bool,
        }

        impl #type_name {
            pub fn new(#scene_param_list) -> #type_name {
                let mut decoder = #project_type_name::_private_get_decoder_for_scene(#type_id);
                let actors = #project_type_name::_private_decode_actor_list(decoder.as_mut());
                let extras = #project_type_name::_private_decode_actor_list(decoder.as_mut());
                let mut out = #type_name {
                    // Clone arguments here, because they are also used for init_call
                    properties: #properties_type_name::_private_create_properties(#(#scene_param_names.clone()),*),
                    actors,
                    extras,
                    remove_extra: false
                };
                #init_call
                out
            }
        }
    })
}

fn gen_scene_decode_fn(scene_type_name: &Ident, params: &[Variable]) -> TokenStream {
    let scene_param_names: Vec<Ident> = params.iter().map(get_parameter_name).collect();
    let scene_args_decoders = params.iter()
        .map(|p| {
            let t = skylite_type_to_rust(&p.typename);
            quote!(#t::deserialize(decoder))
        });

    quote! {
        fn _private_decode(decoder: &mut dyn ::skylite_compress::Decoder) -> #scene_type_name {
            use ::skylite_core::decode::Deserialize;
            #(
                let #scene_param_names = #scene_args_decoders;
            )*
            #scene_type_name::new(#(#scene_param_names),*)
        }
    }
}

fn gen_scene_trait_impl(scene: &SceneStub, project_type_name: &TokenStream, items: &[Item]) -> Result<TokenStream, SkyliteProcError> {
    fn get_name(fun: &ItemFn) -> Ident { fun.sig.ident.clone() }

    let scene_type_name = scene_type_name(&scene.name);

    let decode_fn = gen_scene_decode_fn(&scene_type_name, &scene.parameters);

    let pre_update = get_annotated_function(items, "skylite_proc::pre_update")
        .map(get_name)
        .map(|name| quote!(super::#name(self, controls);))
        .unwrap_or(TokenStream::new());

    let post_update = get_annotated_function(items, "skylite_proc::post_update")
        .map(get_name)
        .map(|name| quote!(super::#name(self, controls);))
        .unwrap_or(TokenStream::new());

    let pre_render = get_annotated_function(items, "skylite_proc::pre_render")
        .map(get_name)
        .map(|name| quote!(super::#name(self, ctx);))
        .unwrap_or(TokenStream::new());

    let post_render = get_annotated_function(items, "skylite_proc::post_render")
        .map(get_name)
        .map(|name| quote!(super::#name(self, ctx);))
        .unwrap_or(TokenStream::new());

    Ok(quote! {
        impl ::skylite_core::scenes::Scene for #scene_type_name {
            type P = #project_type_name;

            #decode_fn

            fn _private_update(&mut self, controls: &mut ::skylite_core::ProjectControls<Self::P>) {
                use ::skylite_core::actors::ActorBase;

                #pre_update

                // We need to take the lists of actors and scenes out of the scene here,
                // to pass the borrow checks. After each actor and extra is updated, the
                // lists are restored.
                let mut actors = ::std::mem::take(&mut self.actors);
                let mut extras = ::std::mem::take(&mut self.extras);

                actors.iter_mut().for_each(|a| a._private_update(self, controls));
                self.actors = actors;

                extras = extras.into_iter().filter_map(|mut e| {
                        self.remove_extra = false;
                        e._private_update(self, controls);
                        if !self.remove_extra {
                            Some(e)
                        } else {
                            None
                        }
                    })
                    .collect();

                // Between taking the extras at the beginning of the update
                // and putting them back here, any of the update calls may
                // have added new extras. These have to go at the end of the list.
                ::std::mem::swap(&mut self.extras, &mut extras);
                self.extras.append(&mut extras);

                #post_update
            }

            fn _private_render(&self, ctx: &mut ::skylite_core::DrawContext<Self::P>) {
                #pre_render
                ::skylite_core::scenes::_private::render_scene(self, ctx);
                #post_render
            }

            fn iter_actors(&self, which: ::skylite_core::scenes::IterActors) -> ::skylite_core::scenes::ActorIterator<<Self::P as ::skylite_core::SkyliteProject>::Actors> {
                use ::skylite_core::scenes::IterActors;
                match which {
                    IterActors::Named => ::skylite_core::scenes::ActorIterator::_private_new(&self.actors, &[]),
                    IterActors::Extra => ::skylite_core::scenes::ActorIterator::_private_new(&[], &self.extras),
                    IterActors::All => ::skylite_core::scenes::ActorIterator::_private_new(&self.actors, &self.extras)
                }
            }

            fn iter_actors_mut(&mut self, which: ::skylite_core::scenes::IterActors) -> ::skylite_core::scenes::ActorIteratorMut<<Self::P as ::skylite_core::SkyliteProject>::Actors> {
                use ::skylite_core::scenes::IterActors;
                match which {
                    IterActors::Named => ::skylite_core::scenes::ActorIteratorMut::_private_new(self.actors.as_mut_slice(), &mut []),
                    IterActors::Extra => ::skylite_core::scenes::ActorIteratorMut::_private_new(&mut [], self.extras.as_mut_slice()),
                    IterActors::All => ::skylite_core::scenes::ActorIteratorMut::_private_new(self.actors.as_mut_slice(), self.extras.as_mut_slice())
                }
            }

            fn add_extra(&mut self, extra: <Self::P as ::skylite_core::SkyliteProject>::Actors) {
                self.extras.push(extra);
            }

            fn remove_current_extra(&mut self) { self.remove_extra = true; }
        }
    })
}

pub(crate) fn generate_scene_definition(scene: &SceneStub, type_id: u32, items: &[Item], project_name: &str, body_raw: &TokenStream) -> Result<TokenStream, SkyliteProcError> {
    let project_type_name = project_type_name(project_name);
    let scene_module_name = format_ident!("{}", change_case(&scene.name, IdentCase::LowerSnakeCase));
    let named_actors_type = gen_named_actors_type(scene);
    let properties_type = gen_properties_type(scene, items)?;
    let scene_type = gen_scene_type(scene, type_id, project_name, items)?;
    let scene_trait_impl = gen_scene_trait_impl(scene, &project_type_name, items)?;

    let imports = items.iter().filter_map(|item| if let Item::Use(import) = item {
        Some(import.to_owned())
    } else {
        None
    });

    Ok(quote! {
        mod #scene_module_name {
            pub mod gen {
                #[allow(unused_imports)]
                #(
                    #imports
                )
                *
                use super::*;

                #named_actors_type

                #properties_type

                #scene_type

                #scene_trait_impl
            }

            use gen::*;

            #body_raw
        }

        pub use #scene_module_name::gen::*;

    })
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::{parse2, File, Item};

    use crate::parse::{scenes::SceneStub, values::{Type, TypedValue}};

    use super::{gen_scene_trait_impl, Variable};

    fn create_test_scene() -> SceneStub {
        SceneStub {
            name: "TestScene".to_owned(),
            actor_names: vec![
                "actor1".to_owned(),
                "actor2".to_owned(),
                "actor3".to_owned(),
            ],
            parameters: vec! [
                Variable {
                    name: "val1".to_owned(),
                    typename: Type::U8,
                    default: Some(TypedValue::U8(5)),
                    documentation: None
                },
                Variable {
                    name: "val2".to_owned(),
                    typename: Type::Bool,
                    default: None,
                    documentation: Some("Test description".to_owned())
                }
            ]
        }
    }

    fn create_test_items() -> Vec<Item> {
        parse2::<File>(quote! {
            skylite_proc::properties! {
                pub val1: u8,
                pub val2: bool
            }

            #[skylite_proc::create_properties]
            fn create_properties(val1: u8, val2: bool) -> TestSceneProperties {
                TestSceneProperties { val1, val2 }
            }

            #[skylite_proc::init]
            fn init(scene: &mut TestScene, val1: u8, val2: bool) {}

            #[skylite_proc::pre_update]
            fn pre_update(scene: &mut TestScene, control: &mut ProjectControls<TestProject>) {}

            #[skylite_proc::post_render]
            fn post_render(scene: &TestScene, control: &mut DrawContext<TestProject>) {}
        }).unwrap().items
    }

    #[test]
    fn test_gen_scene_trait_impl() {
        let scene = create_test_scene();
        let items = create_test_items();

        let code = gen_scene_trait_impl(&scene, &quote!(TestProject), &items).unwrap();
        let expected = quote! {
            impl ::skylite_core::scenes::Scene for TestScene {
                type P = TestProject;

                fn _private_decode(decoder: &mut dyn ::skylite_compress::Decoder) -> TestScene {
                    use ::skylite_core::decode::Deserialize;
                    let val1 = u8::deserialize(decoder);
                    let val2 = bool::deserialize(decoder);
                    TestScene::new(val1, val2)
                }

                fn _private_actors(&mut self) -> &mut [<Self::P as ::skylite_core::SkyliteProject>::Actors] { self.actors.as_mut_slice() }
                fn _private_extras(&mut self) -> &mut Vec<<Self::P as ::skylite_core::SkyliteProject>::Actors> { &mut self.extras }

                fn _private_update(&mut self, controls: &mut ::skylite_core::ProjectControls<Self::P>) {
                    use ::skylite_core::actors::ActorBase;

                    super::pre_update(self, controls);

                    let mut actors = ::std::mem::take(&mut self.actors);
                    let mut extras = ::std::mem::take(&mut self.extras);

                    actors.iter_mut().for_each(|a| a._private_update(self, controls));
                    self.actors = actors;

                    extras = extras.into_iter().filter_map(|mut e| {
                            self.remove_extra = false;
                            e._private_update(self, controls);
                            if !self.remove_extra {
                                Some(e)
                            } else {
                                None
                            }
                        })
                        .collect();

                    ::std::mem::swap(&mut self.extras, &mut extras);
                    self.extras.append(&mut extras);
                }

                fn _private_render(&self, ctx: &mut ::skylite_core::DrawContext<Self::P>) {
                    ::skylite_core::scenes::_private::render_scene(self, ctx);
                    super::post_render(self, ctx);
                }

                fn get_actors(&self) -> &[<Self::P as ::skylite_core::SkyliteProject>::Actors] { &self.actors }

                fn get_extras(&self) -> &[<Self::P as ::skylite_core::SkyliteProject>::Actors] { &self.extras }

                fn remove_current_extra(&mut self) { self.remove_extra = true; }
            }
        };
        assert_eq!(code.to_string(), expected.to_string());
    }
}
