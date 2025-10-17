use crate::asset_server::connect_to_asset_server;

mod asset_server;
mod error;

fn main() {
    connect_to_asset_server().unwrap();
}
