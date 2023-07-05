#[macro_use]
extern crate lazy_static;

#[cfg(feature = "http_server")]
pub mod http_server;

pub mod config;
pub mod render;
pub mod renders;
pub mod rpi;
