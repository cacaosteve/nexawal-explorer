pub mod model;
pub mod rpc;
pub mod view;
pub mod web;

pub mod api;
#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod fixture;
