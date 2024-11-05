use std::{collections::HashMap, fs::read_to_string, path::Path};

use crate::{parse::{guile::scm_pair_p, scheme_util::{eval_str, iter_list, with_guile}, util::{change_case, IdentCase}, values::parse_variable_definition}, SkyliteProcError};

use super::{actors::Actor, guile::{scm_car, scm_cdr, scm_is_false, scm_list_p, SCM}, scheme_util::{assq_str, form_to_string, parse_symbol}, values::{parse_argument_list, TypedValue, Variable}};

#[derive(Debug, PartialEq)]
pub(crate) struct ActorInstance {
    pub actor_name: String,
    pub args: Vec<TypedValue>
}

impl ActorInstance {
    fn from_scheme(form: SCM, actors: &[Actor]) -> Result<ActorInstance, SkyliteProcError> {
        unsafe {
            if scm_is_false(scm_list_p(form)) {
                return Err(SkyliteProcError::DataError(format!("Expected list for actor instantiation, got {}", form_to_string(form))));
            }

            // Parse actor name
            let actor_name = parse_symbol(scm_car(form))?;
            let actor = match actors.iter().find(|a| a.name == actor_name) {
                Some(a) => a,
                None => return Err(SkyliteProcError::DataError(format!("Actor {} not found", actor_name)))
            };

            // Parse instance arguments
            let args_raw = scm_cdr(form);
            let args = parse_argument_list(args_raw, &actor.parameters)?;
            Ok(ActorInstance {
                actor_name, args
            })
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct Scene {
    pub name: String,
    pub actors: HashMap<String, ActorInstance>,
    pub extras: Vec<ActorInstance>,
    pub parameters: Vec<Variable>
}

impl Scene {
    fn from_scheme(form: SCM, name: &str, actors: &[Actor]) -> Result<Scene, SkyliteProcError> {
        unsafe {
            let maybe_actors_scm = assq_str("actors", form)?;
            let maybe_extras_scm = assq_str("extras", form)?;
            let maybe_params_scm = assq_str("parameters", form)?;

            let actor_instances = if let Some(actors_scm) = maybe_actors_scm {
                iter_list(actors_scm)?
                    .map(|e| if scm_is_false(scm_pair_p(e)) {
                            Err(SkyliteProcError::DataError(format!("Expected pair (name . instance) for actor, got {}", form_to_string(e))))
                        } else {
                            Ok((parse_symbol(scm_car(e))?, ActorInstance::from_scheme(scm_cdr(e), actors)?))
                        })
                    .collect::<Result<HashMap<String, ActorInstance>, SkyliteProcError>>()?
            } else {
                HashMap::new()
            };

            let extras = if let Some(extras_scm) = maybe_extras_scm {
                iter_list(extras_scm)?
                    .map(|extra| ActorInstance::from_scheme(extra, actors))
                    .collect::<Result<Vec<ActorInstance>, SkyliteProcError>>()?
            } else {
                Vec::new()
            };

            let parameters = if let Some(parameters_scm) = maybe_params_scm {
                iter_list(parameters_scm)?
                    .map(|param| parse_variable_definition(param))
                    .collect::<Result<Vec<Variable>, SkyliteProcError>>()?
            } else {
                Vec::new()
            };

            Ok(Scene {
                name: name.to_owned(),
                actors: actor_instances,
                extras,
                parameters
            })
        }
    }

    pub(crate) fn from_file(path: &Path, actors: &[Actor]) -> Result<Scene, SkyliteProcError> {
        // Since we are not actually accessing anything from this signature from C,
        // we can get away with ignoring the missing C representations.
        #[allow(improper_ctypes_definitions)]
        extern "C" fn from_file_guile(params: &(&Path, &[Actor])) -> Result<Scene, SkyliteProcError> {
            let (path, actors) = params;
            let definition_raw = read_to_string(path).map_err(|e| SkyliteProcError::OtherError(format!("Error reading project definition: {}", e)))?;
            let definition = unsafe {
                eval_str(&definition_raw)?
            };

            let name = change_case(&path.file_stem().unwrap().to_string_lossy(), IdentCase::UpperCamelCase);
            Scene::from_scheme(definition, &name, actors)
        }

        with_guile(from_file_guile, &(path, actors))
    }
}

/// Reduced representation of a Scene.
///
/// This is used by scene_definition, so the proc-macro only has to parse
/// the stuff it actually needs (specifically it does not have to parse all
/// actors to match actor instantiations).
pub(crate) struct SceneStub {
    name: String,
    actor_names: Vec<String>,
    parameters: Vec<Variable>
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::parse::scenes::{ActorInstance, TypedValue};
    use crate::parse::scheme_util::{eval_str, with_guile};

    use crate::parse::actors::Actor;
    use crate::parse::values::{Type, Variable};

    use super::Scene;

    extern "C" fn test_parse_scene_impl(_: &()) {
        let def_scm = unsafe {
            eval_str("'
            ((actors .
               ((a1 . (TestActor 1))
                (a2 . (TestActor 2))))
             (extras . ((TestActor 3) (TestActor 4)))
             (parameters . ((val1 u8))))
            ").unwrap()
        };
        let test_actor = unsafe { Actor::from_scheme(eval_str("
            '((parameters . ((val u8)))
              (actions .
                ((default)))
              (initial-action . (default)))").unwrap(), "TestActor").unwrap()
        };
        let scene = Scene::from_scheme(def_scm, "TestScene", &[test_actor]).unwrap();

        assert_eq!(scene,
            Scene {
                name: "TestScene".to_owned(),
                actors: HashMap::from([
                    ("a1".to_owned(), ActorInstance { actor_name: "TestActor".to_owned(), args: vec![TypedValue::U8(1)] }),
                    ("a2".to_owned(), ActorInstance { actor_name: "TestActor".to_owned(), args: vec![TypedValue::U8(2)] }),
                ]),
                extras: vec![
                    ActorInstance { actor_name: "TestActor".to_owned(), args: vec![TypedValue::U8(3)] },
                    ActorInstance { actor_name: "TestActor".to_owned(), args: vec![TypedValue::U8(4)] },
                ],
                parameters: vec![
                    Variable { name: "val1".to_owned(), typename: Type::U8, documentation: None, default: None}
                ]
            }
        );
    }

    #[test]
    fn test_parse_scene() {
        with_guile(test_parse_scene_impl, &());
    }
}
