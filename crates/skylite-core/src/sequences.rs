use std::marker::PhantomData;

use skylite_compress::{make_decoder, Decoder};

use crate::decode::{read_varint, Deserialize};
use crate::nodes::Node;
use crate::SkyliteProject;

#[derive(Clone, Copy)]
enum Comparison {
    Equals,
    NotEquals,
    LessThen,
    GreaterThen,
    LessEquals,
    GreaterEquals,
}

impl Comparison {
    fn decode(decoder: &mut dyn Decoder) -> Comparison {
        match u8::deserialize(decoder) {
            0 => Comparison::Equals,
            1 => Comparison::NotEquals,
            2 => Comparison::LessThen,
            3 => Comparison::GreaterThen,
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
        Comparison::LessThen => lhs < rhs,
        Comparison::GreaterThen => lhs > rhs,
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

fn decode_branch_op(op_id: u8, decoder: &mut dyn Decoder) -> Op {
    let target = u32::deserialize(decoder);
    let branch_op = op_id & 0xf;
    if branch_op == BRANCH_IF_TRUE {
        Op::BranchIfTrue { target }
    } else if branch_op == BRANCH_IF_FALSE {
        Op::BranchIfFalse { target }
    } else {
        let rhs_idx = u32::deserialize(decoder);
        let comparison = Comparison::decode(decoder);

        if branch_op == BRANCH_COMPARE_F32 {
            Op::BranchF32 {
                comparison,
                rhs_idx,
                target,
            }
        } else if branch_op == BRANCH_COMPARE_F64 {
            Op::BranchF64 {
                comparison,
                rhs_idx,
                target,
            }
        } else if branch_op < BRANCH_COMPARE_SIGNED {
            let rhs_len = branch_op;
            Op::BranchUInt {
                comparison,
                rhs_idx,
                rhs_len,
                target,
            }
        } else {
            let rhs_len = branch_op & 0x7;
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
    fn decode<P: SkyliteProject>(decoder: &mut dyn Decoder) -> Op {
        let op_id = u8::deserialize(decoder);
        match op_id & 0xf0 {
            OP_SET_FIELD => {
                let data_idx = u32::deserialize(decoder);
                if op_id == OP_SET_FIELD_STRING {
                    Op::SetFieldString(data_idx)
                } else {
                    let len = 1 << (op_id & 0xf);
                    Op::SetField { data_idx, len }
                }
            }
            OP_MODIFY_FIELD => {
                let data_idx = u32::deserialize(decoder);
                if op_id == OP_MODIFY_FIELD_F32 {
                    Op::ModifyFieldF32(data_idx)
                } else if op_id == OP_MODIFY_FIELD_F64 {
                    Op::ModifyFieldF64(data_idx)
                } else {
                    let len = 1 << (op_id & 0xf);
                    Op::ModifyFieldInt { data_idx, len }
                }
            }
            OP_BRANCH_FIELD => decode_branch_op(op_id, decoder),
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

/// Represents the set of information needed to load a `Sequence`.
///
/// This trait is implemented by zero-sized types, which can be passed
/// to `Sequence::load()`, to load the actual `Sequence`.
pub trait SequenceHandle {
    type P: SkyliteProject;
    type Target: Node<P = Self::P>;
    const ID: usize;
}

/// A Sequence is a series of commands that can be run on a Node, such as
/// changing fields or running custom code.
///
/// Sequences are always specific to the Node type defined in the Sequence's
/// asset file.
pub struct Sequence<P: SkyliteProject, Target: Node<P = P>> {
    script: Box<[Op]>,
    data: Box<[u8]>,
    _target: PhantomData<Target>,
}

fn gen_decode_sequence<P: SkyliteProject>(id: usize) -> (Box<[Op]>, Box<[u8]>) {
    let compressed = <P as SkyliteProject>::_private_get_sequence_data(id);
    let mut decoder = make_decoder(compressed);

    let sequence_len = read_varint(decoder.as_mut());
    let mut script = Vec::with_capacity(sequence_len);
    (0..sequence_len).for_each(|_| script.push(Op::decode::<P>(decoder.as_mut())));

    let data_len = read_varint(decoder.as_mut());
    let mut data = Vec::with_capacity(sequence_len);
    (0..data_len).for_each(|_| data.push(u8::deserialize(decoder.as_mut())));

    (script.into_boxed_slice(), data.into_boxed_slice())
}

impl<P: SkyliteProject, Target: Node<P = P>> Sequence<P, Target> {
    pub fn _private_decode_from_id(id: usize) -> Sequence<P, Target> {
        let (script, data) = gen_decode_sequence::<P>(id);
        Sequence {
            script,
            data,
            _target: PhantomData,
        }
    }

    /// Load a `Sequence` from a `SequenceHandle`. This will return a `Sequence`
    /// which is bound to its intended target Node type.
    pub fn load<Handle: SequenceHandle>(_handle: Handle) -> Sequence<Handle::P, Handle::Target> {
        Sequence::_private_decode_from_id(Handle::ID)
    }
}

fn read_string(data: &[u8], mut offset: u32) -> String {
    // Read varint -> len
    let mut len = 0;
    loop {
        let byte = data[offset as usize];
        offset += 1;
        len = (len << 7) + (byte & 0x7f) as usize;
        if byte < 0x80 {
            break;
        }
    }

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
    fn new<'s>(script: &'s [Op], data: &'s [u8]) -> GenSequencer<'s, P> {
        GenSequencer {
            script,
            data,
            position: 0,
            call_stack: Vec::new(),
            wait_timer: 0,
            offset: 0,
            _project: PhantomData,
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
                let v = read_string(&self.data, data_idx);
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
            Op::RunCustom { id } => node._private_custom_action(id),
            Op::BranchCustom { id, target } => {
                if node._private_custom_condition(id) {
                    self.position = target as usize;
                }
            }
        }
    }

    fn update(&mut self, node: &mut dyn Node<P = P>) {
        if self.wait_timer > 0 {
            self.wait_timer -= 1;
        }

        while self.position < self.script.len() && self.wait_timer == 0 {
            let op = self.script[self.position].clone();
            self.position += 1;
            self.run_single_op(op, node);
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
pub struct Sequencer<'sequence, P: SkyliteProject, Target: Node<P = P>> {
    gen_sequencer: GenSequencer<'sequence, P>,
    _target: PhantomData<Target>,
}

impl<'sequence, P: SkyliteProject, Target: Node<P = P>> Sequencer<'sequence, P, Target> {
    /// Creates a new `Sequencer` for a `Sequence`.
    pub fn new<'s>(sequence: &'s Sequence<P, Target>) -> Sequencer<'s, P, Target> {
        Sequencer {
            gen_sequencer: GenSequencer::new(&sequence.script, &sequence.data),
            _target: PhantomData,
        }
    }

    /// Updates the `Sequencer`. This will run the commands from the Sequence
    /// until either a 'wait' command or the end of the Sequence is reached.
    pub fn update(&mut self, node: &mut Target) {
        self.gen_sequencer.update(node);
    }
}
