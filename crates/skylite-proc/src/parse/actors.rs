use std::collections::HashMap;

use crate::{parse::scheme_util::parse_symbol, SkyliteProcError};

use super::{guile::{scm_car, scm_cdr, scm_is_false, scm_is_null, scm_list_p, scm_pair_p, SCM}, scheme_util::{assq_str, form_to_string, iter_list, parse_string}, values::{parse_argument_list, parse_variable_definition, TypedValue, Variable}};

#[derive(Debug, PartialEq)]
pub(crate) struct Action {
    pub params: Vec<Variable>,
    pub description: Option<String>
}

impl Action {
    pub(crate) fn from_scheme(def: SCM) -> Result<Action, SkyliteProcError> {
        unsafe {
            if scm_is_null(def) {
                return Ok(Action { params: vec![], description: None });
            }

            if scm_is_false(scm_list_p(def)) {
                return Err(SkyliteProcError::DataError(format!("Expected list for action definition, got {}", form_to_string(def))));
            }

            let params = iter_list(scm_car(def))?
                .map(|p| parse_variable_definition(p))
                .collect::<Result<Vec<Variable>, SkyliteProcError>>()?;

            let tail = scm_cdr(def);
            let description = if scm_is_null(tail) {
                None
            } else {
                Some(parse_string(scm_car(tail))?)
            };

            Ok(Action {
                params, description
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
    pub fn from_scheme(def: SCM, actions: &HashMap<String, Action>) -> Result<ActionInstance, SkyliteProcError> {
        unsafe {
            if scm_is_false(scm_pair_p(def)) {
                return Err(SkyliteProcError::DataError(format!("Expected list for action instantiation, got {}", form_to_string(def))));
            }

            let name = parse_symbol(scm_car(def))?;
            let action = match actions.get(&name) {
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
    pub actions: HashMap<String, Action>,
    pub initial_action: ActionInstance
}

impl Actor {
    pub fn from_scheme(def: SCM) -> Result<Actor, SkyliteProcError> {
        unsafe {
            if scm_is_false(scm_pair_p(def)) {
                return Err(SkyliteProcError::DataError(format!("Expected list for action, got {}", form_to_string(def))));
            }

            let maybe_name = assq_str("name", def)?;
            let maybe_parameters = assq_str("parameters", def)?;
            let maybe_actions = assq_str("actions", def)?;
            let maybe_initial_action = assq_str("initial-action", def)?;

            let name = if let Some(n) = maybe_name {
                parse_symbol(n)?
            } else {
                return Err(SkyliteProcError::DataError(format!("Missing required field 'name'")));
            };

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
                        Ok((parse_symbol(scm_car(a))?, Action::from_scheme(scm_cdr(a))?))
                    })
                    .collect::<Result<HashMap<String, Action>, SkyliteProcError>>()?
            } else {
                return Err(SkyliteProcError::DataError(format!("Actor must contain at least one action")));
            };

            let initial_action = if let Some(action) = maybe_initial_action {
                ActionInstance::from_scheme(action, &actions)?
            } else {
                return Err(SkyliteProcError::DataError(format!("Missing required field 'initial-action'")));
            };

            Ok(Actor {
                name, parameters, actions, initial_action
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::parse::{actors::{Action, ActionInstance, TypedValue}, scheme_util::{eval_str, with_guile}, values::{Type, Variable}};

    use super::Actor;


    extern "C" fn test_parse_actor_impl(_: &()) {
        unsafe {
            let def = eval_str("
                '((name . TestActor)
                  (parameters . ((x u16) (y u16)))
                  (actions .
                    ((action1 ((dx u8) (dy u8)) \"action 1\")
                     (action2 ((val u8)) \"test\")
                     (action3)))
                   (initial-action . (action2 5)))").unwrap();
            let actor = Actor::from_scheme(def).unwrap();
            assert_eq!(actor, Actor {
                name: "TestActor".to_owned(),
                parameters: vec![
                    Variable { name: "x".to_owned(), typename: Type::U16, documentation: None, default: None },
                    Variable { name: "y".to_owned(), typename: Type::U16, documentation: None, default: None },
                ],
                actions: HashMap::from([
                    ("action1".to_owned(), Action {
                        params: vec![
                            Variable { name: "dx".to_owned(), typename: Type::U8, documentation: None, default: None },
                            Variable { name: "dy".to_owned(), typename: Type::U8, documentation: None, default: None }
                        ],
                        description: Some("action 1".to_owned())
                    }),
                    ("action2".to_owned(), Action {
                        params: vec![
                            Variable { name: "val".to_owned(), typename: Type::U8, documentation: None, default: None }
                        ],
                        description: Some("test".to_owned())
                    }),
                    ("action3".to_owned(), Action {
                        params: vec![],
                        description: None
                    })
                ]),
                initial_action: ActionInstance { name: "action2".to_owned(), args: vec![TypedValue::U8(5)] }
            });
        }
    }

    #[test]
    fn test_parse_actor() {
        with_guile(test_parse_actor_impl, &());
    }
}
