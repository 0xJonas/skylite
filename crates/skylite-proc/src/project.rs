use crate::parse_util::CXROp::{CAR, CDR};
use crate::chibi_scheme::{sexp, sexp_listp, sexp_unbox_boolean};
use crate::chibi_util::ChibiContext;
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

fn parse_glob_list(ctx: &ChibiContext, list: sexp) -> Result<Vec<Pattern>, SkyliteProcError> {
    let mut out: Vec<Pattern> = Vec::new();
    unsafe {
        for g in iter_list(ctx, list)? {
            let glob_raw = conv_string(ctx, g)?;
            out.push(Pattern::new(&glob_raw)
                .or(Err(SkyliteProcError::DataError(format!("Not a valid glob: {}", glob_raw))))?);
        }
    }
    Ok(out)
}

impl AssetDirectories {
    fn from_scheme(ctx: &ChibiContext, alist: sexp) -> Result<AssetDirectories, SkyliteProcError> {
        unsafe {
            if !sexp_unbox_boolean(sexp_listp(ctx.c, alist)) {
                return Err(SkyliteProcError::DataError(format!("Asset directories must be defined as an associative list.")));
            }
            let mut out = Self::default();

            if let Some(expr) = assq_str(ctx, "actors", alist)? {
                out.actors = parse_glob_list(ctx, expr)?;
            }
            if let Some(expr) = assq_str(ctx, "scenes", alist)? {
                out.scenes = parse_glob_list(ctx, expr)?;
            }
            if let Some(expr) = assq_str(ctx, "graphics", alist)? {
                out.graphics = parse_glob_list(ctx, expr)?;
            }
            if let Some(expr) = assq_str(ctx, "sprites", alist)? {
                out.sprites = parse_glob_list(ctx, expr)?;
            }
            if let Some(expr) = assq_str(ctx, "tilesets", alist)? {
                out.tilesets = parse_glob_list(ctx, expr)?;
            }
            if let Some(expr) = assq_str(ctx, "maps", alist)? {
                out.maps = parse_glob_list(ctx, expr)?;
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
    fn from_scheme(ctx: &ChibiContext, definition: sexp) -> Result<SaveItem, SkyliteProcError> {
        unsafe {
            Ok(SaveItem {
                name: conv_symbol(ctx, cxr(ctx, definition, &[CAR])?)?,
                data: parse_typed_value(
                    ctx,
                    cxr(ctx, definition, &[CDR, CAR])?,
                    cxr(ctx, definition, &[CDR, CDR, CAR])?
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
    fn from_scheme(ctx: &ChibiContext, definition: sexp) -> Result<SkyliteProject, SkyliteProcError> {
        unsafe {
            let name = conv_symbol(
                ctx,
                assq_str(ctx, "name", definition)?.ok_or(SkyliteProcError::DataError("Missing required field 'name'".to_owned()))?
            )?;

            let assets = if let Some(alist) = assq_str(ctx, "assets", definition)? {
                AssetDirectories::from_scheme(ctx, alist)?
            } else {
                AssetDirectories::default()
            };

            let mut save_data = Vec::new();
            if let Some(list) = assq_str(ctx, "save-data", definition)? {
                for item in iter_list(ctx, list)? {
                    save_data.push(SaveItem::from_scheme(ctx, item)?)
                }
            }

            let mut tile_types = Vec::new();
            if let Some(list) = assq_str(ctx, "tile-types", definition)? {
                for item in iter_list(ctx, list)? {
                    tile_types.push(conv_symbol(ctx, item)?)
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

    use crate::{chibi_util::ChibiContext, parse_util::{eval_str, TypedValue}, project::{AssetDirectories, SaveItem}};

    use super::SkyliteProject;

    #[test]
    fn test_project_parsing() {
        unsafe {
            let ctx = ChibiContext::new().unwrap();

            let definition = ctx.make_var(eval_str(&ctx,
                r#"'((name TestProject)
                     (assets ((actors ("./test1/*.scm" "./test2/*.scm"))
                              (maps ("./test3/*.scm"))))
                     (save-data
                       ((flag1 bool #f)
                        (val2 u8 5)))
                     (tile-types (solid semi-solid non-solid)))"#).unwrap());
            let project = SkyliteProject::from_scheme(&ctx, *definition.get()).unwrap();
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
}
