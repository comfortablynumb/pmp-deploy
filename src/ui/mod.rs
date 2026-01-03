mod api;
mod server;
mod state;

pub use server::{generate_dev_certificate, start_server, start_server_https, ServerConfig};
pub use state::AppState;
