use hd_sandbox::{Sandbox, ServiceDef, RestartPolicy};

#[test]
fn full_sandbox_lifecycle() {
    let services = vec![
        ServiceDef {
            name: "backend".into(),
            command: "sleep".into(),
            args: vec!["10".into()],
            watch_patterns: vec!["src/**/*.rs".into()],
            depends_on: vec![],
            restart_policy: RestartPolicy::Always,
        },
        ServiceDef {
            name: "frontend".into(),
            command: "sleep".into(),
            args: vec!["10".into()],
            watch_patterns: vec!["web/**/*.ts".into()],
            depends_on: vec!["backend".into()],
            restart_policy: RestartPolicy::Always,
        },
    ];

    let mut sandbox = Sandbox::new(services);

    // Start
    sandbox.start_all().unwrap();
    assert_eq!(sandbox.running_count(), 2);

    // Selective restart: only backend should restart
    let restarted = sandbox.restart_for_changes(&["src/main.rs".into()]).unwrap();
    assert_eq!(restarted, vec!["backend"]);

    // Frontend change: only frontend restarts
    let restarted = sandbox.restart_for_changes(&["web/app.ts".into()]).unwrap();
    assert_eq!(restarted, vec!["frontend"]);

    // Stop
    sandbox.stop_all().unwrap();
    assert_eq!(sandbox.running_count(), 0);
}
