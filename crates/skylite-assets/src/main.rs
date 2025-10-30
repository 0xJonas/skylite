use crate::asset_server::connect_to_asset_server;

mod asset_server;
mod assets;
mod base_serde;
mod nodes;

pub use assets::*;
pub use nodes::*;

fn main() {
    connect_to_asset_server().unwrap();
}
