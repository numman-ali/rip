mod checkpoints;
mod server;
mod session;

#[cfg(not(test))]
#[tokio::main]
async fn main() {
    server::serve(server::data_dir()).await;
}

#[cfg(test)]
mod server_tests;
