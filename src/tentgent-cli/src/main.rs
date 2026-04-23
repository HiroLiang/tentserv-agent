mod cli;

#[tokio::main]
async fn main() -> miette::Result<()> {
    cli::run().await
}
