// src/main.rs
use ouro_dag::run;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    run().await
}
