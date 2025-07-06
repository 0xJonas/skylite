use crate::parse::sequences::{
    BranchCondition, Field, FieldPathSegment, InputLine, InputOp, Sequence,
};
use crate::parse::values::TypedValue;
use crate::{change_case, IdentCase};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Comparison {
    Equals = 0,
    NotEquals = 1,
    LessThan = 2,
    GreaterThan = 3,
    LessEquals = 4,
    GreaterEquals = 5,
}

/// Intermediate representation. Each OpIR is compiled into exactly one Op.
#[derive(Debug, PartialEq)]
pub(super) enum OpIR {
    PushOffset(String, String),
    SetField {
        val: TypedValue,
    },
    ModifyField {
        delta: TypedValue,
    },
    Jump {
        label: String,
    },
    CallSub {
        sub: String,
    },
    Return,
    Wait {
        updates: u16,
    },
    BranchIfTrue {
        label: String,
    },
    BranchIfFalse {
        label: String,
    },
    BranchCmp {
        comparison: Comparison,
        rhs: TypedValue,
        label: String,
    },
    RunCustom {
        id: String,
    },
    BranchCustom {
        id: String,
        label: String,
    },
}

#[derive(Debug, PartialEq)]
pub(super) struct OpIRLine {
    pub labels: Vec<String>,
    pub op_ir: OpIR,
}

fn push_offset_ops_for_field(field: &Field) -> Vec<OpIR> {
    field
        .path
        .iter()
        .map(|segment| match segment {
            FieldPathSegment(node, property) => OpIR::PushOffset(
                change_case(node, IdentCase::UpperCamelCase),
                change_case(property, IdentCase::LowerSnakeCase),
            ),
        })
        .collect()
}

fn input_to_ir_single(input: &InputLine) -> Vec<OpIRLine> {
    let mut ir_lines: Vec<OpIRLine> = match &input.input_op {
        InputOp::Set { field, val } => {
            let mut ir_ops = push_offset_ops_for_field(&field);
            ir_ops.push(OpIR::SetField { val: val.clone() });
            ir_ops
                .into_iter()
                .map(|op_ir| OpIRLine {
                    labels: vec![],
                    op_ir,
                })
                .collect()
        }
        InputOp::Modify { field, delta } => {
            let mut ir_ops = push_offset_ops_for_field(&field);
            ir_ops.push(OpIR::ModifyField {
                delta: delta.clone(),
            });
            ir_ops
                .into_iter()
                .map(|op_ir| OpIRLine {
                    labels: vec![],
                    op_ir,
                })
                .collect()
        }
        InputOp::Branch { condition, label } => {
            let ir_ops = match condition {
                BranchCondition::IfTrue(field) => {
                    let mut ir_ops = push_offset_ops_for_field(&field);
                    ir_ops.push(OpIR::BranchIfTrue {
                        label: label.clone(),
                    });
                    ir_ops
                }
                BranchCondition::IfFalse(field) => {
                    let mut ir_ops = push_offset_ops_for_field(&field);
                    ir_ops.push(OpIR::BranchIfFalse {
                        label: label.clone(),
                    });
                    ir_ops
                }
                BranchCondition::Equals(field, typed_value) => {
                    let mut ir_ops = push_offset_ops_for_field(&field);
                    ir_ops.push(OpIR::BranchCmp {
                        comparison: Comparison::Equals,
                        rhs: typed_value.clone(),
                        label: label.clone(),
                    });
                    ir_ops
                }
                BranchCondition::NotEquals(field, typed_value) => {
                    let mut ir_ops = push_offset_ops_for_field(&field);
                    ir_ops.push(OpIR::BranchCmp {
                        comparison: Comparison::NotEquals,
                        rhs: typed_value.clone(),
                        label: label.clone(),
                    });
                    ir_ops
                }
                BranchCondition::LessThan(field, typed_value) => {
                    let mut ir_ops = push_offset_ops_for_field(&field);
                    ir_ops.push(OpIR::BranchCmp {
                        comparison: Comparison::LessThan,
                        rhs: typed_value.clone(),
                        label: label.clone(),
                    });
                    ir_ops
                }
                BranchCondition::GreaterThan(field, typed_value) => {
                    let mut ir_ops = push_offset_ops_for_field(&field);
                    ir_ops.push(OpIR::BranchCmp {
                        comparison: Comparison::GreaterThan,
                        rhs: typed_value.clone(),
                        label: label.clone(),
                    });
                    ir_ops
                }
                BranchCondition::LessEquals(field, typed_value) => {
                    let mut ir_ops = push_offset_ops_for_field(&field);
                    ir_ops.push(OpIR::BranchCmp {
                        comparison: Comparison::LessEquals,
                        rhs: typed_value.clone(),
                        label: label.clone(),
                    });
                    ir_ops
                }
                BranchCondition::GreaterEquals(field, typed_value) => {
                    let mut ir_ops = push_offset_ops_for_field(&field);
                    ir_ops.push(OpIR::BranchCmp {
                        comparison: Comparison::GreaterEquals,
                        rhs: typed_value.clone(),
                        label: label.clone(),
                    });
                    ir_ops
                }
            };
            ir_ops
                .into_iter()
                .map(|op_ir| OpIRLine {
                    labels: vec![],
                    op_ir,
                })
                .collect()
        }
        InputOp::Jump { label } => vec![OpIRLine {
            labels: vec![],
            op_ir: OpIR::Jump {
                label: label.clone(),
            },
        }],
        InputOp::CallSub { sub } => vec![OpIRLine {
            labels: vec![],
            op_ir: OpIR::CallSub { sub: sub.clone() },
        }],
        InputOp::Return => vec![OpIRLine {
            labels: vec![],
            op_ir: OpIR::Return,
        }],
        InputOp::Wait { updates } => vec![OpIRLine {
            labels: vec![],
            op_ir: OpIR::Wait { updates: *updates },
        }],
        InputOp::RunCustom { id } => vec![OpIRLine {
            labels: vec![],
            op_ir: OpIR::RunCustom { id: id.clone() },
        }],
        InputOp::BranchCustom { id, label } => vec![OpIRLine {
            labels: vec![],
            op_ir: OpIR::BranchCustom {
                id: id.clone(),
                label: label.clone(),
            },
        }],
    };

    ir_lines[0].labels = input.labels.clone();
    ir_lines
}

fn generate_ir(input: &[InputLine]) -> Vec<OpIRLine> {
    input.into_iter().flat_map(input_to_ir_single).collect()
}

fn end_script_section(script: &mut Vec<OpIRLine>) {
    // The previous subroutine, as well as the main script must end with a return or
    // jump, otherwise execution would continue past the end of the
    // script/subroutine.
    let needs_return = match script.last().unwrap().op_ir {
        OpIR::Return | OpIR::Jump { .. } => false,
        _ => true,
    };

    if needs_return {
        script.push(OpIRLine {
            labels: vec![],
            op_ir: OpIR::Return,
        });
    }
}

fn append_subroutine(script: &mut Vec<OpIRLine>, name: &str, mut sub: Vec<OpIRLine>) {
    let subroutine_start = script.len();
    script.append(&mut sub);
    let subroutine_label = format!("sub-{name}");
    script[subroutine_start]
        .labels
        .push(subroutine_label.clone());
    script.iter_mut().for_each(|line| match &mut line.op_ir {
        OpIR::CallSub { sub } => {
            if sub == name {
                *sub = subroutine_label.clone()
            }
        }
        _ => {}
    });
}

pub(super) fn sequence_to_ir(sequence: &Sequence) -> Vec<OpIRLine> {
    let mut main_ir = generate_ir(&sequence.script);
    end_script_section(&mut main_ir);

    for (sub_name, sub_script) in sequence.subs.iter() {
        let mut sub_ir = generate_ir(&sub_script);
        end_script_section(&mut sub_ir);
        append_subroutine(&mut main_ir, &sub_name, sub_ir);
    }

    main_ir
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{sequence_to_ir, OpIR, OpIRLine};
    use crate::assets::{AssetMetaData, AssetSource, AssetType};
    use crate::generate::sequences::ir::Comparison;
    use crate::parse::sequences::{
        BranchCondition, Field, FieldPathSegment, InputLine, InputOp, Sequence,
    };
    use crate::parse::values::{Type, TypedValue};

    #[test]
    fn test_compile_to_ir() {
        macro_rules! input_line {
            ($input_op:expr) => {
                InputLine {
                    labels: vec![],
                    input_op: $input_op
                }
            };
            ($($label:expr),+ => $input_op:expr) => {
                InputLine {
                    labels: vec![$($label.to_owned()),+],
                    input_op: $input_op
                }
            };
        }

        macro_rules! ir_line {
            ($input_op:expr) => {
                OpIRLine {
                    labels: vec![],
                    op_ir: $input_op
                }
            };
            ($($label:expr),+ => $input_op:expr) => {
                OpIRLine {
                    labels: vec![$($label.to_owned()),+],
                    op_ir: $input_op
                }
            };
        }

        let sequence = Sequence {
            meta: AssetMetaData {
                atype: AssetType::Sequence,
                id: 0,
                name: "TestSequence".to_owned(),
                source: AssetSource::Path(PathBuf::new()),
            },
            target_node_name: "TestNode1".to_owned(),
            subs: [(
                "sub1".to_owned(),
                vec![input_line!(InputOp::Wait { updates: 5 })],
            )]
            .into(),
            script: vec![
                input_line!(
                    "start" => InputOp::Set {
                        field: Field {
                            path: vec![
                                FieldPathSegment("TestNode1".to_owned(), "static-1".to_owned()),
                                FieldPathSegment("TestNode2".to_owned(), "prop-2".to_owned())
                            ],
                            typename: Type::U8
                        },
                        val: TypedValue::U8(5)
                    }
                ),
                input_line!(
                    "second" => InputOp::Branch {
                        condition: BranchCondition::Equals(
                            Field {
                                path: vec![
                                    FieldPathSegment("TestNode1".to_owned(), "prop-1".to_owned())
                                ],
                                typename: Type::U16
                            },
                            TypedValue::U16(10)
                        ),
                        label: "start".to_owned()
                    }
                ),
                input_line!(InputOp::CallSub {
                    sub: "sub1".to_owned()
                }),
                input_line!(InputOp::Jump {
                    label: "second".to_owned()
                }),
            ],
        };

        let ir = sequence_to_ir(&sequence);

        assert_eq!(
            ir,
            vec![
                ir_line!(
                    "start" => OpIR::PushOffset(
                        "TestNode1".to_owned(),
                        "static_1".to_owned()
                    )
                ),
                ir_line!(OpIR::PushOffset(
                    "TestNode2".to_owned(),
                    "prop_2".to_owned()
                )),
                ir_line!(OpIR::SetField {
                    val: TypedValue::U8(5)
                }),
                ir_line!(
                    "second" => OpIR::PushOffset(
                        "TestNode1".to_owned(),
                        "prop_1".to_owned()
                    )
                ),
                ir_line!(OpIR::BranchCmp {
                    comparison: Comparison::Equals,
                    rhs: TypedValue::U16(10),
                    label: "start".to_owned()
                }),
                ir_line!(OpIR::CallSub {
                    sub: "sub-sub1".to_owned()
                }),
                ir_line!(OpIR::Jump {
                    label: "second".to_owned()
                }),
                ir_line!("sub-sub1" => OpIR::Wait { updates: 5 }),
                ir_line!(OpIR::Return)
            ]
        )
    }
}
