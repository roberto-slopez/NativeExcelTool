#[tokio::main]
async fn main() -> native_excel::Result<()> {
    native_excel::cli::run(std::env::args().skip(1)).await
}
