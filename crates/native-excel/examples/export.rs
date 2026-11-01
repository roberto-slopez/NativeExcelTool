#[tokio::main]
async fn main() -> native_excel::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "examples/configs/objects.json".into());
    let request = native_excel::parse_request(&std::fs::read_to_string(path)?)?;
    let result = native_excel::ExcelExporter::export(request).await?;
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
    Ok(())
}
