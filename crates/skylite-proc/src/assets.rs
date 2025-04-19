use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::fs::read_to_string;
use std::path::{Path, PathBuf, MAIN_SEPARATOR_STR};

use glob::{glob, Paths};

use crate::parse::guile::SCM;
use crate::parse::scheme_util::{assq_str, eval_str, iter_list, parse_string};
use crate::SkyliteProcError;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AssetSource {
    Path(PathBuf),
    BuiltIn(String),
}

impl AssetSource {
    pub(crate) fn load_with_guile(&self) -> Result<SCM, SkyliteProcError> {
        match self {
            AssetSource::Path(path) => {
                let definition_raw = read_to_string(path).map_err(|e| {
                    SkyliteProcError::OtherError(format!("Error reading asset file: {}", e))
                })?;
                unsafe { eval_str(&definition_raw) }
            }
            AssetSource::BuiltIn(definition_raw) => unsafe { eval_str(&definition_raw) },
        }
    }
}

impl Display for AssetSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetSource::Path(p) => p.fmt(f),
            AssetSource::BuiltIn(_) => write!(f, "<built-in>"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AssetType {
    Node,
    NodeList,
    Sequence,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AssetMetaData {
    pub atype: AssetType,
    pub id: usize,
    pub name: String,
    pub source: AssetSource,
}

fn normalize_glob(glob: &str, base_dir: &Path) -> String {
    if Path::new(&glob).is_relative() {
        base_dir.to_str().unwrap().to_owned() + MAIN_SEPARATOR_STR + &glob
    } else {
        glob.to_owned()
    }
}

fn load_metas_from_raw_globs(
    atype: AssetType,
    globs_raw: Vec<String>,
    base_dir: &Path,
) -> Result<HashMap<String, AssetMetaData>, SkyliteProcError> {
    let glob_iterators = globs_raw
        .iter()
        .map(|g| {
            let normalized = normalize_glob(g, base_dir);
            glob(&normalized).map_err(|err| data_err!("Error parsing glob: {err}"))
        })
        .collect::<Result<Vec<Paths>, SkyliteProcError>>()?;

    let meta_data_mappings = glob_iterators
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, path)| {
            let path =
                path.map_err(|err| SkyliteProcError::OtherError(format!("IO Error: {err}")))?;
            let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
            let meta = AssetMetaData {
                atype: atype.clone(),
                name: name.clone(),
                id: i,
                source: AssetSource::Path(path),
            };
            Ok((name, meta))
        });

    let mut out: HashMap<String, AssetMetaData> = HashMap::new();
    for res in meta_data_mappings {
        let (name, metadata) = res?;
        let entry = out.entry(name.clone());
        if let Entry::Occupied(e) = entry {
            return Err(data_err!(
                "Asset name {name} is ambiguous; both {:?} and {:?} match",
                metadata.source,
                e.get().source
            ));
        } else {
            entry.insert_entry(metadata);
        }
    }

    Ok(out)
}

fn extract_raw_globs(
    alist: Option<SCM>,
    key: &str,
    default: &str,
) -> Result<Vec<String>, SkyliteProcError> {
    unsafe {
        if let Some(expr) = alist.map(|v| assq_str(key, v)).transpose()?.flatten() {
            iter_list(expr)?
                .map(|s| parse_string(s))
                .collect::<Result<Vec<String>, SkyliteProcError>>()
        } else {
            Ok(vec![default.to_owned()])
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct Assets {
    pub nodes: HashMap<String, AssetMetaData>,
    pub node_lists: HashMap<String, AssetMetaData>,
    pub sequences: HashMap<String, AssetMetaData>,
}

fn add_builtin_nodes(nodes: &mut HashMap<String, AssetMetaData>) {
    let next_id = match nodes.values().map(|meta| meta.id).max() {
        Some(max) => max + 1,
        None => 0,
    };

    nodes.insert(
        "s-list".to_owned(),
        AssetMetaData {
            atype: AssetType::Node,
            id: next_id,
            name: "s-list".to_owned(),
            source: AssetSource::BuiltIn(include_str!("../built-ins/s-list.scm").to_owned()),
        },
    );
}

impl Assets {
    pub(crate) fn from_scheme_with_guile(
        alist: Option<SCM>,
        base_dir: &Path,
    ) -> Result<Assets, SkyliteProcError> {
        let mut out = Self::from_scheme_with_guile_without_builtins(alist, base_dir)?;

        add_builtin_nodes(&mut out.nodes);

        Ok(out)
    }

    fn from_scheme_with_guile_without_builtins(
        alist: Option<SCM>,
        base_dir: &Path,
    ) -> Result<Assets, SkyliteProcError> {
        let nodes_globs_raw = extract_raw_globs(alist, "nodes", "nodes/*.scm")?;
        let node_lists_globs_raw = extract_raw_globs(alist, "node-lists", "node-lists/*.scm")?;
        let sequences_globs_raw = extract_raw_globs(alist, "sequences", "sequences/*.scm")?;

        Ok(Assets {
            nodes: load_metas_from_raw_globs(AssetType::Node, nodes_globs_raw, base_dir)?,
            node_lists: load_metas_from_raw_globs(
                AssetType::NodeList,
                node_lists_globs_raw,
                base_dir,
            )?,
            sequences: load_metas_from_raw_globs(
                AssetType::Sequence,
                sequences_globs_raw,
                base_dir,
            )?,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;
    use std::fs::create_dir_all;
    use std::path::Path;

    use tempfile::{tempdir, TempDir};

    use crate::assets::{AssetMetaData, AssetSource, AssetType, Assets};
    use crate::parse::scheme_util::{eval_str, with_guile};

    pub(crate) fn create_tmp_fs(files: &[(&str, &str)]) -> Result<TempDir, std::io::Error> {
        let tmp = tempdir()?;
        for (name, content) in files {
            let file_path = tmp.path().join(name);
            if let Some(parent) = file_path.parent() {
                create_dir_all(parent).unwrap();
            }

            std::fs::write(file_path, content.as_bytes())?;
        }
        Ok(tmp)
    }

    #[test]
    fn test_from_scheme() {
        #[allow(improper_ctypes_definitions)]
        extern "C" fn test_from_scheme_impl(base_dir: &Path) -> Assets {
            let def = unsafe {
                eval_str(
                    r#"
                    '((nodes . ("test-nodes/*.scm"))
                      (node-lists . ("node-lists/*.scm")))"#,
                )
                .unwrap()
            };
            Assets::from_scheme_with_guile_without_builtins(Some(def), base_dir).unwrap()
        }

        let tmp_fs = create_tmp_fs(&[
            ("test-nodes/test-node-1.scm", ""),
            ("test-nodes/test-node-2.scm", ""),
            ("node-lists/list.scm", ""),
        ])
        .unwrap();

        let assets = with_guile(test_from_scheme_impl, tmp_fs.path());
        assert_eq!(
            assets,
            Assets {
                nodes: [
                    (
                        "test-node-1".to_owned(),
                        AssetMetaData {
                            atype: AssetType::Node,
                            id: 0,
                            name: "test-node-1".to_owned(),
                            source: AssetSource::Path(
                                tmp_fs.path().join("test-nodes/test-node-1.scm")
                            )
                        }
                    ),
                    (
                        "test-node-2".to_owned(),
                        AssetMetaData {
                            atype: AssetType::Node,
                            id: 1,
                            name: "test-node-2".to_owned(),
                            source: AssetSource::Path(
                                tmp_fs.path().join("test-nodes/test-node-2.scm")
                            )
                        }
                    )
                ]
                .into(),
                node_lists: [(
                    "list".to_owned(),
                    AssetMetaData {
                        atype: AssetType::NodeList,
                        id: 0,
                        name: "list".to_owned(),
                        source: AssetSource::Path(tmp_fs.path().join("node-lists/list.scm"))
                    }
                )]
                .into(),
                sequences: HashMap::new()
            }
        )
    }
}
