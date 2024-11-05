use proc_macro2::{Ident, Literal, TokenStream};
use quote::{format_ident, quote};

use crate::{parse::{actors::Actor, util::{change_case, IdentCase}}, SkyliteProcError};

pub(super) fn actor_type_name(project_name: &str) -> Ident {
    format_ident!("{}Actors", change_case(project_name, IdentCase::UpperCamelCase))
}

pub(crate) fn generate_actors_type(project_name: &str, actors: &[Actor]) -> Result<TokenStream, SkyliteProcError> {
    let project_ident = format_ident!("{}", project_name);
    let type_name = actor_type_name(project_name);

    let actor_names: Vec<Ident> = actors.iter()
        .map(|a| format_ident!("{}", a.name))
        .collect();
    let actor_ids: Vec<Literal> = (0..actors.len())
        .map(|i| Literal::usize_unsuffixed(i))
        .collect();

    Ok(quote! {
        pub enum #type_name {
            #(#actor_names(::std::boxed::Box::<#actor_names>)),*
        }

        impl skylite_core::actors::InstanceId for #type_name {
            fn get_id(&self) -> u32 where Self: Sized {
                // The combination of `*self` and `ref a` is required for an empty `actors` list work,
                // because there may or may not be a way to construct *something* using an empty actors enum.
                // Realistically, this function and similar functions should never be called then,
                // because it would be impossible to do so from safe code.
                match *self {
                    #(
                        #type_name::#actor_names(ref a) => a.get_id()
                    ),*
                }
            }
        }

        impl skylite_core::actors::ActorBase for #type_name {
            type P = #project_ident;

            fn _private_decode(decoder: &mut dyn skylite_compress::Decoder) -> Self {
                match skylite_core::decode::read_varint(decoder) {
                    #(
                        #actor_ids => #type_name::#actor_names(::std::boxed::Box::new(#actor_names::_private_decode(decoder))),
                    )*
                    _ => ::std::unreachable!()
                }
            }

            fn _private_update(&mut self, project: &mut Self::P) {
                match *self {
                    #(
                        #type_name::#actor_names(ref mut a) => a._private_update(project)
                    ),*
                }
            }
            fn _private_render(&self, ctx: &mut skylite_core::DrawContext<Self::P>) {
                match *self {
                    #(
                        #type_name::#actor_names(ref a) => a._private_render(ctx)
                    ),*
                }
            }
        }

        impl skylite_core::actors::AnyActor for #type_name {
            unsafe fn _private_transmute_mut<A: skylite_core::actors::Actor>(&mut self) -> &mut A {
                match <A as skylite_core::actors::TypeId>::get_id() {
                    #(
                        #actor_ids => if let #actor_names(a) = self {
                            // If everything worked correctly, `a` should already have the type in `A` and this is a no-op.
                            ::std::mem::transmute::<&mut #actor_names, &mut A>(a)
                        } else {
                            ::std::unreachable!()
                        },
                    )*
                    _ => ::std::unreachable!()
                }
            }

            unsafe fn _private_transmute<A: skylite_core::actors::Actor>(&self) -> &A {
                match <A as skylite_core::actors::TypeId>::get_id() {
                    #(
                        #actor_ids => if let #actor_names(a) = self {
                            // If everything worked correctly, `a` should already have the type in `A` and this is a no-op.
                            ::std::mem::transmute::<&#actor_names, &A>(a)
                        } else {
                            ::std::unreachable!()
                        },
                    )*
                    _ => ::std::unreachable!()
                }
            }
        }
    })
}
