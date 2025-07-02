use std::collections::HashMap;

use crate::assets::{AssetMetaData, Assets};
use crate::parse::guile::{
    scm_car, scm_cdr, scm_is_false, scm_is_symbol, scm_is_true, scm_list_p, scm_pair_p,
    scm_symbol_p, SCM,
};
use crate::parse::scheme_util::{
    assq_str, form_to_string, iter_list, parse_bool, parse_f32, parse_f64, parse_int, parse_string,
    parse_symbol, with_guile,
};
use crate::parse::values::{parse_typed_value, Type, TypedValue};
use crate::SkyliteProcError;

fn parse_field(field_path: &str) -> Vec<String> {
    field_path.split('.').map(ToOwned::to_owned).collect()
}

fn expect_args(items: &[SCM], num: usize, context: &str) -> Result<(), SkyliteProcError> {
    if items.len() - 1 != num {
        Err(syntax_err!(
            "{context}: expected {num} arguments, got {}",
            items.len() - 1
        ))
    } else {
        Ok(())
    }
}

/// Condition of a branch operation.
#[derive(Debug, PartialEq, Clone)]
pub(crate) enum BranchCondition {
    IfTrue(Field),
    IfFalse(Field),
    Equals(Field, TypedValue),
    NotEquals(Field, TypedValue),
    LessThan(Field, TypedValue),
    GreaterThan(Field, TypedValue),
    LessEquals(Field, TypedValue),
    GreaterEquals(Field, TypedValue),
}

impl BranchCondition {
    /// Directly parses a BranchCondition from an SCM value.
    pub(crate) fn from_scheme(
        definition: SCM,
        target_node_name: &str,
        assets: &mut Assets,
    ) -> Result<BranchCondition, SkyliteProcError> {
        unsafe {
            if scm_is_true(scm_symbol_p(definition)) {
                let field = parse_field(&parse_symbol(definition).unwrap());
                let field = resolve_field(&field, target_node_name, assets)?;
                if let Type::Bool = field.typename {
                    return Ok(BranchCondition::IfTrue(field));
                } else {
                    return Err(data_err!("Expected bool for branch condition."));
                }
            }

            let items: Vec<SCM> = iter_list(definition)?.collect();
            match parse_symbol(items[0])?.as_str() {
                // (! field)
                "!" => {
                    expect_args(&items, 1, "branch if false")?;
                    let field_path = parse_field(&parse_symbol(items[1])?);
                    let field = resolve_field(&field_path, target_node_name, assets)?;
                    if let Type::Bool = field.typename {
                        Ok(BranchCondition::IfFalse(field))
                    } else {
                        Err(data_err!("Expected bool for branch condition."))
                    }
                }
                // (= field 5)
                "=" | "==" => {
                    expect_args(&items, 2, "branch if equals (=)")?;
                    let field_path = parse_field(&parse_symbol(items[1])?);
                    let field = resolve_field(&field_path, target_node_name, assets)?;
                    let value = parse_typed_value_for_primitive(&field.typename, items[2])?;
                    Ok(BranchCondition::Equals(field, value))
                }

                // (!= field 5)
                "!=" => {
                    expect_args(&items, 2, "branch if not equals (!=)")?;
                    let field_path = parse_field(&parse_symbol(items[1])?);
                    let field = resolve_field(&field_path, target_node_name, assets)?;
                    let value = parse_typed_value_for_primitive(&field.typename, items[2])?;
                    Ok(BranchCondition::NotEquals(field, value))
                }

                // (< field 5)
                "<" => {
                    expect_args(&items, 2, "branch if less than (<)")?;
                    let field_path = parse_field(&parse_symbol(items[1])?);
                    let field = resolve_field(&field_path, target_node_name, assets)?;
                    expect_numeric_type(&field.typename)?;
                    let value = parse_typed_value_for_primitive(&field.typename, items[2])?;
                    Ok(BranchCondition::LessThan(field, value))
                }

                // (> field 5)
                ">" => {
                    expect_args(&items, 2, "branch if greater than (>)")?;
                    let field_path = parse_field(&parse_symbol(items[1])?);
                    let field = resolve_field(&field_path, target_node_name, assets)?;
                    expect_numeric_type(&field.typename)?;
                    let value = parse_typed_value_for_primitive(&field.typename, items[2])?;
                    Ok(BranchCondition::GreaterThan(field, value))
                }

                // (<= field 5)
                "<=" => {
                    expect_args(&items, 2, "branch if less or equals (<=)")?;
                    let field_path = parse_field(&parse_symbol(items[1])?);
                    let field = resolve_field(&field_path, target_node_name, assets)?;
                    expect_numeric_type(&field.typename)?;
                    let value = parse_typed_value_for_primitive(&field.typename, items[2])?;
                    Ok(BranchCondition::LessEquals(field, value))
                }

                // (>= field 5)
                ">=" => {
                    expect_args(&items, 2, "branch if greater or equals (>=)")?;
                    let field_path = parse_field(&parse_symbol(items[1])?);
                    let field = resolve_field(&field_path, target_node_name, assets)?;
                    expect_numeric_type(&field.typename)?;
                    let value = parse_typed_value_for_primitive(&field.typename, items[2])?;
                    Ok(BranchCondition::GreaterEquals(field, value))
                }

                // (field)
                field if items.len() == 1 => {
                    let field_path = parse_field(field);
                    let field = resolve_field(&field_path, target_node_name, assets)?;
                    if let Type::Bool = field.typename {
                        Ok(BranchCondition::IfTrue(field))
                    } else {
                        Err(data_err!("Expected bool for branch condition."))
                    }
                }
                op => Err(syntax_err!("Unknown operator {op}")),
            }
        }
    }
}

/// A Segment of the path of a `Field`. A path consists of
/// a chain of static node and property references. Each segment
/// contains the Node for which it applies as well as either the
/// static node or property it is referencing.
#[derive(Debug, PartialEq, Clone)]
pub(crate) enum FieldPathSegment {
    StaticNode(String, String),
    Property(String, String),
}

/// Information on a field used in an `InputOp`.
#[derive(Debug, PartialEq, Clone)]
pub(crate) struct Field {
    pub path: Vec<FieldPathSegment>,
    pub typename: Type,
}

fn resolve_field(
    path: &[String],
    target_node_name: &str,
    assets: &mut Assets,
) -> Result<Field, SkyliteProcError> {
    let field_name = path[path.len() - 1].as_str();
    let mut current_node_name = target_node_name.to_owned();
    let mut segments = Vec::new();
    for segment in path[..path.len() - 1].iter() {
        let node = assets.load_node(&current_node_name).unwrap();
        let next_node_name = node
            .static_nodes
            .iter()
            .find(|(name, _)| name == segment)
            .ok_or(data_err!("Static node not found on node: {segment}"))?
            .1
            .name
            .clone();

        segments.push(FieldPathSegment::StaticNode(
            node.meta.name.clone(),
            segment.clone(),
        ));

        current_node_name = next_node_name;
    }

    let bottom_node = assets.load_node(&current_node_name).unwrap();

    let typename = bottom_node
        .properties
        .iter()
        .find(|v| v.name == field_name)
        .ok_or(data_err!("Property not found one node: {field_name}"))?
        .typename
        .clone();

    segments.push(FieldPathSegment::Property(
        bottom_node.meta.name.clone(),
        field_name.to_owned(),
    ));

    Ok(Field {
        path: segments,
        typename,
    })
}

/// Like `parse_typed_value`, but only for types that are supported
/// by the `modify` and `branch` operations.
unsafe fn parse_typed_value_for_primitive(
    typename: &Type,
    data: SCM,
) -> Result<TypedValue, SkyliteProcError> {
    match typename {
        Type::U8 => Ok(TypedValue::U8(parse_int(data)?)),
        Type::U16 => Ok(TypedValue::U16(parse_int(data)?)),
        Type::U32 => Ok(TypedValue::U32(parse_int(data)?)),
        Type::U64 => Ok(TypedValue::U64(parse_int(data)?)),
        Type::I8 => Ok(TypedValue::I8(parse_int(data)?)),
        Type::I16 => Ok(TypedValue::I16(parse_int(data)?)),
        Type::I32 => Ok(TypedValue::I32(parse_int(data)?)),
        Type::I64 => Ok(TypedValue::I64(parse_int(data)?)),
        Type::F32 => Ok(TypedValue::F32(parse_f32(data)?)),
        Type::F64 => Ok(TypedValue::F64(parse_f64(data)?)),
        Type::Bool => Ok(TypedValue::Bool(parse_bool(data)?)),
        Type::String => Ok(TypedValue::String(parse_string(data)?)),
        _ => Err(data_err!("Type not supported for operation.")),
    }
}

fn expect_numeric_type(typename: &Type) -> Result<(), SkyliteProcError> {
    match typename {
        Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::F32
        | Type::F64 => Ok(()),
        Type::Bool | Type::String | Type::Tuple(_) | Type::Vec(_) | Type::NodeList => {
            Err(data_err!("Expected numeric type"))
        }
    }
}

/// Single operation in a `Sequence` script. The set of input operations
/// are those available to sequence assets and differ slightly from the
/// lower-level operations used by skylite_core.
#[derive(Debug, PartialEq, Clone)]
pub(crate) enum InputOp {
    /// Sets a field to the specified value.
    Set { field: Field, val: TypedValue },

    /// Adds a value to a numeric field.
    Modify { field: Field, delta: TypedValue },

    /// Jumps to a label when a condition is fulfilled.
    Branch {
        condition: BranchCondition,
        label: String,
    },

    /// Unconditionally jump to a label.
    Jump { label: String },

    /// Call a subroutine defined in the `subs` key in the sequence asset.
    CallSub { sub: String },

    /// Return from a subroutine.
    Return,

    /// Wait the given number of updates.
    Wait { updates: u16 },

    /// Call custom function. The code for the function must be defined through
    /// a sequence_definition.
    RunCustom { id: String },

    /// Branch based on the result of a custom function. The function must be
    /// defined through a sequence_definition.
    BranchCustom { id: String, label: String },
}

impl InputOp {
    fn from_scheme(
        definition: SCM,
        target_node_name: &str,
        assets: &mut Assets,
    ) -> Result<InputOp, SkyliteProcError> {
        unsafe {
            let items: Vec<SCM> = iter_list(definition)?.collect();
            if items.len() == 0 {
                return Err(syntax_err!("Expected sequence directive, got empty list"));
            }
            let mnemonic = parse_symbol(items[0])?;

            match mnemonic.as_str() {
                // (set field 5)
                "set" => {
                    expect_args(&items, 2, "set")?;
                    let field_path = parse_field(&parse_symbol(items[1])?);
                    let field = resolve_field(&field_path, target_node_name, assets)?;
                    let val = parse_typed_value(&field.typename, items[2], &assets.index)?;
                    Ok(InputOp::Set { field, val })
                }

                // (modify field 5)
                "modify" => {
                    expect_args(&items, 2, "modify")?;
                    let field_path = parse_field(&parse_symbol(items[1])?);
                    let field = resolve_field(&field_path, target_node_name, assets)?;
                    let delta = parse_typed_value_for_primitive(&field.typename, items[2])?;
                    Ok(InputOp::Modify { field, delta })
                }

                // (branch condition label)
                "branch" => {
                    expect_args(&items, 2, "branch")?;
                    let condition =
                        BranchCondition::from_scheme(items[1], target_node_name, assets)?;
                    let label = parse_symbol(items[2])?;
                    Ok(InputOp::Branch { condition, label })
                }
                // (jump label)
                "jump" => {
                    expect_args(&items, 1, "jump")?;
                    let label = parse_symbol(items[1])?;
                    Ok(InputOp::Jump { label })
                }

                // (call sub)
                "call" => {
                    expect_args(&items, 1, "call")?;
                    let sub = parse_symbol(items[1])?;
                    Ok(InputOp::CallSub { sub })
                }

                // (return)
                "return" => {
                    expect_args(&items, 0, "return")?;
                    Ok(InputOp::Return)
                }

                // (wait 5)
                "wait" => {
                    expect_args(&items, 1, "wait")?;
                    let updates = parse_int(items[1])?;
                    Ok(InputOp::Wait { updates })
                }

                // (run-custom name)
                "run-custom" => {
                    expect_args(&items, 1, "run-custom")?;
                    let id = parse_symbol(items[1])?;
                    Ok(InputOp::RunCustom { id })
                }

                // (branch-custom branch_fn label)
                "branch-custom" => {
                    expect_args(&items, 2, "branch-custom")?;
                    let id = parse_symbol(items[1])?;
                    let label = parse_symbol(items[2])?;
                    Ok(InputOp::BranchCustom { id, label })
                }
                _ => {
                    return Err(syntax_err!("Illegal sequence directive '{mnemonic}'"));
                }
            }
        }
    }
}

/// Single line in a Sequence's script, consisting of a single operation
/// and optional labels.
#[derive(Debug, PartialEq, Clone)]
pub(crate) struct InputLine {
    pub labels: Vec<String>,
    pub input_op: InputOp,
}

fn validate_labels(script: &[InputLine]) -> Result<(), SkyliteProcError> {
    for (i, line) in script.iter().enumerate() {
        let maybe_label = match &line.input_op {
            InputOp::Jump { label } => Some(label),
            InputOp::Branch { label, .. } => Some(label),
            InputOp::BranchCustom { label, .. } => Some(label),
            _ => None,
        };
        if let Some(label) = maybe_label {
            let first_char = label.chars().next().unwrap();
            // Labels starting with '-' are backwards searching anonymous labels.
            let search_range = if first_char == '-' {
                0..i
            }
            // Labels starting with '+' are forwards searching anonymous labels.
            else if first_char == '+' {
                (i + 1)..script.len()
            }
            // Normal labels
            else {
                0..script.len()
            };

            script[search_range]
                .iter()
                .find(|l| l.labels.contains(label))
                .ok_or(data_err!("Jump target {label} not found"))?;

            // TODO: Prevent the same named label to refer to different indices
        }
    }

    Ok(())
}

// TODO: This step should really be done after converting
// the InputLines to IR, but it becomes more difficult then.
// The reason for this is that branch ops get preceeded by
// PushOffset ops, so the case '- (branch -)' becomes more
// complicated to handle.
fn rename_labels(input: &mut [InputLine], name: &str) {
    fn is_forward_label(label: &str) -> bool {
        label.chars().next().unwrap() == '+'
    }

    fn is_backward_label(label: &str) -> bool {
        label.chars().next().unwrap() == '-'
    }

    fn get_jump_target(input_op: &mut InputOp) -> Option<&mut String> {
        match input_op {
            InputOp::Jump { label: target } => Some(target),
            InputOp::Branch { label: target, .. } => Some(target),
            InputOp::BranchCustom { label: target, .. } => Some(target),
            _ => None,
        }
    }

    let mut anonymous_labels: HashMap<String, usize> = HashMap::new();
    for line in input.iter_mut() {
        let mut target = get_jump_target(&mut line.input_op);

        // Rename normal jump targets and backwards anonymous labels first,
        // then rename labels, then rename forwards anonymous labels.
        // This order is to make sure the following works correctly:
        //
        // - (...)
        // - (jump -) ; Jumps to previous line (jump -) ; Jumps to previous line
        //
        //   (jump +) ; Jumps to next line
        // + (jump +) ; Jumps to next line
        // + (...)

        if let Some(ref mut label) = target {
            if is_backward_label(label) {
                // Entry must exist, otherwise the validation during parsing would have failed.
                let idx = anonymous_labels.get(*label).unwrap();
                **label = format!("{name}-b-{label}-{idx}");
            } else if !is_forward_label(label) {
                **label = format!("{name}-l-{label}");
            }
        }

        for label in line.labels.iter_mut() {
            if is_backward_label(label) {
                let idx = if let Some(idx) = anonymous_labels.get(label) {
                    idx + 1
                } else {
                    0
                };
                anonymous_labels.insert(label.to_owned(), idx);

                *label = format!("{name}-b-{label}-{idx}");
            } else if is_forward_label(label) {
                let idx = anonymous_labels.get(label).unwrap_or(&0);
                let org_label = label.clone();
                *label = format!("{name}-f-{label}-{idx}");
                anonymous_labels.insert(org_label, *idx + 1);
            } else {
                *label = format!("{name}-l-{label}");
            }
        }

        if let Some(label) = target {
            if is_forward_label(label) {
                let idx = anonymous_labels.entry(label.to_owned()).or_insert(0);
                *label = format!("{name}-f-{label}-{idx}");
            }
        }
    }
}

fn parse_script(
    definition: SCM,
    script_name: &str,
    target_node_name: &str,
    assets: &mut Assets,
) -> Result<Vec<InputLine>, SkyliteProcError> {
    let mut labels = Vec::new();
    let mut script = Vec::new();

    unsafe {
        for item in iter_list(definition)? {
            if scm_is_symbol(item) {
                labels.push(parse_symbol(item).unwrap());
            } else if scm_is_true(scm_list_p(item)) {
                let input_op = InputOp::from_scheme(item, target_node_name, assets)?;
                script.push(InputLine {
                    input_op,
                    labels: std::mem::take(&mut labels),
                })
            } else {
                return Err(syntax_err!("Expected symbol or list"));
            }
        }
    }

    validate_labels(&script)?;
    rename_labels(&mut script, script_name);
    Ok(script)
}

/// Fully parsed Sequence asset.
#[derive(Debug, PartialEq, Clone)]
pub(crate) struct Sequence {
    pub meta: AssetMetaData,
    pub target_node_name: String,
    pub subs: HashMap<String, Vec<InputLine>>,
    pub script: Vec<InputLine>,
}

impl Sequence {
    fn from_meta_with_guile(
        meta: AssetMetaData,
        assets: &mut Assets,
    ) -> Result<Sequence, SkyliteProcError> {
        let def = meta.source.load_with_guile()?;
        unsafe {
            if scm_is_false(scm_list_p(def)) {
                return Err(syntax_err!(
                    "Expected alist for sequence definition, got {}",
                    form_to_string(def)
                ));
            }

            let target_node_scm = assq_str("node", def)?.ok_or(syntax_err!(
                "Missing required key 'node' for sequence definition."
            ))?;
            let target_node_name = parse_symbol(target_node_scm)?;

            let subs = match assq_str("subs", def)? {
                Some(scm) => iter_list(scm)?
                    .map(|pair| {
                        if scm_is_false(scm_pair_p(pair)) {
                            return Err(syntax_err!("Expected alist for key 'subs'."));
                        }

                        let sub_name = parse_symbol(scm_car(pair))?;
                        let script = parse_script(
                            scm_cdr(pair),
                            &format!("sub-{sub_name}"),
                            &target_node_name,
                            assets,
                        )?;
                        return Ok((sub_name, script));
                    })
                    .collect::<Result<HashMap<String, Vec<InputLine>>, SkyliteProcError>>()?,
                None => HashMap::new(),
            };

            let script = parse_script(
                assq_str("script", def)?.ok_or(syntax_err!(
                    "Missing required key 'script' for sequence definition"
                ))?,
                "main",
                &target_node_name,
                assets,
            )?;

            Ok(Sequence {
                meta,
                target_node_name,
                subs,
                script,
            })
        }
    }

    pub(crate) fn from_meta(
        meta: AssetMetaData,
        assets: &mut Assets,
    ) -> Result<Sequence, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_meta_inner(
            args: (AssetMetaData, &mut Assets),
        ) -> Result<Sequence, SkyliteProcError> {
            let (meta, assets) = args;
            Sequence::from_meta_with_guile(meta, assets)
        }

        with_guile(from_meta_inner, (meta, assets))
    }
}

#[cfg(test)]
mod tests {

    use super::Sequence;
    use crate::assets::tests::create_tmp_fs;
    use crate::assets::Assets;
    use crate::parse::scheme_util::with_guile;
    use crate::parse::sequences::{BranchCondition, Field, FieldPathSegment, InputLine, InputOp};
    use crate::parse::values::{Type, TypedValue};

    extern "C" fn test_parse_sequence_impl(_: &()) {
        let tmp_fs = create_tmp_fs(&[
            (
                "nodes/test-node-1.scm",
                "'((properties . ((prop1 u8) (prop2 bool))) (static-nodes . ((static1 . (test-node-2)))))",
            ),
            ("nodes/test-node-2.scm", "'((properties . ((prop1 u16))))"),
            (
                "sequences/test-sequence.scm",
                r#"
                '((node . test-node-1)
                  (subs .
                    ((sub1 . ((wait 10) (return)))))
                  (script .
                    (
                    start
                      (set prop1 5)
                      (call sub1)
                    -
                      (run-custom custom-fn)
                    - (branch prop2 -)
                      (modify static1.prop1 5)
                      (branch (< prop1 10) -)
                      (branch-custom branch-fn end)
                      (jump start)
                    end
                      (wait 0)
                    )))
                "#,
            ),
        ])
        .unwrap();

        let mut assets = Assets::from_scheme_with_guile(None, tmp_fs.path()).unwrap();
        let sequence = assets.load_sequence("test-sequence").unwrap();

        assert_eq!(
            sequence,
            &Sequence {
                meta: sequence.meta.clone(),
                target_node_name: "test-node-1".to_owned(),
                subs: [(
                    "sub1".to_owned(),
                    vec![
                        InputLine {
                            labels: vec![],
                            input_op: InputOp::Wait { updates: 10 }
                        },
                        InputLine {
                            labels: vec![],
                            input_op: InputOp::Return
                        }
                    ]
                )]
                .into(),
                script: vec![
                    InputLine {
                        labels: vec!["main-l-start".to_owned()],
                        input_op: InputOp::Set {
                            field: Field {
                                path: vec![FieldPathSegment::Property(
                                    "test-node-1".to_owned(),
                                    "prop1".to_owned()
                                )],
                                typename: Type::U8
                            },
                            val: TypedValue::U8(5)
                        }
                    },
                    InputLine {
                        labels: vec![],
                        input_op: InputOp::CallSub {
                            sub: "sub1".to_owned()
                        }
                    },
                    InputLine {
                        labels: vec!["main-b---0".to_owned()],
                        input_op: InputOp::RunCustom {
                            id: "custom-fn".to_owned()
                        }
                    },
                    InputLine {
                        labels: vec!["main-b---1".to_owned()],
                        input_op: InputOp::Branch {
                            condition: BranchCondition::IfTrue(Field {
                                path: vec![FieldPathSegment::Property(
                                    "test-node-1".to_owned(),
                                    "prop2".to_owned()
                                )],
                                typename: Type::Bool
                            }),
                            label: "main-b---0".to_owned()
                        }
                    },
                    InputLine {
                        labels: vec![],
                        input_op: InputOp::Modify {
                            field: Field {
                                path: vec![
                                    FieldPathSegment::StaticNode(
                                        "test-node-1".to_owned(),
                                        "static1".to_owned()
                                    ),
                                    FieldPathSegment::Property(
                                        "test-node-2".to_owned(),
                                        "prop1".to_owned()
                                    )
                                ],
                                typename: Type::U16
                            },
                            delta: TypedValue::U16(5)
                        }
                    },
                    InputLine {
                        labels: vec![],
                        input_op: InputOp::Branch {
                            condition: BranchCondition::LessThan(
                                Field {
                                    path: vec![FieldPathSegment::Property(
                                        "test-node-1".to_owned(),
                                        "prop1".to_owned()
                                    )],
                                    typename: Type::U8
                                },
                                TypedValue::U8(10)
                            ),
                            label: "main-b---1".to_owned()
                        }
                    },
                    InputLine {
                        labels: vec![],
                        input_op: InputOp::BranchCustom {
                            id: "branch-fn".to_owned(),
                            label: "main-l-end".to_owned()
                        }
                    },
                    InputLine {
                        labels: vec![],
                        input_op: InputOp::Jump {
                            label: "main-l-start".to_owned()
                        }
                    },
                    InputLine {
                        labels: vec!["main-l-end".to_owned()],
                        input_op: InputOp::Wait { updates: 0 }
                    }
                ]
            }
        )
    }

    #[test]
    fn test_parse_sequence() {
        with_guile(test_parse_sequence_impl, &())
    }
}
