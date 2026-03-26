use hd_oci::{translate_dockerfile, ImageRef};

#[test]
fn dockerfile_to_spec_to_dag() {
    let dockerfile = r#"
FROM node:20-alpine
RUN npm install
CMD ["node", "index.js"]
"#;
    let spec = translate_dockerfile(dockerfile).unwrap();
    assert_eq!(spec.environment.base, "node:20-alpine");
    assert_eq!(spec.build.steps, vec!["npm install"]);
    assert_eq!(spec.services["app"].command, "node index.js");

    // Verify the spec can be serialized back to TOML
    let toml = spec.to_toml().unwrap();
    let reparsed = hd_spec::EnvSpec::from_toml(&toml).unwrap();
    assert_eq!(reparsed.environment.name, spec.environment.name);
}

#[test]
fn image_ref_to_registry_url() {
    let r = ImageRef::parse("nginx:latest").unwrap();
    assert_eq!(r.registry, "registry-1.docker.io");
    assert_eq!(r.repository, "library/nginx");
    assert_eq!(r.tag, "latest");
    assert_eq!(r.to_string(), "registry-1.docker.io/library/nginx:latest");
}
