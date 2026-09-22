use std::fs;
use std::path::Path;
use tempfile::TempDir;

use crate::work::executor::{WorkExecutionRequest, WorkExecutionStatus, WorkExecutor};
use crate::work::models::{
    ExecutionNetworkPolicy, WorkExecutionManifest, WorkExecutionRuntime, WorkResourceKind,
    WorkResourceManifest,
};
use crate::work::paths::WorkPaths;
use crate::work::sandbox::fake::FakeSandboxProvider;

fn create_test_executor() -> WorkExecutor {
    WorkExecutor::new_restricted_host_with_sandbox(Box::new(FakeSandboxProvider::new_full()))
}

fn create_test_workspace_layout() -> (TempDir, WorkPaths, String) {
    let temp = TempDir::new().unwrap();
    let paths = WorkPaths::new(temp.path().join("data"));
    paths.ensure_layout().unwrap();

    let workspace_id = "test-ws-1".to_string();
    let ws_dir = paths.workspace_dir(&workspace_id).unwrap();
    fs::create_dir_all(ws_dir.join("input")).unwrap();
    fs::create_dir_all(ws_dir.join("scratch")).unwrap();
    fs::create_dir_all(ws_dir.join("output")).unwrap();
    fs::create_dir_all(ws_dir.join("context")).unwrap();
    fs::write(
        paths.manifest_path(&workspace_id).unwrap(),
        serde_json::json!({
            "id": workspace_id,
            "name": "Test workspace",
            "root": ws_dir,
            "inputDir": ws_dir.join("input"),
            "scratchDir": ws_dir.join("scratch"),
            "outputDir": ws_dir.join("output"),
            "contextDir": ws_dir.join("context"),
            "createdAt": crate::models::now_iso(),
            "updatedAt": crate::models::now_iso()
        })
        .to_string(),
    )
    .unwrap();

    (temp, paths, workspace_id)
}

fn create_sample_manifest(trusted: bool, network: ExecutionNetworkPolicy) -> WorkResourceManifest {
    WorkResourceManifest {
        id: "work-test-text".to_string(),
        name: "Test Text Capability".to_string(),
        description: "Deterministic text transformer fixture".to_string(),
        kind: WorkResourceKind::Capability,
        origin: crate::work::models::ResourceOrigin::Builtin,
        modes: vec![],
        runtimes: vec![],
        entry: "capabilities/test".to_string(),
        permissions: vec!["local-exec".to_string()],
        enabled: true,
        discovery: Default::default(),
        execution: Some(WorkExecutionManifest {
            trusted,
            action_risk_classes: Default::default(),
            runtime: WorkExecutionRuntime::Python,
            entry: "script.py".to_string(),
            actions: vec!["transform".to_string()],
            network,
            timeout_seconds: 5,
            readable_areas: vec!["input".to_string()],
            writable_areas: vec!["output".to_string()],
        }),
    }
}

#[tokio::test]
async fn test_1_trusted_builtin_resource_executes() {
    let (_temp, paths, ws_id) = create_test_workspace_layout();
    let executor = create_test_executor();

    let res_dir = paths.data_root().join("res_1");

    fs::create_dir_all(&res_dir).unwrap();

    // Create simple python script
    let script = r#"
import sys, json
action = sys.argv[1]
args = json.loads(sys.argv[2])
if action == "transform":
    with open("input/in.txt", "r") as f:
        content = f.read()
    with open("output/out.txt", "w") as f:
        f.write(content.upper())
"#;
    fs::write(res_dir.join("script.py"), script).unwrap();

    // Create input file inside workspace
    let ws_dir = paths.workspace_dir(&ws_id).unwrap();
    fs::write(ws_dir.join("input/in.txt"), "hello work").unwrap();

    let manifest = create_sample_manifest(true, ExecutionNetworkPolicy::None);
    let req = WorkExecutionRequest {
        workspace_id: ws_id.clone(),
        resource_id: manifest.id.clone(),
        action: "transform".to_string(),
        arguments: serde_json::json!({}),
        input_paths: vec!["input/in.txt".to_string()],
        expected_outputs: vec!["output/out.txt".to_string()],
        task_id: None,
        work_run_id: None,
    };

    let result = executor
        .execute(&paths, &manifest, &res_dir, &req, true)
        .await
        .unwrap();

    assert_eq!(result.status, WorkExecutionStatus::Success);
    assert_eq!(result.outputs, vec!["output/out.txt"]);
    assert_eq!(
        fs::read_to_string(ws_dir.join("output/out.txt")).unwrap(),
        "HELLO WORK"
    );
}

#[tokio::test]
async fn test_2_unknown_or_missing_manifest_denied() {
    let (_temp, paths, ws_id) = create_test_workspace_layout();
    let executor = create_test_executor();

    let mut manifest = create_sample_manifest(true, ExecutionNetworkPolicy::None);
    manifest.execution = None;

    let req = WorkExecutionRequest {
        workspace_id: ws_id,
        resource_id: manifest.id.clone(),
        action: "transform".to_string(),
        arguments: serde_json::json!({}),
        input_paths: vec![],
        expected_outputs: vec![],
        task_id: None,
        work_run_id: None,
    };

    let res = executor
        .execute(&paths, &manifest, Path::new("/tmp"), &req, true)
        .await;
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("no execution manifest"));
}

#[tokio::test]
async fn test_5_untrusted_resource_denied_on_host_executor() {
    let (_temp, paths, ws_id) = create_test_workspace_layout();
    let executor = create_test_executor();

    let manifest = create_sample_manifest(false, ExecutionNetworkPolicy::None);
    let req = WorkExecutionRequest {
        workspace_id: ws_id,
        resource_id: manifest.id.clone(),
        action: "transform".to_string(),
        arguments: serde_json::json!({}),
        input_paths: vec![],
        expected_outputs: vec![],
        task_id: None,
        work_run_id: None,
    };

    let res = executor
        .execute(&paths, &manifest, Path::new("/tmp"), &req, false)
        .await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .contains("Restricted Host Executor only permits trusted built-in Work Resources"));
}

#[tokio::test]
async fn test_7_and_8_absolute_and_traversal_entry_denied() {
    let (_temp, paths, ws_id) = create_test_workspace_layout();
    let executor = create_test_executor();

    let mut manifest = create_sample_manifest(true, ExecutionNetworkPolicy::None);
    manifest.execution.as_mut().unwrap().entry = "../escape.py".to_string();

    let req = WorkExecutionRequest {
        workspace_id: ws_id,
        resource_id: manifest.id.clone(),
        action: "transform".to_string(),
        arguments: serde_json::json!({}),
        input_paths: vec![],
        expected_outputs: vec![],
        task_id: None,
        work_run_id: None,
    };

    let res = executor
        .execute(&paths, &manifest, Path::new("/tmp"), &req, true)
        .await;
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("cannot contain '..'"));
}

#[tokio::test]
async fn test_10_and_11_path_escapes_denied() {
    let (_temp, paths, ws_id) = create_test_workspace_layout();
    let executor = create_test_executor();

    let manifest = create_sample_manifest(true, ExecutionNetworkPolicy::None);
    let req = WorkExecutionRequest {
        workspace_id: ws_id,
        resource_id: manifest.id.clone(),
        action: "transform".to_string(),
        arguments: serde_json::json!({}),
        input_paths: vec!["../etc/passwd".to_string()],
        expected_outputs: vec![],
        task_id: None,
        work_run_id: None,
    };

    let res = executor
        .execute(&paths, &manifest, Path::new("/tmp"), &req, true)
        .await;
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("cannot contain . or .."));
}

#[tokio::test]
async fn test_18_execution_result_does_not_auto_create_artifact() {
    let (_temp, paths, ws_id) = create_test_workspace_layout();
    let executor = create_test_executor();

    let res_dir = paths.data_root().join("res_18");

    fs::create_dir_all(&res_dir).unwrap();
    fs::write(
        res_dir.join("script.py"),
        "import sys; open('output/doc.pdf', 'w').write('pdf')",
    )
    .unwrap();

    let manifest = create_sample_manifest(true, ExecutionNetworkPolicy::None);
    let req = WorkExecutionRequest {
        workspace_id: ws_id.clone(),
        resource_id: manifest.id.clone(),
        action: "transform".to_string(),
        arguments: serde_json::json!({}),
        input_paths: vec![],
        expected_outputs: vec!["output/doc.pdf".to_string()],
        task_id: None,
        work_run_id: None,
    };

    let result = executor
        .execute(&paths, &manifest, &res_dir, &req, true)
        .await
        .unwrap();
    assert_eq!(result.status, WorkExecutionStatus::Success);

    // Verify artifact registry was NOT touched automatically
    let artifact_registry_path = paths
        .workspace_dir(&ws_id)
        .unwrap()
        .join("context/artifacts.json");
    assert!(!artifact_registry_path.exists());
}

#[tokio::test]
async fn test_21_network_capabilities_fail_closed_on_host_executor() {
    let (_temp, paths, ws_id) = create_test_workspace_layout();
    let executor = create_test_executor();

    let manifest = create_sample_manifest(true, ExecutionNetworkPolicy::Internet);
    let req = WorkExecutionRequest {
        workspace_id: ws_id,
        resource_id: manifest.id.clone(),
        action: "transform".to_string(),
        arguments: serde_json::json!({}),
        input_paths: vec![],
        expected_outputs: vec![],
        task_id: None,
        work_run_id: None,
    };

    let res = executor
        .execute(&paths, &manifest, Path::new("/tmp"), &req, true)
        .await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .contains("does not support network-enabled executable capabilities"));
}

#[tokio::test]
async fn test_22_executor_fails_closed_when_sandbox_fails() {
    let (_temp, paths, ws_id) = create_test_workspace_layout();
    let unavailable_sandbox = FakeSandboxProvider::new_unavailable();
    let executor = WorkExecutor::new_restricted_host_with_sandbox(Box::new(unavailable_sandbox));

    let res_dir = paths.data_root().join("res_22");
    fs::create_dir_all(&res_dir).unwrap();
    fs::write(res_dir.join("script.py"), "print('hi')").unwrap();

    let manifest = create_sample_manifest(true, ExecutionNetworkPolicy::None);
    let req = WorkExecutionRequest {
        workspace_id: ws_id,
        resource_id: manifest.id.clone(),
        action: "transform".to_string(),
        arguments: serde_json::json!({}),
        input_paths: vec![],
        expected_outputs: vec![],
        task_id: Some("task-1".to_string()),
        work_run_id: Some("run-1".to_string()),
    };

    let res = executor
        .execute(&paths, &manifest, &res_dir, &req, true)
        .await
        .unwrap();

    assert_eq!(res.status, WorkExecutionStatus::Failed);
    assert_eq!(
        res.failure_kind,
        Some(crate::work::executor::ExecutionFailureKind::SandboxInfrastructureFailure)
    );
    assert!(
        res.stderr
            .contains("Native Sandbox confinement failed (fail-closed)"),
        "Error must indicate fail-closed sandbox refusal: {}",
        res.stderr
    );
}
