#[cfg(not(test))]
#[tokio::main]
async fn main() {
    ripd::serve_default().await;
}
