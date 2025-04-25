use std::marker::PhantomData;

use skylite_compress::{make_decoder, Decoder};

use crate::decode::{read_varint, Deserialize};
use crate::nodes::Node;
use crate::SkyliteProject;

#[derive(Clone, Copy)]
enum Comparison {
    Equals,
    NotEquals,
    LessThan,
    GreaterThan,
    LessEquals,
    GreaterEquals,
}

impl Comparison {
    fn decode(decoder: &mut dyn Decoder) -> Comparison {
        match u8::deserialize(decoder) {
            0 => Comparison::Equals,
            1 => Comparison::NotEquals,
            2 => Comparison::LessThan,
            3 => Comparison::GreaterThan,
            4 => Comparison::LessEquals,
            5 => Comparison::GreaterEquals,
            _ => unreachable!(),
        }
    }
}

fn test_comparison<T: PartialEq + PartialOrd>(lhs: T, comparison: Comparison, rhs: T) -> bool {
    match comparison {
        Comparison::Equals => lhs == rhs,
        Comparison::NotEquals => lhs != rhs,
        Comparison::LessThan => lhs < rhs,
        Comparison::GreaterThan => lhs > rhs,
        Comparison::LessEquals => lhs <= rhs,
        Comparison::GreaterEquals => lhs >= rhs,
    }
}

#[derive(Clone)]
enum Op {
    PushOffset(u32),
    SetField {
        data_idx: u32,
        len: u8,
    },
    SetFieldString(u32),
    ModifyFieldInt {
        data_idx: u32,
        len: u8,
    },
    ModifyFieldF32(u32),
    ModifyFieldF64(u32),
    Jump {
        target: u32,
    },
    CallSub {
        target: u32,
    },
    Return,
    Wait {
        num_updates: u16,
    },
    BranchIfTrue {
        target: u32,
    },
    BranchIfFalse {
        target: u32,
    },
    BranchUInt {
        comparison: Comparison,
        rhs_idx: u32,
        rhs_len: u8,
        target: u32,
    },
    BranchSInt {
        comparison: Comparison,
        rhs_idx: u32,
        rhs_len: u8,
        target: u32,
    },
    BranchF32 {
        comparison: Comparison,
        rhs_idx: u32,
        target: u32,
    },
    BranchF64 {
        comparison: Comparison,
        rhs_idx: u32,
        target: u32,
    },
    RunCustom {
        id: u16,
    },
    BranchCustom {
        id: u16,
        target: u32,
    },
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

fn decode_branch_op(op_id: u8, decoder: &mut dyn Decoder, data: &mut Vec<u8>) -> Op {
    let target = u32::deserialize(decoder);
    let branch_op = op_id & 0xf;
    if branch_op == BRANCH_IF_TRUE {
        Op::BranchIfTrue { target }
    } else if branch_op == BRANCH_IF_FALSE {
        Op::BranchIfFalse { target }
    } else {
        let comparison = Comparison::decode(decoder);
        let rhs_idx = data.len() as u32;

        if branch_op == BRANCH_COMPARE_F32 {
            // Copy serialized f32
            for _ in 0..4 {
                data.push(u8::deserialize(decoder));
            }

            Op::BranchF32 {
                comparison,
                rhs_idx,
                target,
            }
        } else if branch_op == BRANCH_COMPARE_F64 {
            // Copy serialized f64
            for _ in 0..8 {
                data.push(u8::deserialize(decoder));
            }

            Op::BranchF64 {
                comparison,
                rhs_idx,
                target,
            }
        } else if branch_op < BRANCH_COMPARE_SIGNED {
            let rhs_len = branch_op;
            // Copy serialized data
            for _ in 0..rhs_len {
                data.push(u8::deserialize(decoder));
            }

            Op::BranchUInt {
                comparison,
                rhs_idx,
                rhs_len,
                target,
            }
        } else {
            let rhs_len = branch_op & 0x7;
            // Copy serialized data
            for _ in 0..rhs_len {
                data.push(u8::deserialize(decoder));
            }

            Op::BranchSInt {
                comparison,
                rhs_idx,
                rhs_len,
                target,
            }
        }
    }
}

impl Op {
    fn decode<P: SkyliteProject>(decoder: &mut dyn Decoder, data: &mut Vec<u8>) -> Op {
        let op_id = u8::deserialize(decoder);
        match op_id & 0xf0 {
            OP_SET_FIELD => {
                let data_idx = data.len() as u32;
                if op_id == OP_SET_FIELD_STRING {
                    let str_len = read_varint(decoder) as u16;
                    for b in str_len.to_ne_bytes() {
                        data.push(b);
                    }

                    for _ in 0..str_len {
                        data.push(u8::deserialize(decoder));
                    }

                    Op::SetFieldString(data_idx)
                } else {
                    let len = 1 << (op_id & 0xf);
                    for _ in 0..len {
                        data.push(u8::deserialize(decoder));
                    }
                    Op::SetField { data_idx, len }
                }
            }
            OP_MODIFY_FIELD => {
                let data_idx = data.len() as u32;
                if op_id == OP_MODIFY_FIELD_F32 {
                    for _ in 0..4 {
                        data.push(u8::deserialize(decoder));
                    }
                    Op::ModifyFieldF32(data_idx)
                } else if op_id == OP_MODIFY_FIELD_F64 {
                    for _ in 0..8 {
                        data.push(u8::deserialize(decoder));
                    }
                    Op::ModifyFieldF64(data_idx)
                } else {
                    let len = 1 << (op_id & 0xf);
                    for _ in 0..len {
                        data.push(u8::deserialize(decoder));
                    }
                    Op::ModifyFieldInt { data_idx, len }
                }
            }
            OP_BRANCH_FIELD => decode_branch_op(op_id, decoder, data),
            _ => match op_id {
                OP_PUSH_OFFSET => {
                    let field_id = u32::deserialize(decoder) as usize;
                    Op::PushOffset(P::_private_get_offset(field_id))
                }
                OP_JUMP => Op::Jump {
                    target: u32::deserialize(decoder),
                },
                OP_CALL_SUB => Op::CallSub {
                    target: u32::deserialize(decoder),
                },
                OP_RETURN => Op::Return,
                OP_WAIT => Op::Wait {
                    num_updates: u16::deserialize(decoder),
                },
                OP_RUN_CUSTOM => Op::RunCustom {
                    id: u16::deserialize(decoder),
                },
                OP_BRANCH_CUSTOM => {
                    let id = u16::deserialize(decoder);
                    let target = u32::deserialize(decoder);
                    Op::BranchCustom { id, target }
                }
                _ => unreachable!(),
            },
        }
    }
}

pub struct GenSequence<P: SkyliteProject> {
    script: Box<[Op]>,
    data: Box<[u8]>,
    _project: PhantomData<P>,
}

impl<P: SkyliteProject> GenSequence<P> {
    pub fn _private_decode_from_id(id: usize) -> GenSequence<P> {
        let compressed = <P as SkyliteProject>::_private_get_sequence_data(id);
        let mut decoder = make_decoder(compressed);

        let mut data = Vec::new();

        let sequence_len = read_varint(decoder.as_mut());
        let mut script = Vec::with_capacity(sequence_len);
        (0..sequence_len).for_each(|_| script.push(Op::decode::<P>(decoder.as_mut(), &mut data)));

        GenSequence {
            script: script.into_boxed_slice(),
            data: data.into_boxed_slice(),
            _project: PhantomData,
        }
    }
}

pub trait Sequence {
    type P: SkyliteProject;
    type Target: Node<P = Self::P>;

    fn load() -> Self;
    fn _private_run_custom(node: &mut Self::Target, id: u16);
    fn _private_branch_custom(node: &Self::Target, id: u16) -> bool;
    fn _private_get_generic_sequence(&self) -> &GenSequence<Self::P>;
}

fn read_string(data: &[u8], mut offset: usize) -> String {
    let len = u16::from_ne_bytes([data[offset], data[offset + 1]]) as usize;
    offset += 2;

    let bytes = data[offset as usize..offset as usize + len].to_owned();
    unsafe { String::from_utf8_unchecked(bytes) }
}

#[inline]
fn data_to_u64(data: &[u8]) -> u64 {
    let mut bytes = [0_u8; 8];
    bytes[0..data.len()].copy_from_slice(data);
    u64::from_ne_bytes(bytes)
}

#[inline]
fn data_to_f32(data: &[u8]) -> f32 {
    let mut bytes = [0_u8; 4];
    bytes[0..data.len()].copy_from_slice(data);
    f32::from_ne_bytes(bytes)
}

#[inline]
fn data_to_f64(data: &[u8]) -> f64 {
    let mut bytes = [0_u8; 8];
    bytes[0..data.len()].copy_from_slice(data);
    f64::from_ne_bytes(bytes)
}

unsafe fn modify_field_int(target: *mut u8, data: &[u8], offset: usize, len: usize) {
    let field = std::slice::from_raw_parts_mut(target, len);
    let value = data_to_u64(field);
    let delta = data_to_u64(&data[offset..offset + len]);

    let result = value.wrapping_add(delta);
    let result_bytes = result.to_ne_bytes();
    field.copy_from_slice(&result_bytes[0..len]);
}

fn compare_field_int(
    comparison: Comparison,
    mut lhs: u64,
    mut rhs: u64,
    len: usize,
    signed: bool,
) -> bool {
    if signed {
        #[cfg(target_endian = "little")]
        let adjust = 0x80 << (len - 1);
        #[cfg(not(target_endian = "little"))]
        let adjust = 0x8000_0000_0000_0000;
        lhs = lhs.wrapping_add(adjust);
        rhs = rhs.wrapping_add(adjust);
    }

    test_comparison(lhs, comparison, rhs)
}

struct GenSequencer<'sequence, P: SkyliteProject> {
    script: &'sequence [Op],
    data: &'sequence [u8],
    position: usize,
    call_stack: Vec<usize>,
    wait_timer: u16,
    offset: usize,
    _project: PhantomData<P>,
}

impl<'sequence, P: SkyliteProject> GenSequencer<'sequence, P> {
    fn new<'s>(gen_sequence: &'s GenSequence<P>) -> GenSequencer<'s, P> {
        GenSequencer {
            script: &gen_sequence.script,
            data: &gen_sequence.data,
            position: 0,
            // This means that returning from the main script will end the sequence.
            call_stack: vec![gen_sequence.script.len()],
            wait_timer: 0,
            offset: 0,
            _project: PhantomData,
        }
    }

    fn fetch_next(&mut self) -> Option<Op> {
        if self.wait_timer > 0 {
            self.wait_timer -= 1;
            None
        } else if self.position >= self.script.len() {
            None
        } else {
            let op = self.script[self.position].clone();
            self.position += 1;
            Some(op)
        }
    }

    fn run_branch_op(
        &mut self,
        node_mem: *const u8,
        comparison: Comparison,
        rhs_idx: usize,
        rhs_len: usize,
        target: usize,
        signed: bool,
    ) {
        let lhs_data = unsafe { std::slice::from_raw_parts(node_mem.add(self.offset), rhs_len) };
        let lhs = data_to_u64(lhs_data);
        let rhs_data = &self.data[rhs_idx..rhs_idx + rhs_len];
        let rhs = data_to_u64(rhs_data);
        if compare_field_int(comparison, lhs, rhs, rhs_len, signed) {
            self.position = target as usize;
        }
        self.offset = 0;
    }

    fn run_single_op(&mut self, op: Op, node: &mut dyn Node<P = P>) {
        let node_mem = node as *mut dyn Node<P = P> as *mut u8;
        match op {
            Op::PushOffset(offset) => self.offset += offset as usize,
            Op::SetField { data_idx, len } => unsafe {
                let src = &self.data[data_idx as usize] as *const u8;
                let dest = node_mem.add(self.offset);
                dest.copy_from(src, len as usize);
                self.offset = 0;
            },
            Op::SetFieldString(data_idx) => unsafe {
                let v = read_string(&self.data, data_idx as usize);
                *(node_mem.add(self.offset) as *mut String) = v;
                self.offset = 0;
            },
            Op::ModifyFieldInt { data_idx, len } => unsafe {
                let dest = node_mem.add(self.offset);
                modify_field_int(dest, &self.data, data_idx as usize, len as usize);
                self.offset = 0;
            },
            Op::ModifyFieldF32(data_idx) => unsafe {
                let field_addr = node_mem.add(self.offset);
                let field_data = std::slice::from_raw_parts(field_addr, 4);
                let field = data_to_f32(field_data);
                let delta = data_to_f32(&self.data[data_idx as usize..data_idx as usize + 4]);
                let result = field + delta;
                let result_data = result.to_ne_bytes();
                field_addr.copy_from(result_data.as_ptr(), 4);
            },
            Op::ModifyFieldF64(data_idx) => unsafe {
                let field_addr = node_mem.add(self.offset);
                let field_data = std::slice::from_raw_parts(field_addr, 8);
                let field = data_to_f64(field_data);
                let delta = data_to_f64(&self.data[data_idx as usize..data_idx as usize + 8]);
                let result = field + delta;
                let result_data = result.to_ne_bytes();
                field_addr.copy_from(result_data.as_ptr(), 8);
            },
            Op::Jump { target } => self.position = target as usize,
            Op::CallSub { target } => {
                self.call_stack.push(self.position + 1);
                self.position = target as usize;
            }
            Op::Return => self.position = self.call_stack.pop().unwrap() as usize,
            Op::Wait { num_updates } => self.wait_timer = num_updates,
            Op::BranchIfTrue { target } => {
                let v = unsafe { *(node_mem.add(self.offset) as *const bool) };
                if v {
                    self.position = target as usize;
                }
            }
            Op::BranchIfFalse { target } => {
                let v = unsafe { *(node_mem.add(self.offset) as *const bool) };
                if !v {
                    self.position = target as usize;
                }
            }
            Op::BranchUInt {
                comparison,
                rhs_idx,
                rhs_len,
                target,
            } => self.run_branch_op(
                node_mem,
                comparison,
                rhs_idx as usize,
                rhs_len as usize,
                target as usize,
                false,
            ),
            Op::BranchSInt {
                comparison,
                rhs_idx,
                rhs_len,
                target,
            } => self.run_branch_op(
                node_mem,
                comparison,
                rhs_idx as usize,
                rhs_len as usize,
                target as usize,
                true,
            ),
            Op::BranchF32 {
                comparison,
                rhs_idx,
                target,
            } => {
                let lhs_data = unsafe { std::slice::from_raw_parts(node_mem.add(self.offset), 4) };
                let lhs = data_to_f32(lhs_data);
                let rhs = data_to_f32(&self.data[rhs_idx as usize..rhs_idx as usize + 4]);

                if test_comparison(lhs, comparison, rhs) {
                    self.position = target as usize;
                }
                self.offset = 0;
            }
            Op::BranchF64 {
                comparison,
                rhs_idx,
                target,
            } => {
                let lhs_data = unsafe { std::slice::from_raw_parts(node_mem.add(self.offset), 8) };
                let lhs = data_to_f64(lhs_data);
                let rhs = data_to_f64(&self.data[rhs_idx as usize..rhs_idx as usize + 8]);

                if test_comparison(lhs, comparison, rhs) {
                    self.position = target as usize;
                }
                self.offset = 0;
            }
            Op::RunCustom { .. } => unreachable!(),
            Op::BranchCustom { .. } => unreachable!(),
        }
    }
}

/// A `Sequencer` is used to play back a `Sequence`. A `Sequencer` is always
/// specific to the same `Node` as the `Sequence` it is playing back.
///
/// Playing back a sequence is done by calling the sequencer's `update`
/// function. This function should be called exactly once during a Node's update
/// cycle, either in the `pre_update` or the `post_update`. The sequencer will
/// make the modifications to the Node that are defined in the sequence.
pub struct Sequencer<'sequence, S: Sequence> {
    gen_sequencer: GenSequencer<'sequence, S::P>,
}

impl<'sequence, S: Sequence> Sequencer<'sequence, S> {
    /// Creates a new `Sequencer` for a `Sequence`.
    pub fn new<'s>(sequence: &'s mut S) -> Sequencer<'s, S> {
        Sequencer {
            gen_sequencer: GenSequencer::new(sequence._private_get_generic_sequence()),
        }
    }

    /// Updates the `Sequencer`. This will run the commands from the Sequence
    /// until either a 'wait' command or the end of the Sequence is reached.
    pub fn update(&mut self, node: &mut S::Target) {
        while let Some(op) = self.gen_sequencer.fetch_next() {
            match op {
                Op::RunCustom { id } => <S as Sequence>::_private_run_custom(node, id),
                Op::BranchCustom { id, target } => {
                    if <S as Sequence>::_private_branch_custom(node, id) {
                        self.gen_sequencer.position = target as usize;
                    }
                }
                _ => self.gen_sequencer.run_single_op(op, node),
            }
        }
    }
}
