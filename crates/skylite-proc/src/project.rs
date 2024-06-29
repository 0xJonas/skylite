use crate::guile::{scm_is_false, scm_list_p, SCM};
use crate::parse_util::CXROp::{CAR, CDR};
use crate::SkyliteProcError;
use glob::Pattern;
use crate::parse_util::{assq_str, conv_string, conv_symbol, cxr, iter_list, parse_typed_value, TypedValue};

#[derive(PartialEq, Debug)]
struct AssetDirectories {
    actors: Vec<Pattern>,
    scenes: Vec<Pattern>,
    graphics: Vec<Pattern>,
    sprites: Vec<Pattern>,
    tilesets: Vec<Pattern>,
    maps: Vec<Pattern>
}

fn parse_glob_list(list: SCM) -> Result<Vec<Pattern>, SkyliteProcError> {
    let mut out: Vec<Pattern> = Vec::new();
    unsafe {
        for g in iter_list(list)? {
            let glob_raw = conv_string(g)?;
            out.push(Pattern::new(&glob_raw)
                .or(Err(SkyliteProcError::DataError(format!("Not a valid glob: {}", glob_raw))))?);
        }
    }
    Ok(out)
}

impl AssetDirectories {
    fn from_scheme(alist: SCM) -> Result<AssetDirectories, SkyliteProcError> {
        unsafe {
            if scm_is_false(scm_list_p(alist)) {
                return Err(SkyliteProcError::DataError(format!("Asset directories must be defined as an associative list.")));
            }
            let mut out = Self::default();

            if let Some(expr) = assq_str("actors", alist)? {
                out.actors = parse_glob_list(expr)?;
            }
            if let Some(expr) = assq_str("scenes", alist)? {
                out.scenes = parse_glob_list(expr)?;
            }
            if let Some(expr) = assq_str("graphics", alist)? {
                out.graphics = parse_glob_list(expr)?;
            }
            if let Some(expr) = assq_str("sprites", alist)? {
                out.sprites = parse_glob_list(expr)?;
            }
            if let Some(expr) = assq_str("tilesets", alist)? {
                out.tilesets = parse_glob_list(expr)?;
            }
            if let Some(expr) = assq_str("maps", alist)? {
                out.maps = parse_glob_list(expr)?;
            }

            Ok(out)
        }
    }
}

impl Default for AssetDirectories {
    fn default() -> Self {
        Self {
            actors: vec![Pattern::new("./actors/*.scm").unwrap()],
            scenes: vec![Pattern::new("./scenes/*.scm").unwrap()],
            graphics: vec![Pattern::new("./graphics/*.scm").unwrap()],
            sprites: vec![Pattern::new("./sprites/*.scm").unwrap()],
            tilesets: vec![Pattern::new("./tilesets/*.scm").unwrap()],
            maps: vec![Pattern::new("./maps/*.scm").unwrap()]
        }
    }
}

#[derive(PartialEq, Debug)]
struct SaveItem {
    name: String,
    data: TypedValue
}

impl SaveItem {
    fn from_scheme(definition: SCM) -> Result<SaveItem, SkyliteProcError> {
        unsafe {
            Ok(SaveItem {
                name: conv_symbol(cxr(definition, &[CAR])?)?,
                data: parse_typed_value(
                    cxr(definition, &[CDR, CAR])?,
                    cxr(definition, &[CDR, CDR, CAR])?
                )?
            })
        }
    }
}

#[derive(PartialEq, Debug)]
struct SkyliteProject {
    name: String,
    assets: AssetDirectories,
    save_data: Vec<SaveItem>,
    tile_types: Vec<String>
}

impl SkyliteProject {
    fn from_scheme(definition: SCM) -> Result<SkyliteProject, SkyliteProcError> {
        unsafe {
            let name = conv_symbol(
                assq_str("name", definition)?.ok_or(SkyliteProcError::DataError("Missing required field 'name'".to_owned()))?
            )?;

            let assets = if let Some(alist) = assq_str("assets", definition)? {
                AssetDirectories::from_scheme(alist)?
            } else {
                AssetDirectories::default()
            };

            let mut save_data = Vec::new();
            if let Some(list) = assq_str("save-data", definition)? {
                for item in iter_list(list)? {
                    save_data.push(SaveItem::from_scheme(item)?)
                }
            }

            let mut tile_types = Vec::new();
            if let Some(list) = assq_str("tile-types", definition)? {
                for item in iter_list(list)? {
                    tile_types.push(conv_symbol(item)?)
                }
            }

            if tile_types.len() == 0 {
                return Err(SkyliteProcError::DataError("At least one tile-type must be defined.".to_owned()))
            }

            Ok(SkyliteProject {
                name,
                assets,
                save_data,
                tile_types
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use glob::Pattern;

    use crate::{parse_util::{eval_str, TypedValue}, project::{AssetDirectories, SaveItem}, with_guile};

    use super::SkyliteProject;

    extern "C" fn test_project_parsing_impl(_: &()) {
        unsafe {
            let definition = eval_str(
                r#"'((name TestProject)
                     (assets ((actors ("./test1/*.scm" "./test2/*.scm"))
                              (maps ("./test3/*.scm"))))
                     (save-data
                       ((flag1 bool #f)
                        (val2 u8 5)))
                     (tile-types (solid semi-solid non-solid)))"#).unwrap();
            let project = SkyliteProject::from_scheme(definition).unwrap();
            assert_eq!(project, SkyliteProject {
                name: "TestProject".to_owned(),
                assets: AssetDirectories {
                    actors: vec![Pattern::new("./test1/*.scm").unwrap(), Pattern::new("./test2/*.scm").unwrap()],
                    scenes: vec![Pattern::new("./scenes/*.scm").unwrap()],
                    graphics: vec![Pattern::new("./graphics/*.scm").unwrap()],
                    sprites: vec![Pattern::new("./sprites/*.scm").unwrap()],
                    tilesets: vec![Pattern::new("./tilesets/*.scm").unwrap()],
                    maps: vec![Pattern::new("./test3/*.scm").unwrap()]
                },
                save_data: vec![
                    SaveItem {
                        name: "flag1".to_owned(),
                        data: TypedValue::Bool(false)
                    },
                    SaveItem {
                        name: "val2".to_owned(),
                        data: TypedValue::U8(5)
                    }
                ],
                tile_types: vec!["solid".to_owned(), "semi-solid".to_owned(), "non-solid".to_owned()]
            });
        }
    }

    #[test]
    fn test_project_parsing() {
        with_guile(test_project_parsing_impl, ());
    }
}
