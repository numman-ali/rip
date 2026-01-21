mod checkpoints;
mod provider_openresponses;
mod runner;
mod server;
mod session;
mod tasks;

pub use runner::{SessionEngine, SessionHandle};

#[cfg(not(test))]
pub async fn serve_default() {
    server::serve(server::data_dir()).await;
}

#[cfg(test)]
mod server_tests;
