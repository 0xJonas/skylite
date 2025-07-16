use syn::{parse_str, Attribute, Fields, ImplItem, Item, ItemStruct, Path};

pub(crate) mod encode;
pub(crate) mod node_lists;
pub(crate) mod nodes;
pub(crate) mod project;
pub(crate) mod sequences;
pub(crate) mod util;

macro_rules! define_annotations {
    ($($key:ident => $val:literal),*) => {
        $(
        pub(crate) const $key: &'static str = $val;
        )*

        static ANNOTATIONS: &[&'static str] = &[
            $($val),*
        ];
    };
}

define_annotations! {
    ANNOTATION_INIT => "skylite_proc::init",
    ANNOTATION_NEW => "skylite_proc::new",
    ANNOTATION_PROPERTY => "skylite_proc::property",
    ANNOTATION_NODE => "skylite_proc::node",
    ANNOTATION_NODES => "skylite_proc::nodes",
    ANNOTATION_PRE_UPDATE => "skylite_proc::pre_update",
    ANNOTATION_UPDATE => "skylite_proc::update",
    ANNOTATION_POST_UPDATE => "skylite_proc::post_update",
    ANNOTATION_PRE_RENDER => "skylite_proc::pre_render",
    ANNOTATION_RENDER => "skylite_proc::render",
    ANNOTATION_POST_RENDER => "skylite_proc::post_render",
    ANNOTATION_Z_ORDER => "skylite_proc::z_order",
    ANNOTATION_IS_VISIBLE => "skylite_proc::is_visible",
    ANNOTATION_CUSTOM_OP => "skylite_proc::custom_op",
    ANNOTATION_CUSTOM_CONDITION => "skylite_proc::custom_condition"
}

fn is_skylite_annotation(attr: &Attribute) -> bool {
    ANNOTATIONS
        .iter()
        .map(|a| -> Path { parse_str(a).unwrap() })
        .any(|p| &p == attr.path())
}

/// Removes skylite annotations from all places where they can currently appear.
pub(crate) fn remove_annotations_from_items(items: &mut [Item]) {
    for item in items {
        match item {
            // Remove annotations from function items.
            Item::Fn(i) => i.attrs.retain(|attr| !is_skylite_annotation(attr)),

            // Remove annotations from struct fields.
            Item::Struct(ItemStruct {
                fields: Fields::Named(fields),
                ..
            }) => {
                for f in fields.named.iter_mut() {
                    f.attrs.retain(|attr| !is_skylite_annotation(attr));
                }
            }

            // Remove annotations from functions inside impl blocks.
            Item::Impl(i) => {
                for impl_item in i.items.iter_mut() {
                    if let ImplItem::Fn(i) = impl_item {
                        i.attrs.retain(|attr| !is_skylite_annotation(attr))
                    }
                }
            }
            _ => {}
        }
    }
}
