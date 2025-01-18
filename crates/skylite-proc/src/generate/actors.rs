use proc_macro2::{Ident, Literal, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_str, Item, ItemFn, Meta};

use crate::{parse::{actors::{Action, Actor}, util::{change_case, IdentCase}, values::Variable}, SkyliteProcError};

use super::{project::project_type_name, util::{generate_param_list, get_annotated_function, get_macro_item, skylite_type_to_rust, typed_value_to_rust}};

fn actor_type_name(actor_name: &str) -> Ident { format_ident!("{}", change_case(actor_name, IdentCase::UpperCamelCase)) }
fn action_type_name(actor_name: &str) -> Ident { format_ident!("{}Actions", change_case(actor_name, IdentCase::UpperCamelCase)) }
fn properties_type_name(actor_name: &str) -> Ident { format_ident!("{}Properties", change_case(actor_name, IdentCase::UpperCamelCase)) }

fn get_documentation(doc: &Option<String>) -> TokenStream {
    match &doc {
        Some(v) => {
            let content = Literal::string(&v);
            quote!(#[doc = #content])
        },
        None => TokenStream::new(),
    }
}

fn get_parameter_name(var: &Variable) -> Ident { format_ident!("{}", change_case(&var.name, IdentCase::LowerSnakeCase)) }
fn get_parameter_type(var: &Variable) -> TokenStream { skylite_type_to_rust(&var.typename) }
fn get_parameter_docs(var: &Variable) -> TokenStream { get_documentation(&var.documentation) }

// region: Actor Actions

fn get_action_impl_name(action_name: &str, items: &[Item]) -> Result<Ident, SkyliteProcError> {
    let meta = parse_str::<Meta>(&format!("skylite_proc::action(\"{}\")", action_name)).unwrap();
    let mut res = items.iter().filter_map(|item| if let Item::Fn(fun) = item {
            Some(fun)
        } else {
            None
        })
        .filter(|fun| fun.attrs.iter().any(|attr| attr.meta == meta));

    let out = match res.next() {
        Some(fun) => fun.sig.ident.clone(),
        None => return Err(SkyliteProcError::DataError(format!("Missing implementation for action {}", action_name)))
    };

    match res.next() {
        Some(_) => return Err(SkyliteProcError::DataError(format!("Multiple implementation for action {}", action_name))),
        None => ()
    };

    Ok(out)
}

fn gen_action_deserialize_calls(action: &Action) -> TokenStream {
    let names = action.params.iter().map(|a| format_ident!("{}", change_case(&a.name, IdentCase::LowerSnakeCase)));
    let types = action.params.iter().map(|a| skylite_type_to_rust(&a.typename));
    quote! {
        #(
            let #names = #types::deserialize(decoder);
        )*
    }
}

fn get_action_name(action: &Action) -> Ident { format_ident!("{}", change_case(&action.name, IdentCase::UpperCamelCase)) }

fn get_action_param_names(action: &Action) -> TokenStream {
    let names = action.params.iter().map(get_parameter_name);
    quote!(#(#names),*)
}

fn gen_actions_type(name: &Ident, actions: &[Action]) -> TokenStream {
    let action_names: Vec<Ident> = actions.iter().map(get_action_name).collect();
    let action_documentation = actions.iter().map(|action| get_documentation(&action.description));
    let action_param_lists: Vec<TokenStream> = actions.iter()
        .map(|action| {
            let param_docs = action.params.iter().map(get_parameter_docs);
            let param_names = action.params.iter().map(get_parameter_name);
            let param_types = action.params.iter().map(get_parameter_type);
            quote!(#(#param_docs #param_names: #param_types),*)
        }).collect();
    let action_param_names: Vec<TokenStream> = actions.iter().map(get_action_param_names).collect();
    let action_ids = (0..actions.len()).map(|i| Literal::u8_unsuffixed(i as u8));
    let action_decoders = actions.iter().map(gen_action_deserialize_calls);

    quote! {
        pub enum #name {
            #(
                #action_documentation
                #action_names { #action_param_lists }
            ),*
        }

        impl ::skylite_core::actors::ActorAction for #name {
            fn _private_decode(decoder: &mut dyn ::skylite_compress::Decoder) -> #name {
                use skylite_core::decode::Deserialize;
                match u8::deserialize(decoder) {
                    #(
                        #action_ids => {
                            #action_decoders
                            #name::#action_names { #action_param_names }
                        },
                    )*
                    _ => unreachable!()
                }
            }
        }
    }
}

// endregion

// region: Actor Properties Type

fn get_actor_param_list(actor: &Actor) -> TokenStream { generate_param_list(&actor.parameters) }

fn gen_properties_type(actor: &Actor, items: &[Item]) -> Result<TokenStream, SkyliteProcError> {
    let actor_param_list = get_actor_param_list(actor);
    let actor_param_names: Vec<Ident> = actor.parameters.iter().map(get_parameter_name).collect();
    let properties_type_name = properties_type_name(&actor.name);

    // The properties are copied directly from the `skylite_proc::properties!` function macro.
    let properties = match get_macro_item("skylite_proc::properties", items)? {
        Some(tokens) => tokens.clone(),
        None => TokenStream::new()
    };

    let create_properties_call = if !properties.is_empty() {
        match get_annotated_function(items, "skylite_proc::create_properties") {
            Some(fun) => {
                let ident = &fun.sig.ident;
                quote! { super::#ident(#(#actor_param_names),*) }
            },
            None => return Err(SkyliteProcError::DataError(format!("Missing required special function `create_properties`. Function is required because the actor has properties.")))
        }
    } else {
        quote!(#properties_type_name {})
    };

    Ok(quote! {
        pub struct #properties_type_name {
            #properties
        }

        impl #properties_type_name {
            fn _private_create_properties(#actor_param_list) -> #properties_type_name {
                #create_properties_call
            }
        }
    })
}

// endregion

// region: Main Actor Type

fn gen_actor_type(actor: &Actor, items: &[Item]) -> TokenStream {
    let actor_type_name = actor_type_name(&actor.name);
    let action_type_name = action_type_name(&actor.name);
    let properties_type_name = properties_type_name(&actor.name);
    let actor_param_list = get_actor_param_list(actor);
    let actor_param_names: Vec<Ident> = actor.parameters.iter()
        .map(get_parameter_name)
        .collect();

    let initial_action_name = format_ident!("{}", change_case(&actor.initial_action.name, IdentCase::UpperCamelCase));
    let initial_action_params = actor
        .actions.iter()
            .find(|action| action.name == actor.initial_action.name).unwrap()
        .params.iter()
            .map(|p| format_ident!("{}", change_case(&p.name, IdentCase::LowerSnakeCase)));
    let initial_action_args = actor.initial_action.args.iter()
        .map(typed_value_to_rust);

    let init_fn = get_annotated_function(items, "skylite_proc::init")
        .map(|fun| fun.sig.ident.clone())
        .map(|name| quote!(super::#name(out, #(#actor_param_names),*);))
        .unwrap_or(TokenStream::new());

    quote! {
        pub struct #actor_type_name {
            pub properties: #properties_type_name,
            entity: ::skylite_core::ecs::Entity,
            current_action: #action_type_name,
            action_changed: bool,
            clear_action_changed: bool
        }

        impl #actor_type_name {
            pub fn new(#actor_param_list) -> #actor_type_name {
                let mut out = #actor_type_name {
                    // See `gen_actor_properties_type` for the definition of `create_properties`.
                    properties: #properties_type_name::_private_create_properties(#(#actor_param_names.clone()),*),
                    entity: ::skylite_core::ecs::Entity::new(),
                    current_action: #action_type_name::#initial_action_name {
                        #(#initial_action_params: #initial_action_args),*
                    },
                    action_changed: true,
                    clear_action_changed: false
                };

                #init_fn
                out
            }
        }
    }
}

// endregion

// region: Actor Trait Implementation

fn gen_actor_decode_fn(actor_type_name: &Ident, params: &[Variable]) -> TokenStream {
    let actor_param_names: Vec<Ident> = params.iter().map(get_parameter_name).collect();
    let actor_args_decoders = params.iter()
        .map(|p| {
            let t = skylite_type_to_rust(&p.typename);
            quote!(#t::deserialize(decoder))
        });

    quote! {
        fn _private_decode(decoder: &mut dyn ::skylite_compress::Decoder) -> #actor_type_name
        where Self: Sized {
            use skylite_core::decode::Deserialize;
            #(
                let #actor_param_names = #actor_args_decoders;
            )*
            // See `gen_actor_type` for the definition of `new`
            #actor_type_name::new(#(#actor_param_names),*)
        }
    }
}

fn gen_actor_update_fn(actions_type_name: &Ident, actions: &[Action], items: &[Item]) -> Result<TokenStream, SkyliteProcError> {
    fn get_name(fun: &ItemFn) -> Ident { fun.sig.ident.clone() }

    let action_names: Vec<Ident> = actions.iter().map(get_action_name).collect();
    let action_param_names = actions.iter().map(get_action_param_names);
    let action_args = actions.iter()
        .map(|action| {
            let param_names = action.params.iter().map(get_parameter_name);
            // The arguments to the action implementation must be cloned,
            // because some of possible types (String, Vec) own memory on the heap.
            quote!(#(#param_names.clone()),*)
        });

    let action_implementations = actions.iter()
        .map(|action| get_action_impl_name(&action.name, items))
        .collect::<Result<Vec<Ident>, SkyliteProcError>>()?;

    let pre_update = get_annotated_function(items, "skylite_proc::pre_update")
        .map(get_name)
        .map(|name| quote!(super::#name(self, scene, controls);))
        .unwrap_or(TokenStream::new());

    let post_update = get_annotated_function(items, "skylite_proc::post_update")
        .map(get_name)
        .map(|name| quote!(super::#name(self, scene, controls);))
        .unwrap_or(TokenStream::new());

    Ok(quote! {
        fn _private_update(&mut self, scene: &mut dyn ::skylite_core::scenes::Scene<P=Self::P>, controls: &mut ::skylite_core::ProjectControls<Self::P>) {
            #pre_update

            self.clear_action_changed = self.action_changed;
            match self.current_action {
                #(
                    #actions_type_name::#action_names { #action_param_names } => super::#action_implementations(self, scene, controls, #action_args)
                ),*
            };

            #post_update

            if self.clear_action_changed {
                self.action_changed = false;
                self.clear_action_changed = false;
            }
        }
    })
}

fn gen_actor_impl(actor: &Actor, project_type_ident: &TokenStream, items: &[Item]) -> Result<TokenStream, SkyliteProcError> {
    fn get_name(fun: &ItemFn) -> Ident { fun.sig.ident.clone() }

    let actor_type_name = actor_type_name(&actor.name);
    let action_type_name = action_type_name(&actor.name);

    let private_decode = gen_actor_decode_fn(&actor_type_name, &actor.parameters);
    let private_update = gen_actor_update_fn(&action_type_name, &actor.actions, items)?;

    let render = get_annotated_function(items, "skylite_proc::render")
        .map(get_name)
        .map(|name| quote!(super::#name(self, ctx);))
        .unwrap_or(TokenStream::new());

    let z_order = get_annotated_function(items, "skylite_proc::z_order")
        .map(get_name)
        .map(|name| quote!(fn z_order(&self) -> i16 { super::#name(self) }))
        .unwrap_or(TokenStream::new());

    Ok(quote! {
        impl ::skylite_core::actors::Actor for #actor_type_name {
            type P = #project_type_ident;
            type Action = #action_type_name where Self: Sized;

            #private_decode

            #private_update

            fn _private_render(&self, ctx: &mut ::skylite_core::DrawContext<Self::P>) {
                #render
            }

            fn get_entity(&self) -> &::skylite_core::ecs::Entity { &self.entity }

            fn get_entity_mut(&mut self) -> &mut ::skylite_core::ecs::Entity { &mut self.entity }

            fn set_action(&mut self, action: #action_type_name)
            where Self: Sized {
                self.current_action = action;
                self.action_changed = true;
                self.clear_action_changed = false;
            }

            fn action_changed(&self) -> bool { self.action_changed }

            #z_order
        }
    })
}

// endregion

// region: generate_actor_definition Entrypoint

pub(crate) fn generate_actor_definition(actor: &Actor, actor_id: usize, project_name: &str, items: &[Item], body_raw: &TokenStream) -> Result<TokenStream, SkyliteProcError> {
    let project_type_name = project_type_name(project_name);
    let actor_module_name = format_ident!("{}", change_case(&actor.name, IdentCase::LowerSnakeCase));
    let actor_type_name = actor_type_name(&actor.name);
    let actor_id = Literal::usize_unsuffixed(actor_id);

    let imports = items.iter().filter_map(|item| if let Item::Use(import) = item {
            Some(import.to_owned())
        } else {
            None
        });

    let action_type_name = action_type_name(&actor.name);
    let action_type = gen_actions_type(&action_type_name, &actor.actions);

    let properties_type = gen_properties_type(actor, items)?;
    let actor_type = gen_actor_type(actor, items);
    let actor_impl = gen_actor_impl(actor, &project_type_name, items)?;

    // The idea here is that `actor_definition! { ... }` opens a separate scope, but the generated code
    // is still accessible from the outside. This enables putting multiple actor_definitions into the same
    // file, with each of the actor types being public at the root of the file.
    Ok(quote! {
        mod #actor_module_name {
            pub mod gen {
                #![allow(unused_imports)]
                #(
                    #imports
                )
                *

                use super::*;

                #action_type

                #properties_type

                #actor_type

                impl ::skylite_core::actors::TypeId for #actor_type_name {
                    fn get_id() -> usize {
                        #actor_id
                    }
                }

                #actor_impl
            }

            use gen::*;

            #body_raw
        }

        pub use #actor_module_name::gen::*;
    })
}

// endregion

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::{parse2, File, Item};
    use crate::parse::actors::{Actor, Action, ActionInstance};
    use crate::parse::values::{Type, TypedValue, Variable};

    use super::{action_type_name, gen_actions_type, gen_actor_impl, gen_actor_type, gen_properties_type};

    fn create_test_actor() -> Actor {
        Actor {
            name: "TestActor".to_owned(),
            parameters: vec![
                Variable { name: "x".to_owned(), typename: Type::U16, documentation: Some("x-coordinate".to_owned()), default: None },
                Variable { name: "y".to_owned(), typename: Type::U16, documentation: Some("y-coordinate".to_owned()), default: None },
            ],
            actions: vec![
                Action {
                    name: "action1".to_owned(),
                    params: vec![
                        Variable { name: "dx".to_owned(), typename: Type::U8, documentation: None, default: None },
                        Variable { name: "dy".to_owned(), typename: Type::U8, documentation: None, default: None }
                    ],
                    description: Some("action 1".to_owned())
                },
                Action {
                    name: "action2".to_owned(),
                    params: vec![
                        Variable { name: "val".to_owned(), typename: Type::U8, documentation: Some("test2 doc".to_owned()), default: None }
                    ],
                    description: Some("test".to_owned())
                },
                Action {
                    name: "action3".to_owned(),
                    params: vec![],
                    description: None
                }
            ],
            initial_action: ActionInstance { name: "action2".to_owned(), args: vec![TypedValue::U8(5)] }
        }
    }

    fn create_test_items() -> Vec<Item> {
        parse2::<File>(quote! {
            skylite_proc::properties! {
                val1: u8,
                val2: u8,
                val3: bool
            }

            #[skylite_proc::create_properties]
            fn create_properties(x: u8, y: u8) -> TestActorProperties { todo!() }

            #[skylite_proc::init]
            fn init(actor: &mut TestActor, x: u8, y: u8) {}

            #[skylite_proc::pre_update]
            fn pre_update(actor: &mut TestActor, project: &mut TestProject) {}

            #[skylite_proc::render]
            fn render(actor: &TestActor, project: &mut ::skylite_core::DrawContext<TestProject>) {}

            #[skylite_proc::action("action1")]
            fn action1(actor: &mut TestActor, project: &mut TestProject, dx: u8, dy: u8) {}

            #[skylite_proc::action("action2")]
            fn action2(actor: &mut TestActor, project: &mut TestProject, val: u8) {}

            #[skylite_proc::action("action3")]
            fn action3(actor: &mut TestActor, project: &mut TestProject) {}

            #[skylite_proc::z_order]
            fn z_order(actor: &mut TestActor) -> i16 { 5 }
        }).unwrap().items
    }

    #[test]
    fn test_gen_actions_type() {
        let actor = create_test_actor();
        let actor_type_name = action_type_name(&actor.name);
        let code = gen_actions_type(&actor_type_name, &actor.actions);
        let expectation = quote! {
            pub enum TestActorActions {
                #[doc="action 1"]
                Action1 {
                    dx: u8,
                    dy: u8
                },
                #[doc="test"]
                Action2 {
                    #[doc="test2 doc"]
                    val: u8
                },
                Action3 {}
            }

            impl ::skylite_core::actors::ActorAction for TestActorActions {
                fn _private_decode(decoder: &mut dyn ::skylite_compress::Decoder) -> TestActorActions {
                    use skylite_core::decode::Deserialize;
                    match u8::deserialize(decoder) {
                        0 => {
                            let dx = u8::deserialize(decoder);
                            let dy = u8::deserialize(decoder);
                            TestActorActions::Action1 { dx, dy }
                        },
                        1 => {
                            let val = u8::deserialize(decoder);
                            TestActorActions::Action2 { val }
                        },
                        2 => {
                            TestActorActions::Action3 {}
                        },
                        _ => unreachable!()
                    }
                }
            }
        };
        assert_eq!(code.to_string(), expectation.to_string());
    }

    #[test]
    fn test_gen_properties_type() {
        let actor = create_test_actor();
        let items = create_test_items();
        let code = gen_properties_type(&actor, &items).unwrap();
        let expectation = quote! {
            pub struct TestActorProperties {
                val1: u8,
                val2: u8,
                val3: bool
            }

            impl TestActorProperties {
                fn _private_create_properties(x: u16, y: u16) -> TestActorProperties {
                    super::create_properties(x, y)
                }
            }
        };
        assert_eq!(code.to_string(), expectation.to_string());
    }

    #[test]
    fn test_gen_actor_type() {
        let actor = create_test_actor();
        let items = create_test_items();
        let code = gen_actor_type(&actor, &items);
        let expectation = quote! {
            pub struct TestActor {
                pub properties: TestActorProperties,
                entity: ::skylite_core::ecs::Entity,
                current_action: TestActorActions,
                action_changed: bool,
                clear_action_changed: bool
            }

            impl TestActor {
                pub fn new(x: u16, y: u16) -> TestActor {
                    let mut out = TestActor {
                        properties: TestActorProperties::_private_create_properties(x.clone(), y.clone()),
                        entity: ::skylite_core::ecs::Entity::new(),
                        current_action: TestActorActions::Action2 { val: 5u8 },
                        action_changed: true,
                        clear_action_changed: false
                    };

                    super::init(out, x, y);
                    out
                }
            }
        };
        assert_eq!(code.to_string(), expectation.to_string());
    }

    #[test]
    fn test_gen_actor_base_impl() {
        let actor = create_test_actor();
        let items = create_test_items();
        let code = gen_actor_impl(&actor, &quote!(crate::TestProject), &items).unwrap();
        let expectation = quote! {
            impl ::skylite_core::actors::Actor for TestActor {
                type P = crate::TestProject;
                type Action = TestActorActions where Self: Sized;

                fn _private_decode(decoder: &mut dyn ::skylite_compress::Decoder) -> TestActor where Self: Sized{
                    use skylite_core::decode::Deserialize;
                    let x = u16::deserialize(decoder);
                    let y = u16::deserialize(decoder);
                    TestActor::new(x, y)
                }

                fn _private_update(&mut self, scene: &mut dyn ::skylite_core::scenes::Scene<P=Self::P>, controls: &mut ::skylite_core::ProjectControls<Self::P>) {
                    super::pre_update(self, scene, controls);

                    self.clear_action_changed = self.action_changed;
                    match self.current_action {
                        TestActorActions::Action1 { dx, dy } => super::action1(self, scene, controls, dx.clone(), dy.clone()),
                        TestActorActions::Action2 { val } => super::action2(self, scene, controls, val.clone()),
                        TestActorActions::Action3 {} => super::action3(self, scene, controls,)
                    };

                    if self.clear_action_changed {
                        self.action_changed = false;
                        self.clear_action_changed = false;
                    }
                }

                fn _private_render(&self, ctx: &mut ::skylite_core::DrawContext<Self::P>) {
                    super::render(self, ctx);
                }

                fn get_entity(&self) -> &::skylite_core::ecs::Entity { &self.entity }

                fn get_entity_mut(&mut self) -> &mut ::skylite_core::ecs::Entity { &mut self.entity }

                fn set_action(&mut self, action: TestActorActions)
                where Self: Sized {
                    self.current_action = action;
                    self.action_changed = true;
                    self.clear_action_changed = false;
                }

                fn action_changed(&self) -> bool { self.action_changed }

                fn z_order(&self) -> i16 { super::z_order(self) }
            }
        };
        assert_eq!(code.to_string(), expectation.to_string());
    }
}
