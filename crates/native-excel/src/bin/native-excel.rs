#[tokio::main]
async fn main() {
    if let Err(error) = native_excel::cli::run(std::env::args().skip(1)).await {
        eprintln!("Export failed: {error}");
        std::process::exit(1);
    }
}
