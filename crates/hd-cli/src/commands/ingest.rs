pub fn run(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let spec = hd_oci::translate_dockerfile(&content)?;
    let toml = spec.to_toml()?;

    let out_path = "hd.toml";
    if std::path::Path::new(out_path).exists() {
        return Err("hd.toml already exists. Remove it first or use a different name.".into());
    }
    std::fs::write(out_path, &toml)?;
    println!("Translated {} -> hd.toml", path);
    println!("Review and customize the generated hd.toml");
    Ok(())
}
