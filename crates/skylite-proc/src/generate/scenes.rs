use quote::{format_ident, quote};
use std::collections::HashMap;

use proc_macro2::{Literal, TokenStream, Ident};

use crate::parse::{actors::Actor, scenes::Scene, util::{change_case, IdentCase}};

use super::{actors::any_actor_type_name, encode::{CompressionBuffer, Serialize}};

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

pub(crate) fn generate_scenes_type(project_name: &str, scenes: &[Scene], actors: &[Actor]) -> TokenStream {
    let any_actor_type_name = any_actor_type_name(project_name);
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

        fn decode_actor_list(decoder: &mut dyn ::skylite_compress::Decoder) -> Vec<#any_actor_type_name> {
            use ::skylite_core::actors::ActorBase;
            let len = ::skylite_core::decode::read_varint(decoder);
            (0..len).map(|_| #any_actor_type_name::_private_decode(decoder)).collect()
        }
    }
}
