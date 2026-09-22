use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::work::executor::request::{WorkExecutionRequest, WorkExecutionStatus};
use crate::work::executor::{ExecutionFailureKind, WorkExecutor};
use crate::work::models::{
    ExecutionNetworkPolicy, WorkAccessRoot, WorkExecutionManifest, WorkExecutionRuntime,
    WorkResourceKind, WorkResourceManifest, WorkWorkspace,
};
use crate::work::paths::WorkPaths;
use crate::work::sandbox::fake::FakeSandboxProvider;
use crate::work::sandbox::macos::MacosSeatbeltProvider;
use crate::work::sandbox::policy::{
    SandboxEnforcement, SandboxMode, SandboxPolicyResolver, SandboxRequirement,
};
use crate::work::sandbox::{
    create_default_sandbox_provider, ExecutionCommand, NativeSandboxProvider,
    SandboxDenialClassifier, SandboxError, UnavailableSandboxProvider,
};

fn create_test_manifest(readable: Vec<&str>, writable: Vec<&str>) -> WorkExecutionManifest {
    WorkExecutionManifest {
        trusted: true,
        action_risk_classes: Default::default(),
        runtime: WorkExecutionRuntime::Python,
        entry: "main.py".to_string(),
        actions: vec!["run".to_string()],
        network: ExecutionNetworkPolicy::None,
        timeout_seconds: 30,
        readable_areas: readable.into_iter().map(|s| s.to_string()).collect(),
        writable_areas: writable.into_iter().map(|s| s.to_string()).collect(),
    }
}

fn write_test_workspace_manifest(paths: &WorkPaths, workspace_id: &str) {
    let workspace_dir = paths.workspace_dir(workspace_id).unwrap();
    let now = "2026-08-28T00:00:00Z".to_string();
    let workspace = WorkWorkspace {
        id: workspace_id.to_string(),
        name: workspace_id.to_string(),
        root: workspace_dir.to_string_lossy().into_owned(),
        input_dir: workspace_dir.join("input").to_string_lossy().into_owned(),
        scratch_dir: workspace_dir.join("scratch").to_string_lossy().into_owned(),
        output_dir: workspace_dir.join("output").to_string_lossy().into_owned(),
        context_dir: workspace_dir.join("context").to_string_lossy().into_owned(),
        created_at: now.clone(),
        updated_at: now,
        artifact_count: 0,
        archived: false,
        access_roots: Vec::new(),
        default_policy: Default::default(),
        default_model: None,
        default_effort: None,
        root_kind: Default::default(),
        primary_work_root: None,
        working_root_valid: None,
        artifact_storage_mode: Default::default(),
    };
    fs::write(
        paths.manifest_path(workspace_id).unwrap(),
        serde_json::to_vec_pretty(&workspace).unwrap(),
    )
    .unwrap();
}

#[test]
fn test_sandbox_policy_resolver_readonly_exact_areas() {
    let temp_dir = TempDir::new().unwrap();
    let ws_path = temp_dir.path();
    let input_dir = ws_path.join("input");
    let context_dir = ws_path.join("context");
    let scratch_dir = ws_path.join("scratch");
    let output_dir = ws_path.join("output");

    fs::create_dir_all(&input_dir).unwrap();
    fs::create_dir_all(&context_dir).unwrap();
    fs::create_dir_all(&scratch_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    // Manifest strictly declaring only input area as readable
    let manifest = create_test_manifest(vec!["input"], vec![]);
    let policy = SandboxPolicyResolver::resolve(
        ws_path,
        None,
        &manifest,
        &[],
        &[],
        "run-123",
        "exec-123",
        temp_dir.path(),
    )
    .unwrap();

    assert_eq!(policy.mode, SandboxMode::ReadOnly);
    assert_eq!(policy.requirement, SandboxRequirement::FullRequired);
    assert!(policy.write_roots.is_empty());
    assert!(
        policy.temp_root.is_none(),
        "ReadOnly mode must not provide a writable temp_root"
    );

    // Only input is in read_roots; context, scratch, output MUST NOT be present
    assert!(policy.read_roots.iter().any(|p| p.ends_with("input")));
    assert!(!policy.read_roots.iter().any(|p| p.ends_with("context")));
    assert!(!policy.read_roots.iter().any(|p| p.ends_with("scratch")));
    assert!(!policy.read_roots.iter().any(|p| p.ends_with("output")));
}

#[test]
fn test_sandbox_policy_resolver_workspace_write_exact_areas() {
    let temp_dir = TempDir::new().unwrap();
    let ws_path = temp_dir.path();
    let input_dir = ws_path.join("input");
    let context_dir = ws_path.join("context");
    let scratch_dir = ws_path.join("scratch");
    let output_dir = ws_path.join("output");

    fs::create_dir_all(&input_dir).unwrap();
    fs::create_dir_all(&context_dir).unwrap();
    fs::create_dir_all(&scratch_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    // Manifest strictly declaring input (readable) and output (writable)
    let manifest = create_test_manifest(vec!["input"], vec!["output"]);
    let extra_root = WorkAccessRoot {
        path: temp_dir
            .path()
            .join("extra_folder")
            .to_string_lossy()
            .to_string(),
        writable: true,
    };
    fs::create_dir_all(&extra_root.path).unwrap();

    let policy = SandboxPolicyResolver::resolve(
        ws_path,
        None,
        &manifest,
        &[extra_root],
        &[],
        "run-123",
        "exec-123",
        temp_dir.path(),
    )
    .unwrap();

    assert_eq!(policy.mode, SandboxMode::WorkspaceWrite);
    assert_eq!(policy.requirement, SandboxRequirement::FullRequired);
    assert!(
        policy.temp_root.is_some(),
        "WorkspaceWrite mode must have private temp_root"
    );
    assert!(
        !policy.read_roots.iter().any(|path| path.ends_with(".home")),
        "WorkspaceWrite mode must not create a fake HOME"
    );
    assert!(
        !policy
            .write_roots
            .iter()
            .any(|path| path.ends_with(".home")),
        "WorkspaceWrite mode must not make a fake HOME writable"
    );

    // Output and extra_folder are writable; scratch and input are NOT in write_roots
    assert!(policy.write_roots.iter().any(|p| p.ends_with("output")));
    assert!(policy
        .write_roots
        .iter()
        .any(|p| p.ends_with("extra_folder")));
    assert!(!policy.write_roots.iter().any(|p| p.ends_with("input")));
    assert!(!policy.write_roots.iter().any(|p| p.ends_with("context")));

    // Read roots include input, output (writable area), and extra_folder
    assert!(policy.read_roots.iter().any(|p| p.ends_with("input")));
    assert!(policy.read_roots.iter().any(|p| p.ends_with("output")));
    assert!(policy
        .read_roots
        .iter()
        .any(|p| p.ends_with("extra_folder")));
    assert!(!policy.read_roots.iter().any(|p| p.ends_with("context")));
}

#[test]
fn test_sandbox_policy_resolver_rejects_unknown_areas() {
    let temp_dir = TempDir::new().unwrap();
    let ws_path = temp_dir.path();

    let manifest = create_test_manifest(vec!["/etc/passwd"], vec!["output"]);
    let res = SandboxPolicyResolver::resolve(
        ws_path,
        None,
        &manifest,
        &[],
        &[],
        "run-123",
        "exec-123",
        temp_dir.path(),
    );
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Unknown workspace area"));
}

#[test]
fn test_unavailable_provider_fails_closed() {
    let unavail = UnavailableSandboxProvider::new("Simulated platform failure");
    assert_eq!(unavail.name(), "unavailable");
    assert!(matches!(unavail.probe(), Err(SandboxError::Unavailable(_))));

    let cmd = ExecutionCommand {
        program: PathBuf::from("/bin/echo"),
        args: vec![OsString::from("test")],
        current_dir: PathBuf::from("/tmp"),
        envs: vec![],
    };

    let manifest = create_test_manifest(vec!["input"], vec![]);
    let temp_dir = TempDir::new().unwrap();
    let policy = SandboxPolicyResolver::resolve(
        temp_dir.path(),
        None,
        &manifest,
        &[],
        &[],
        "run-1",
        "exec-1",
        temp_dir.path(),
    )
    .unwrap();

    let res = unavail.confine(&cmd, &policy);
    assert!(matches!(res, Err(SandboxError::Unavailable(_))));
}

#[test]
fn test_fake_sandbox_provider_fail_closed() {
    let fake_unavail = FakeSandboxProvider::new_unavailable();
    assert!(fake_unavail.probe().is_err());

    let cmd = ExecutionCommand {
        program: PathBuf::from("/bin/echo"),
        args: vec![OsString::from("test")],
        current_dir: PathBuf::from("/tmp"),
        envs: vec![],
    };

    let manifest = create_test_manifest(vec!["input"], vec![]);
    let temp_dir = TempDir::new().unwrap();
    let policy = SandboxPolicyResolver::resolve(
        temp_dir.path(),
        None,
        &manifest,
        &[],
        &[],
        "run-1",
        "exec-1",
        temp_dir.path(),
    )
    .unwrap();

    // Unavailable provider must fail confinement
    let res = fake_unavail.confine(&cmd, &policy);
    assert!(matches!(res, Err(SandboxError::Unavailable(_))));

    // Partial provider must fail when FullRequired
    let fake_partial = FakeSandboxProvider::new_partial();
    let res_partial = fake_partial.confine(&cmd, &policy);
    assert!(matches!(
        res_partial,
        Err(SandboxError::EnforcementInsufficient { .. })
    ));

    // Full provider succeeds
    let fake_full = FakeSandboxProvider::new_full();
    let res_full = fake_full.confine(&cmd, &policy).unwrap();
    assert_eq!(res_full.enforcement, SandboxEnforcement::Full);
}

#[test]
fn test_production_default_sandbox_provider_contract() {
    let provider = create_default_sandbox_provider();

    #[cfg(target_os = "macos")]
    {
        assert_eq!(provider.name(), "macos-seatbelt");
    }

    #[cfg(not(target_os = "macos"))]
    {
        assert_eq!(provider.name(), "unavailable");
        assert!(
            provider.probe().is_err(),
            "Non-macOS default provider must fail closed"
        );
    }
}

#[test]
fn test_sandbox_denial_classifier_rules() {
    let denial_signatures = vec![
        "operation not permitted".to_string(),
        "sandbox: denied".to_string(),
    ];
    let runner_failure_signatures = vec!["sandbox-exec:".to_string(), "sandbox_apply:".to_string()];

    // 1. Runner failure rule match
    let kind1 = SandboxDenialClassifier::classify_failure(
        "sandbox-exec: sandbox_apply: failed to compile profile",
        Some(1),
        &denial_signatures,
        &runner_failure_signatures,
    );
    assert_eq!(kind1, ExecutionFailureKind::SandboxInfrastructureFailure);

    // 2. Specific denial signature match
    let kind2 = SandboxDenialClassifier::classify_failure(
        "open(workspace/output/hack.txt): Operation not permitted",
        Some(1),
        &denial_signatures,
        &runner_failure_signatures,
    );
    assert_eq!(kind2, ExecutionFailureKind::SandboxDenied);

    // 3. Normal capability runtime error
    let kind3 = SandboxDenialClassifier::classify_failure(
        "ValueError: invalid literal for int() with base 10: 'abc'",
        Some(1),
        &denial_signatures,
        &runner_failure_signatures,
    );
    assert_eq!(kind3, ExecutionFailureKind::CapabilityFailure);
}

#[test]
fn test_macos_seatbelt_profile_generation() {
    let temp_dir = TempDir::new().unwrap();
    let ws_path = temp_dir.path();
    let input_dir = ws_path.join("input");
    let output_dir = ws_path.join("output");
    fs::create_dir_all(&input_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    let manifest = create_test_manifest(vec!["input"], vec!["output"]);
    let policy = SandboxPolicyResolver::resolve(
        ws_path,
        None,
        &manifest,
        &[],
        &[],
        "run-test",
        "exec-test",
        temp_dir.path(),
    )
    .unwrap();

    let cmd = ExecutionCommand {
        program: PathBuf::from("/usr/bin/python3"),
        args: vec![OsString::from("main.py")],
        current_dir: ws_path.to_path_buf(),
        envs: vec![],
    };

    let profile = MacosSeatbeltProvider::generate_profile(&cmd, &policy, None);
    assert!(profile.contains("(deny default)"));
    assert!(profile.contains("(allow process-exec (literal \"/usr/bin/python3\"))"));
    assert!(profile.contains("(allow file-write*"));
    // macOS 15+ baseline: unqualified read is required for dyld cold start;
    // credentials must still be explicitly denied.
    assert!(profile.contains("(allow file-read* (subpath \"/\"))"));
    assert!(profile.contains("(deny file-read* (regex #\"^/Users/[^/]+/\\.ssh(/|$)\"))"));
    assert!(profile.contains("Library/Keychains"));
    assert!(profile.contains(
        &input_dir
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string()
    ));
    assert!(profile.contains(
        &output_dir
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string()
    ));
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn test_adversarial_macos_seatbelt_confinement() {
    let provider = MacosSeatbeltProvider::new();
    if provider.probe().is_err() {
        eprintln!("Skipping live macOS seatbelt test: sandbox-exec unavailable");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let ws_path = temp_dir.path();
    let input_dir = ws_path.join("input");
    let output_dir = ws_path.join("output");
    let scratch_dir = ws_path.join("scratch");
    let context_dir = ws_path.join("context");

    fs::create_dir_all(&input_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();
    fs::create_dir_all(&scratch_dir).unwrap();
    fs::create_dir_all(&context_dir).unwrap();

    let secret_input_file = input_dir.join("data.txt");
    fs::write(&secret_input_file, "hello from input").unwrap();

    // /usr/bin/python3 may be an xcode-select stub that works on the host but fails
    // inside the sandbox (xcrun needs tmp writes and Xcode.app). Probe candidates
    // under a minimal real sandbox profile and keep the first one that survives.
    let usable_in_sandbox = |p: &PathBuf| {
        if !p.exists() {
            return false;
        }
        let mini = format!(
            "(version 1)(deny default)(allow process-fork)(allow sysctl-read)(allow file-read-metadata)(allow file-read* (subpath \"/\"))(allow process-exec (literal \"{}\"))(allow process-exec (subpath \"/usr/bin\") (subpath \"/usr/local/bin\") (subpath \"/opt/homebrew\") (subpath \"/bin\") (subpath \"/Library/Frameworks\"))",
            p.to_string_lossy()
        );
        std::process::Command::new("/usr/bin/sandbox-exec")
            .arg("-p")
            .arg(&mini)
            .arg(p)
            .arg("-c")
            .arg("pass")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    let py_bin = [
        "/opt/homebrew/bin/python3",
        "/usr/local/bin/python3",
        "/usr/bin/python3",
    ]
    .iter()
    .map(PathBuf::from)
    .find(usable_in_sandbox);
    let Some(py_bin) = py_bin else {
        eprintln!("Skipping live test: no sandbox-functional python3 found");
        return;
    };

    // 1. Legal Read from input in ReadOnly mode
    {
        let manifest = create_test_manifest(vec!["input"], vec![]);
        let policy = SandboxPolicyResolver::resolve(
            ws_path,
            None,
            &manifest,
            &[],
            &[],
            "run-read",
            "exec-read",
            temp_dir.path(),
        )
        .unwrap();

        let script = format!(
            "with open('{}', 'r') as f:\n    print(f.read().strip())",
            secret_input_file.to_string_lossy()
        );

        let cmd = ExecutionCommand {
            program: py_bin.clone(),
            args: vec![OsString::from("-c"), OsString::from(script)],
            current_dir: ws_path.to_path_buf(),
            envs: vec![],
        };

        let confined = provider.confine(&cmd, &policy).unwrap();
        let output = std::process::Command::new(&confined.program)
            .args(&confined.args)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "Legal read failed: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout_str.trim(), "hello from input");
    }

    // 2. Adversarial: Attempt write to output in ReadOnly mode -> must be denied by Seatbelt
    {
        let manifest = create_test_manifest(vec!["input"], vec![]);
        let policy = SandboxPolicyResolver::resolve(
            ws_path,
            None,
            &manifest,
            &[],
            &[],
            "run-hack-write",
            "exec-hack-write",
            temp_dir.path(),
        )
        .unwrap();

        let hack_target = output_dir.join("hack.txt");
        let script = format!(
            "with open('{}', 'w') as f:\n    f.write('pwned')",
            hack_target.to_string_lossy()
        );

        let cmd = ExecutionCommand {
            program: py_bin.clone(),
            args: vec![OsString::from("-c"), OsString::from(script)],
            current_dir: ws_path.to_path_buf(),
            envs: vec![],
        };

        let confined = provider.confine(&cmd, &policy).unwrap();
        let output = std::process::Command::new(&confined.program)
            .args(&confined.args)
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "Adversarial write should have been denied"
        );
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        let failure_kind = SandboxDenialClassifier::classify_failure(
            &stderr_str,
            output.status.code(),
            &confined.denial_signatures,
            &confined.runner_failure_signatures,
        );
        assert_eq!(failure_kind, ExecutionFailureKind::SandboxDenied);
        assert!(!hack_target.exists(), "File must not be created");
    }

    // 3. Adversarial: Attempt write to input in WorkspaceWrite mode -> input is read-only, must be denied
    {
        let manifest = create_test_manifest(vec!["input"], vec!["output"]);
        let policy = SandboxPolicyResolver::resolve(
            ws_path,
            None,
            &manifest,
            &[],
            &[],
            "run-hack-input",
            "exec-hack-input",
            temp_dir.path(),
        )
        .unwrap();

        let tamper_target = input_dir.join("tampered.txt");
        let script = format!(
            "with open('{}', 'w') as f:\n    f.write('tampered')",
            tamper_target.to_string_lossy()
        );

        let cmd = ExecutionCommand {
            program: py_bin.clone(),
            args: vec![OsString::from("-c"), OsString::from(script)],
            current_dir: ws_path.to_path_buf(),
            envs: vec![],
        };

        let confined = provider.confine(&cmd, &policy).unwrap();
        let output = std::process::Command::new(&confined.program)
            .args(&confined.args)
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "Adversarial write to input directory should be denied"
        );
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        let failure_kind = SandboxDenialClassifier::classify_failure(
            &stderr_str,
            output.status.code(),
            &confined.denial_signatures,
            &confined.runner_failure_signatures,
        );
        assert_eq!(failure_kind, ExecutionFailureKind::SandboxDenied);
        assert!(!tamper_target.exists(), "Tampered file must not be created");
    }

    // 4. Legal Write to output in WorkspaceWrite mode -> must succeed
    {
        let manifest = create_test_manifest(vec!["input"], vec!["output"]);
        let policy = SandboxPolicyResolver::resolve(
            ws_path,
            None,
            &manifest,
            &[],
            &[],
            "run-legal-write",
            "exec-legal-write",
            temp_dir.path(),
        )
        .unwrap();

        let legal_output = output_dir.join("result.json");
        let script = format!(
            "with open('{}', 'w') as f:\n    f.write('{{\"status\": \"ok\"}}')",
            legal_output.to_string_lossy()
        );

        let cmd = ExecutionCommand {
            program: py_bin.clone(),
            args: vec![OsString::from("-c"), OsString::from(script)],
            current_dir: ws_path.to_path_buf(),
            envs: vec![],
        };

        let confined = provider.confine(&cmd, &policy).unwrap();
        let output = std::process::Command::new(&confined.program)
            .args(&confined.args)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "Legal write failed: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            legal_output.exists(),
            "Result file should have been written"
        );
        let content = fs::read_to_string(&legal_output).unwrap();
        assert_eq!(content, "{\"status\": \"ok\"}");
    }

    // 5. Adversarial: Attempt unauthorized read of user home SSH directory -> must be denied
    {
        let manifest = create_test_manifest(vec!["input"], vec!["output"]);
        let policy = SandboxPolicyResolver::resolve(
            ws_path,
            None,
            &manifest,
            &[],
            &[],
            "run-hack-ssh",
            "exec-hack-ssh",
            temp_dir.path(),
        )
        .unwrap();

        let script = "import os; open(os.path.expanduser('~/.ssh/id_rsa'), 'r').read()";

        let cmd = ExecutionCommand {
            program: py_bin.clone(),
            args: vec![OsString::from("-c"), OsString::from(script)],
            current_dir: ws_path.to_path_buf(),
            envs: vec![],
        };

        let confined = provider.confine(&cmd, &policy).unwrap();
        let output = std::process::Command::new(&confined.program)
            .args(&confined.args)
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "Unauthorized home read should be denied"
        );
    }

    // 6. Adversarial: Deterministic loopback network connection -> must be denied by Seatbelt (deny default)
    {
        // 6a. Bind a real local TCP listener on 127.0.0.1:0
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        // 6b. Verify unsandboxed connection succeeds
        assert!(
            std::net::TcpStream::connect(format!("127.0.0.1:{port}")).is_ok(),
            "Unsandboxed connection to local listener must succeed"
        );

        // 6c. Sandboxed script attempting connection to the exact same local port
        let manifest = create_test_manifest(vec!["input"], vec!["output"]);
        let policy = SandboxPolicyResolver::resolve(
            ws_path,
            None,
            &manifest,
            &[],
            &[],
            "run-hack-net",
            "exec-hack-net",
            temp_dir.path(),
        )
        .unwrap();

        let script = format!(
            "import socket\ns = socket.socket()\ns.settimeout(1.0)\ns.connect(('127.0.0.1', {port}))"
        );

        let cmd = ExecutionCommand {
            program: py_bin.clone(),
            args: vec![OsString::from("-c"), OsString::from(script)],
            current_dir: ws_path.to_path_buf(),
            envs: vec![],
        };

        let confined = provider.confine(&cmd, &policy).unwrap();
        let output = std::process::Command::new(&confined.program)
            .args(&confined.args)
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "Sandboxed loopback network socket connection must be denied by Seatbelt"
        );
    }
}

#[tokio::test]
async fn test_process_tree_background_helper_termination() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let workspace_id = "test-ws-proc-tree".to_string();
    let ws_dir = paths.workspace_dir(&workspace_id).unwrap();
    fs::create_dir_all(ws_dir.join("input")).unwrap();
    fs::create_dir_all(ws_dir.join("scratch")).unwrap();
    fs::create_dir_all(ws_dir.join("output")).unwrap();
    fs::create_dir_all(ws_dir.join("context")).unwrap();
    write_test_workspace_manifest(&paths, &workspace_id);

    let res_dir = paths.data_root().join("res_proc_tree");
    fs::create_dir_all(&res_dir).unwrap();

    // Script forks a background child (stays in same pgid as parent) that sleeps 0.5s then writes output/late.txt.
    // Parent exits immediately. If process group kill works, the child must be killed and late.txt must NOT appear.
    let script = r#"
import os, time, sys
child_pid = os.fork()
if child_pid == 0:
    # child - inherit parent pgid, sleep then try to write
    time.sleep(0.5)
    with open('output/late.txt', 'w') as f:
        f.write('late')
    os._exit(0)
else:
    # parent exits immediately
    sys.exit(0)
"#;
    fs::write(res_dir.join("script.py"), script).unwrap();

    let manifest = WorkResourceManifest {
        id: "work-test-proc-tree".to_string(),
        name: "Test Process Tree Capability".to_string(),
        description: "Process tree fixture".to_string(),
        kind: WorkResourceKind::Capability,
        origin: crate::work::models::ResourceOrigin::Builtin,
        modes: vec![],
        runtimes: vec![],
        entry: "capabilities/tree".to_string(),
        permissions: vec!["local-exec".to_string()],
        enabled: true,
        discovery: Default::default(),
        execution: Some(WorkExecutionManifest {
            trusted: true,
            action_risk_classes: Default::default(),
            runtime: WorkExecutionRuntime::Python,
            entry: "script.py".to_string(),
            actions: vec!["run".to_string()],
            network: ExecutionNetworkPolicy::None,
            timeout_seconds: 10,
            readable_areas: vec!["input".to_string()],
            writable_areas: vec!["scratch".to_string(), "output".to_string()],
        }),
    };

    let req = WorkExecutionRequest {
        workspace_id: workspace_id.clone(),
        resource_id: manifest.id.clone(),
        action: "run".to_string(),
        arguments: serde_json::json!({}),
        input_paths: vec![],
        expected_outputs: vec![],
        task_id: Some("task-tree-1".to_string()),
        work_run_id: Some("run-tree-1".to_string()),
    };

    let executor =
        WorkExecutor::new_restricted_host_with_sandbox(Box::new(FakeSandboxProvider::new_full()));
    let result = executor
        .execute(&paths, &manifest, &res_dir, &req, true)
        .await
        .unwrap();

    assert_eq!(result.status, WorkExecutionStatus::Success);

    // Sleep long enough (>0.5s) to verify whether background child process was killed
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // Verify late.txt was NEVER created because process group was reaped upon tool completion
    let late_file = ws_dir.join("output/late.txt");
    assert!(
        !late_file.exists(),
        "Background helper process must be killed with process group and not produce late side effects"
    );
}

#[tokio::test]
async fn test_per_execution_temp_isolation_and_automatic_cleanup() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let workspace_id = "test-ws-temp".to_string();
    let ws_dir = paths.workspace_dir(&workspace_id).unwrap();
    fs::create_dir_all(ws_dir.join("input")).unwrap();
    fs::create_dir_all(ws_dir.join("scratch")).unwrap();
    fs::create_dir_all(ws_dir.join("output")).unwrap();
    fs::create_dir_all(ws_dir.join("context")).unwrap();
    write_test_workspace_manifest(&paths, &workspace_id);

    let res_dir = paths.data_root().join("res_temp");
    fs::create_dir_all(&res_dir).unwrap();

    // Script writes a file into TMPDIR and output/done.txt
    let script = r#"
import os
tmp_dir = os.environ.get("TMPDIR", "/tmp")
with open(os.path.join(tmp_dir, "temp_data.txt"), "w") as f:
    f.write("temporary buffer")
with open("output/done.txt", "w") as f:
    f.write("ok")
"#;
    fs::write(res_dir.join("script.py"), script).unwrap();

    let manifest = WorkResourceManifest {
        id: "work-test-temp".to_string(),
        name: "Test Temp Capability".to_string(),
        description: "Temp capability fixture".to_string(),
        kind: WorkResourceKind::Capability,
        origin: crate::work::models::ResourceOrigin::Builtin,
        modes: vec![],
        runtimes: vec![],
        entry: "capabilities/temp".to_string(),
        permissions: vec!["local-exec".to_string()],
        enabled: true,
        discovery: Default::default(),
        execution: Some(WorkExecutionManifest {
            trusted: true,
            action_risk_classes: Default::default(),
            runtime: WorkExecutionRuntime::Python,
            entry: "script.py".to_string(),
            actions: vec!["run".to_string()],
            network: ExecutionNetworkPolicy::None,
            timeout_seconds: 10,
            readable_areas: vec!["input".to_string()],
            writable_areas: vec!["scratch".to_string(), "output".to_string()],
        }),
    };

    let req = WorkExecutionRequest {
        workspace_id: workspace_id.clone(),
        resource_id: manifest.id.clone(),
        action: "run".to_string(),
        arguments: serde_json::json!({}),
        input_paths: vec![],
        expected_outputs: vec!["output/done.txt".to_string()],
        task_id: Some("task-temp-1".to_string()),
        work_run_id: Some("run-temp-1".to_string()),
    };

    let executor =
        WorkExecutor::new_restricted_host_with_sandbox(Box::new(FakeSandboxProvider::new_full()));
    let result = executor
        .execute(&paths, &manifest, &res_dir, &req, true)
        .await
        .unwrap();

    assert_eq!(
        result.status,
        WorkExecutionStatus::Success,
        "Execution failed with stderr: {}",
        result.stderr
    );

    // Verify output exists
    assert!(ws_dir.join("output/done.txt").exists());

    // Verify per-execution temp dir was automatically cleaned up after execution!
    let per_exec_temp_dir = paths
        .data_root()
        .join("sandbox-tmp")
        .join("run-temp-1")
        .join(&result.execution_id);
    assert!(
        !per_exec_temp_dir.exists(),
        "Per-execution sandbox temporary directory must be cleaned up automatically after run"
    );
}

#[tokio::test]
async fn test_large_stdout_does_not_deadlock() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let workspace_id = "test-ws-large-stdout".to_string();
    let ws_dir = paths.workspace_dir(&workspace_id).unwrap();
    fs::create_dir_all(ws_dir.join("input")).unwrap();
    fs::create_dir_all(ws_dir.join("scratch")).unwrap();
    fs::create_dir_all(ws_dir.join("output")).unwrap();
    fs::create_dir_all(ws_dir.join("context")).unwrap();
    write_test_workspace_manifest(&paths, &workspace_id);

    let res_dir = paths.data_root().join("res_large_stdout");
    fs::create_dir_all(&res_dir).unwrap();

    // Writes 500,000 bytes (far larger than OS pipe buffer ~64KB) to stdout and exits
    let script = r#"
import sys
payload = "x" * 500_000
sys.stdout.write(payload)
sys.stdout.flush()
sys.exit(0)
"#;
    fs::write(res_dir.join("script.py"), script).unwrap();

    let manifest = WorkResourceManifest {
        id: "work-test-large-stdout".to_string(),
        name: "Test Large Stdout Capability".to_string(),
        description: "Pipe deadlock regression fixture".to_string(),
        kind: WorkResourceKind::Capability,
        origin: crate::work::models::ResourceOrigin::Builtin,
        modes: vec![],
        runtimes: vec![],
        entry: "capabilities/pipe".to_string(),
        permissions: vec!["local-exec".to_string()],
        enabled: true,
        discovery: Default::default(),
        execution: Some(WorkExecutionManifest {
            trusted: true,
            action_risk_classes: Default::default(),
            runtime: WorkExecutionRuntime::Python,
            entry: "script.py".to_string(),
            actions: vec!["run".to_string()],
            network: ExecutionNetworkPolicy::None,
            timeout_seconds: 5,
            readable_areas: vec!["input".to_string()],
            writable_areas: vec!["scratch".to_string(), "output".to_string()],
        }),
    };

    let req = WorkExecutionRequest {
        workspace_id: workspace_id.clone(),
        resource_id: manifest.id.clone(),
        action: "run".to_string(),
        arguments: serde_json::json!({}),
        input_paths: vec![],
        expected_outputs: vec![],
        task_id: Some("task-pipe-1".to_string()),
        work_run_id: Some("run-pipe-1".to_string()),
    };

    let executor =
        WorkExecutor::new_restricted_host_with_sandbox(Box::new(FakeSandboxProvider::new_full()));
    let result = executor
        .execute(&paths, &manifest, &res_dir, &req, true)
        .await
        .unwrap();

    assert_eq!(
        result.status,
        WorkExecutionStatus::Success,
        "Execution must succeed without pipe deadlock or timeout. stderr: {}",
        result.stderr
    );
    assert_eq!(result.stdout.len(), 500_000);
}

#[tokio::test]
async fn test_fast_terminating_background_helper() {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let workspace_id = "test-ws-fast-helper".to_string();
    let ws_dir = paths.workspace_dir(&workspace_id).unwrap();
    fs::create_dir_all(ws_dir.join("input")).unwrap();
    fs::create_dir_all(ws_dir.join("scratch")).unwrap();
    fs::create_dir_all(ws_dir.join("output")).unwrap();
    fs::create_dir_all(ws_dir.join("context")).unwrap();
    write_test_workspace_manifest(&paths, &workspace_id);

    let res_dir = paths.data_root().join("res_fast_helper");
    fs::create_dir_all(&res_dir).unwrap();

    // Parent exits immediately, helper sleeps 20ms and writes late.txt
    let script = r#"
import os, time, sys
child_pid = os.fork()
if child_pid == 0:
    time.sleep(0.02)
    with open('output/late.txt', 'w') as f:
        f.write('late')
    os._exit(0)
else:
    sys.exit(0)
"#;
    fs::write(res_dir.join("script.py"), script).unwrap();

    let manifest = WorkResourceManifest {
        id: "work-test-fast-helper".to_string(),
        name: "Test Fast Helper Capability".to_string(),
        description: "Process tree race fixture".to_string(),
        kind: WorkResourceKind::Capability,
        origin: crate::work::models::ResourceOrigin::Builtin,
        modes: vec![],
        runtimes: vec![],
        entry: "capabilities/fast_helper".to_string(),
        permissions: vec!["local-exec".to_string()],
        enabled: true,
        discovery: Default::default(),
        execution: Some(WorkExecutionManifest {
            trusted: true,
            action_risk_classes: Default::default(),
            runtime: WorkExecutionRuntime::Python,
            entry: "script.py".to_string(),
            actions: vec!["run".to_string()],
            network: ExecutionNetworkPolicy::None,
            timeout_seconds: 5,
            readable_areas: vec!["input".to_string()],
            writable_areas: vec!["scratch".to_string(), "output".to_string()],
        }),
    };

    let req = WorkExecutionRequest {
        workspace_id: workspace_id.clone(),
        resource_id: manifest.id.clone(),
        action: "run".to_string(),
        arguments: serde_json::json!({}),
        input_paths: vec![],
        expected_outputs: vec![],
        task_id: Some("task-fast-1".to_string()),
        work_run_id: Some("run-fast-1".to_string()),
    };

    let executor =
        WorkExecutor::new_restricted_host_with_sandbox(Box::new(FakeSandboxProvider::new_full()));
    let result = executor
        .execute(&paths, &manifest, &res_dir, &req, true)
        .await
        .unwrap();

    assert_eq!(result.status, WorkExecutionStatus::Success);

    // Wait 100ms and verify late.txt was NOT written
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        !ws_dir.join("output/late.txt").exists(),
        "Background helper must be killed immediately when direct child exits"
    );
}

#[test]
fn test_symlink_poison_sandbox_temp_anchor() {
    let temp = TempDir::new().unwrap();
    let ws_path = temp.path().join("ws");
    fs::create_dir_all(ws_path.join("scratch")).unwrap();
    fs::create_dir_all(ws_path.join("output")).unwrap();

    let external_probe = temp.path().join("external_probe");
    fs::create_dir_all(&external_probe).unwrap();

    let host_data_root = temp.path().join("data_root");
    fs::create_dir_all(&host_data_root).unwrap();

    // Adversary pre-plants a symlink at data_root/sandbox-tmp -> external_probe
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&external_probe, host_data_root.join("sandbox-tmp")).unwrap();

        let manifest = create_test_manifest(vec![], vec!["scratch"]);
        let res = SandboxPolicyResolver::resolve(
            &ws_path,
            None,
            &manifest,
            &[],
            &[],
            "run-poison",
            "exec-poison",
            &host_data_root,
        );

        assert!(
            res.is_err(),
            "SandboxPolicyResolver must fail-closed if sandbox-tmp anchor is a symlink"
        );
        assert!(
            res.unwrap_err().contains("is a symlink"),
            "Error message must mention symlink refusal"
        );

        // Verify Host created zero files/directories inside external_probe
        let entries: Vec<_> = fs::read_dir(&external_probe).unwrap().collect();
        assert!(
            entries.is_empty(),
            "Host must not create any files or directories in the external directory"
        );
    }
}
