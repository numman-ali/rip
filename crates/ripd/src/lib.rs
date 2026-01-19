mod checkpoints;
mod provider_openresponses;
mod server;
mod session;

#[cfg(not(test))]
pub async fn serve_default() {
    server::serve(server::data_dir()).await;
}

#[cfg(test)]
mod server_tests;
