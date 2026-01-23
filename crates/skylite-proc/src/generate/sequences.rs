use std::collections::HashMap;

use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote};
use skylite_assets::{Op, Sequence, TypedValue};
use syn::Item;

use crate::generate::encode::{CompressionBuffer, Serialize};
use crate::generate::nodes::node_type_name;
use crate::generate::util::{change_case, get_annotated_function, IdentCase};
use crate::generate::{ANNOTATION_CUSTOM_CONDITION, ANNOTATION_CUSTOM_OP};
use crate::SkyliteProcError;

// region: sequence processing within skylite_project

struct CompilationResult {
    /// The bytes representing the actual compiled sequences.
    compiled_data: Vec<Vec<u8>>,

    /// The list of offsets that must be available from the generated
    /// _private_get_offset function.
    required_offsets: Vec<(String, String)>,
}

fn len_of_typed_value(val: &TypedValue) -> usize {
    match val {
        TypedValue::U8(_) => 1,
        TypedValue::U16(_) => 2,
        TypedValue::U32(_) => 4,
        TypedValue::U64(_) => 8,
        TypedValue::I8(_) => 1,
        TypedValue::I16(_) => 2,
        TypedValue::I32(_) => 4,
        TypedValue::I64(_) => 8,
        TypedValue::F32(_) => 4,
        TypedValue::F64(_) => 8,
        TypedValue::Bool(_) => 1,
        _ => unreachable!(),
    }
}

const OP_SET_FIELD: u8 = 0x00;
const OP_SET_FIELD_STRING: u8 = 0x0f;
const OP_MODIFY_FIELD: u8 = 0x10;
const OP_MODIFY_FIELD_F32: u8 = 0x1e;
const OP_MODIFY_FIELD_F64: u8 = 0x1f;
const OP_BRANCH_FIELD: u8 = 0x20;
const BRANCH_COMPARE_SIGNED: u8 = 0x8;
const BRANCH_COMPARE_F32: u8 = 0xc;
const BRANCH_COMPARE_F64: u8 = 0xd;
const BRANCH_IF_TRUE: u8 = 0xe;
const BRANCH_IF_FALSE: u8 = 0xf;

const OP_PUSH_OFFSET: u8 = 0x30;
const OP_JUMP: u8 = 0x31;
const OP_CALL_SUB: u8 = 0x32;
const OP_RETURN: u8 = 0x33;
const OP_WAIT: u8 = 0x34;
const OP_RUN_CUSTOM: u8 = 0x35;
const OP_BRANCH_CUSTOM: u8 = 0x36;

fn encode_script(
    script: &[Op],
    required_offsets: &mut HashMap<(String, String), usize>,
) -> Vec<u8> {
    let mut next_offset_id = if let Some(val) = required_offsets.values().max() {
        val + 1
    } else {
        0
    };

    let custom_op_ids: HashMap<String, usize> = {
        let mut custom_op_names: Vec<String> = script
            .iter()
            .filter_map(|op| {
                if let Op::RunCustom { id } = op {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();
        custom_op_names.sort();
        custom_op_names
            .into_iter()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect()
    };

    let custom_condition_ids: HashMap<String, usize> = {
        let mut names: Vec<String> = script
            .iter()
            .filter_map(|op| {
                if let Op::BranchCustom { id, .. } = &op {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();
        names.sort();
        names
            .into_iter()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect()
    };

    let mut buffer = CompressionBuffer::new();
    buffer.write_varint(script.len());

    for op in script {
        match op {
            Op::PushOffset { node, property } => {
                let offset_id = *required_offsets
                    .entry((node.clone(), property.clone()))
                    .or_insert_with(|| {
                        let id = next_offset_id;
                        next_offset_id += 1;
                        id
                    }) as u16;
                OP_PUSH_OFFSET.serialize(&mut buffer);
                offset_id.serialize(&mut buffer);
            }

            Op::Set { value } => {
                let len = len_of_typed_value(value);
                let len_log2 = len.trailing_zeros() as u8;
                (OP_SET_FIELD | len_log2).serialize(&mut buffer);
                value.serialize(&mut buffer);
            }
            Op::SetString { value } => {
                OP_SET_FIELD_STRING.serialize(&mut buffer);
                value.as_str().serialize(&mut buffer);
            }

            Op::Modify { delta } => {
                let len = len_of_typed_value(delta);
                let len_log2 = len.trailing_zeros() as u8;
                (OP_MODIFY_FIELD | len_log2).serialize(&mut buffer);
                delta.serialize(&mut buffer);
            }
            Op::ModifyF32 { delta } => {
                OP_MODIFY_FIELD_F32.serialize(&mut buffer);
                delta.serialize(&mut buffer);
            }
            Op::ModifyF64 { delta } => {
                OP_MODIFY_FIELD_F64.serialize(&mut buffer);
                delta.serialize(&mut buffer);
            }

            Op::Jump { target } => {
                OP_JUMP.serialize(&mut buffer);
                buffer.write_varint(*target);
            }

            Op::Call { target } => {
                OP_CALL_SUB.serialize(&mut buffer);
                buffer.write_varint(*target);
            }

            Op::Return => OP_RETURN.serialize(&mut buffer),

            Op::Wait { frames } => {
                OP_WAIT.serialize(&mut buffer);
                buffer.write_varint(*frames as usize);
            }

            Op::BranchIfTrue { target } => {
                (OP_BRANCH_FIELD | BRANCH_IF_TRUE).serialize(&mut buffer);
                buffer.write_varint(*target);
            }

            Op::BranchIfFalse { target } => {
                (OP_BRANCH_FIELD | BRANCH_IF_FALSE).serialize(&mut buffer);
                buffer.write_varint(*target);
            }

            Op::BranchUInt {
                comparison,
                target,
                value,
            } => {
                let rhs_len = len_of_typed_value(value);
                let rhs_len_log2 = rhs_len.trailing_zeros() as u8;

                (OP_BRANCH_FIELD | rhs_len_log2).serialize(&mut buffer);
                buffer.write_varint(*target);
                (*comparison as u8).serialize(&mut buffer);
                value.serialize(&mut buffer);
            }
            Op::BranchSInt {
                comparison,
                target,
                value,
            } => {
                let rhs_len = len_of_typed_value(value);
                let rhs_len_log2 = rhs_len.trailing_zeros() as u8;

                (OP_BRANCH_FIELD | BRANCH_COMPARE_SIGNED | rhs_len_log2).serialize(&mut buffer);
                buffer.write_varint(*target);
                (*comparison as u8).serialize(&mut buffer);
                value.serialize(&mut buffer);
            }
            Op::BranchF32 {
                comparison,
                target,
                value,
            } => {
                (OP_BRANCH_FIELD | BRANCH_COMPARE_F32).serialize(&mut buffer);
                buffer.write_varint(*target);
                (*comparison as u8).serialize(&mut buffer);
                value.serialize(&mut buffer);
            }
            Op::BranchF64 {
                comparison,
                target,
                value,
            } => {
                (OP_BRANCH_FIELD | BRANCH_COMPARE_F64).serialize(&mut buffer);
                buffer.write_varint(*target);
                (*comparison as u8).serialize(&mut buffer);
                value.serialize(&mut buffer);
            }

            Op::RunCustom { id } => {
                OP_RUN_CUSTOM.serialize(&mut buffer);
                buffer.write_varint(*custom_op_ids.get(id).unwrap());
            }

            Op::BranchCustom { id, target } => {
                OP_BRANCH_CUSTOM.serialize(&mut buffer);
                buffer.write_varint(*custom_condition_ids.get(id).unwrap());

                buffer.write_varint(*target);
            }
        }
    }

    buffer.encode()
}

fn compile_sequences(sequences: &[Sequence]) -> CompilationResult {
    let mut required_offsets_map = HashMap::new();
    let compiled_data: Vec<Vec<u8>> = sequences
        .iter()
        .enumerate()
        .map(|(i, sequence)| {
            assert_eq!(sequence.meta.id, i);
            encode_script(&sequence.script, &mut required_offsets_map)
        })
        .collect();

    let mut required_offsets: Vec<(String, String)> = Vec::new();
    required_offsets.resize(required_offsets_map.len(), (String::new(), String::new()));
    for (field, idx) in required_offsets_map.into_iter() {
        assert!(idx < required_offsets.len());
        assert!(required_offsets[idx].0.is_empty());
        assert!(required_offsets[idx].1.is_empty());

        required_offsets[idx] = field;
    }

    CompilationResult {
        compiled_data,
        required_offsets,
    }
}

pub(crate) fn generate_sequence_data(sequences: &[Sequence]) -> TokenStream {
    let res = compile_sequences(sequences);

    let num_sequences = res.compiled_data.len();
    let sequence_data_tokens = res.compiled_data.into_iter().map(|single_sequence_data| {
        let bytes = single_sequence_data
            .into_iter()
            .map(|b| Literal::u8_unsuffixed(b));
        quote! {
            &[#(#bytes),*]
        }
    });

    let required_offsets =
        res.required_offsets
            .into_iter()
            .enumerate()
            .map(|(id, (node, field))| {
                let node_ident = node_type_name(&node);
                let field_ident =
                    format_ident!("{}", change_case(&field, IdentCase::LowerSnakeCase));
                quote! {
                    #id => std::mem::offset_of!(#node_ident, #field_ident) as u32,
                }
            });

    quote! {
        static _PRIVATE_SEQUENCE_DATA: [&[u8];#num_sequences] = [
            #(#sequence_data_tokens),*
        ];

        fn _private_get_offset(field_id: usize) -> u32 {
            match field_id {
                #(#required_offsets)*
                _ => unreachable!(),
            }
        }
    }
}

// endregion

// region: sequence_definition

fn collect_ids<IdFun: Fn(&Op) -> Option<String>>(
    sequence: &Sequence,
    id_fun: IdFun,
) -> Vec<String> {
    let mut ids: Vec<String> = sequence.script.iter().filter_map(|op| id_fun(op)).collect();

    ids.sort();
    ids.dedup();
    ids
}

fn gen_run_custom(sequence: &Sequence, items: &[Item]) -> Result<TokenStream, SkyliteProcError> {
    let ids = collect_ids(sequence, |op| {
        if let Op::RunCustom { id } = op {
            Some(id.clone())
        } else {
            None
        }
    });

    let match_arms = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let impl_name =
                &get_annotated_function(items, &format!("{}(\"{}\")", ANNOTATION_CUSTOM_OP, id))
                    .ok_or(data_err!("No definition for custom op {}", id))?
                    .sig
                    .ident;
            Ok(quote! {
                #i => super::#impl_name(node),
            })
        })
        .collect::<Result<Vec<TokenStream>, SkyliteProcError>>()?;

    Ok(quote! {
        fn _private_run_custom(node: &mut Self::Target, id: u16) {
            match id {
                #(#match_arms)*
                _ => unreachable!(),
            }
        }
    })
}

fn gen_branch_custom(sequence: &Sequence, items: &[Item]) -> Result<TokenStream, SkyliteProcError> {
    let ids = collect_ids(sequence, |op| {
        if let Op::BranchCustom { id, .. } = op {
            Some(id.clone())
        } else {
            None
        }
    });

    let match_arms = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let impl_name = &get_annotated_function(
                items,
                &format!("{}(\"{}\")", ANNOTATION_CUSTOM_CONDITION, id),
            )
            .ok_or(data_err!("No definition for custom condition {}", id))?
            .sig
            .ident;
            Ok(quote! {
                #i => super::#impl_name(node),
            })
        })
        .collect::<Result<Vec<TokenStream>, SkyliteProcError>>()?;

    Ok(quote! {
        fn _private_branch_custom(node: &Self::Target, id: u16) -> bool{
            match id {
                #(#match_arms)*
                _ => unreachable!(),
            }
        }
    })
}

pub(crate) fn generate_sequence_definition(
    sequence: &Sequence,
    project_name: &str,
    items: &[Item],
) -> Result<TokenStream, SkyliteProcError> {
    let sequence_handle_name = format_ident!(
        "{}Handle",
        change_case(&sequence.meta.name, IdentCase::UpperCamelCase)
    );

    let id = sequence.meta.id;
    let project_ident = format_ident!("{}", change_case(project_name, IdentCase::UpperCamelCase));
    let target_node_ident =
        format_ident!("{}", change_case(&sequence.node, IdentCase::UpperCamelCase));

    let run_custom = gen_run_custom(sequence, items)?;
    let branch_custom = gen_branch_custom(sequence, items)?;

    Ok(quote! {
        use ::skylite_core::sequences::SequenceHandle;

        pub(crate) struct #sequence_handle_name;

        impl SequenceHandle for #sequence_handle_name {
            const ID: usize = #id;
            type P = #project_ident;
            type Target = #target_node_ident;

            #run_custom
            #branch_custom
        }
    })
}

// endregion
