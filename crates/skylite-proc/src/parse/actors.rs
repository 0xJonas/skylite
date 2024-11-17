use std::{fs::read_to_string, path::Path};

use crate::{parse::{scheme_util::{eval_str, parse_symbol, with_guile}, util::{change_case, IdentCase}}, SkyliteProcError};

use super::{guile::{scm_car, scm_cdr, scm_is_false, scm_is_null, scm_list_p, scm_pair_p, SCM}, scheme_util::{assq_str, form_to_string, iter_list, parse_string}, values::{parse_argument_list, parse_variable_definition, TypedValue, Variable}};

#[derive(Debug, PartialEq)]
pub(crate) struct Action {
    pub name: String,
    pub params: Vec<Variable>,
    pub description: Option<String>
}

impl Action {
    pub(crate) fn from_scheme(def: SCM) -> Result<Action, SkyliteProcError> {
        unsafe {
            if scm_is_false(scm_list_p(def)) && !scm_is_null(def) {
                return Err(SkyliteProcError::DataError(format!("Expected list for action definition, got {}", form_to_string(def))));
            }

            let name = parse_symbol(scm_car(def))?;

            let tail = scm_cdr(def);
            if scm_is_null(tail) {
                return Ok(Action { name, params: Vec::new(), description: None });
            }

            let params = iter_list(scm_car(tail))?
                .map(|p| parse_variable_definition(p))
                .collect::<Result<Vec<Variable>, SkyliteProcError>>()?;

            let tail = scm_cdr(tail);
            if scm_is_null(tail) {
                return Ok(Action { name, params, description: None });
            }

            let description = Some(parse_string(scm_car(tail))?);

            Ok(Action {
                name, params, description
            })
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct ActionInstance {
    pub name: String,
    pub args: Vec<TypedValue>
}

impl ActionInstance {
    pub fn from_scheme(def: SCM, actions: &[Action]) -> Result<ActionInstance, SkyliteProcError> {
        unsafe {
            if scm_is_false(scm_pair_p(def)) && !scm_is_null(def) {
                return Err(SkyliteProcError::DataError(format!("Expected list for action instantiation, got {}", form_to_string(def))));
            }

            let name = parse_symbol(scm_car(def))?;
            let action = match actions.iter().find(|a| a.name == name) {
                Some(a) => a,
                None => return Err(SkyliteProcError::DataError(format!("No action {} found", name)))
            };

            let args = parse_argument_list(scm_cdr(def), &action.params)?;

            Ok(ActionInstance {
                name, args
            })
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct Actor {
    pub name: String,
    pub parameters: Vec<Variable>,
    pub actions: Vec<Action>,
    pub initial_action: ActionInstance
}

impl Actor {
    pub fn from_scheme(def: SCM, name: &str) -> Result<Actor, SkyliteProcError> {
        unsafe {
            if scm_is_false(scm_pair_p(def)) && !scm_is_null(def) {
                return Err(SkyliteProcError::DataError(format!("Expected list for actor, got {}", form_to_string(def))));
            }

            let maybe_parameters = assq_str("parameters", def)?;
            let maybe_actions = assq_str("actions", def)?;
            let maybe_initial_action = assq_str("initial-action", def)?;

            let parameters = if let Some(ps) = maybe_parameters {
                iter_list(ps)?
                    .map(|p| parse_variable_definition(p))
                    .collect::<Result<Vec<Variable>, SkyliteProcError>>()?
            } else {
                Vec::new()
            };

            let actions = if let Some(cs) = maybe_actions {
                iter_list(cs)?
                    .map(|a| if scm_is_false(scm_pair_p(a)) {
                        Err(SkyliteProcError::DataError(format!("Expected (name params [description]) for action definition, got {}", form_to_string(a))))
                    } else {
                        Action::from_scheme(a)
                    })
                    .collect::<Result<Vec<Action>, SkyliteProcError>>()?
            } else {
                return Err(SkyliteProcError::DataError(format!("Actor must contain at least one action")));
            };

            let initial_action = if let Some(action) = maybe_initial_action {
                ActionInstance::from_scheme(action, &actions)?
            } else {
                return Err(SkyliteProcError::DataError(format!("Missing required field 'initial-action'")));
            };

            Ok(Actor {
                name: name.to_owned(), parameters, actions, initial_action
            })
        }
    }

    pub(crate) fn from_file(path: &Path) -> Result<Actor, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_file_guile(path: &Path) -> Result<Actor, SkyliteProcError> {
            let definition_raw = read_to_string(path).map_err(|e| SkyliteProcError::OtherError(format!("Error reading project definition: {}", e)))?;
            let definition = unsafe {
                eval_str(&definition_raw)?
            };
            let name = &path.file_stem().unwrap().to_string_lossy();
            Actor::from_scheme(definition, &name)
        }

        with_guile(from_file_guile, path)
    }
}

#[cfg(test)]
mod tests {
    use crate::parse::{actors::{Action, ActionInstance, TypedValue}, scheme_util::{eval_str, with_guile}, values::{Type, Variable}};

    use super::Actor;


    extern "C" fn test_parse_actor_impl(_: &()) {
        unsafe {
            let def = eval_str("
                '((parameters . ((x u16) (y u16)))
                  (actions .
                    ((action1 ((dx u8) (dy u8)) \"action 1\")
                     (action2 ((val u8)) \"test\")
                     (action3)))
                   (initial-action . (action2 5)))").unwrap();
            let actor = Actor::from_scheme(def, "TestActor").unwrap();
            assert_eq!(actor, Actor {
                name: "TestActor".to_owned(),
                parameters: vec![
                    Variable { name: "x".to_owned(), typename: Type::U16, documentation: None, default: None },
                    Variable { name: "y".to_owned(), typename: Type::U16, documentation: None, default: None },
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
                            Variable { name: "val".to_owned(), typename: Type::U8, documentation: None, default: None }
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
            });
        }
    }

    #[test]
    fn test_parse_actor() {
        with_guile(test_parse_actor_impl, &());
    }
}
