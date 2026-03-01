#[tokio::main]
async fn main() -> anyhow::Result<()> {
    coast_cli::run().await
}
