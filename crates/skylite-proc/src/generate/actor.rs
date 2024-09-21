use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{generate::project::actor_type_name, parse::project::AssetGroup};

pub(crate) fn generate_actors_type(project_name: &str, assets: &AssetGroup) -> TokenStream {
    let project_ident = format_ident!("{}", project_name);
    let type_name = actor_type_name(project_name);

    // ! EMPTY DUMMY IMPLEMENTATION

    quote! {
        pub struct #type_name();

        impl skylite_core::actor::TypeId for #type_name {
            fn get_id() -> u32 where Self: Sized { unimplemented!() }
        }

        impl skylite_core::actor::ActorBase for #type_name {
            type P = #project_ident;

            fn _private_decode(decoder: &mut dyn skylite_compress::Decoder) -> Self { unimplemented!() }

            fn _private_update(&mut self, project: &mut Self::P) {}
            fn _private_render(&self, ctx: &mut skylite_core::DrawContext<Self::P>) {}
        }

        impl skylite_core::actor::AnyActor for #type_name {
            unsafe fn _private_transmute_mut<A: skylite_core::actor::Actor>(&mut self) -> &mut A { unimplemented!() }
            unsafe fn _private_transmute<A: skylite_core::actor::Actor>(&self) -> &A { unimplemented!() }
        }
    }
}
