use std::path::Path;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new("hd.toml");
    if path.exists() {
        return Err("hd.toml already exists in this directory".into());
    }

    let template = r#"[environment]
name = "myapp"
base = "ubuntu:22.04"

# [dependencies]
# apt = ["curl", "git"]
# node = "20.x"

# [build]
# steps = ["npm install"]
# cache = ["node_modules"]

# [services.web]
# command = "npm run dev"
# watch = ["src/**"]
# port = 3000

[files]
include = ["."]
exclude = [".git", "target", "node_modules/.cache"]

[options]
restart_grace = "5s"
"#;

    std::fs::write(path, template)?;
    println!("Created hd.toml");
    Ok(())
}
