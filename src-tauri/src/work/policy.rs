use crate::work::models::{
    CollaborationMode, CommandRiskClassification, ExecutionContext, TaskStandingRule,
    ToolRiskClass, WorkExecutionMode, WorkPolicy, WorkResourceManifest,
};

pub struct PolicyEvaluator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkPolicyDecision {
    Allow,
    Ask,
    Deny,
}

impl PolicyEvaluator {
    /// Classify standard built-in or connector tool risk level.
    pub fn classify_tool_risk(tool_name: &str) -> ToolRiskClass {
        let name_lower = tool_name.to_lowercase();
        if name_lower.starts_with("work_read")
            || name_lower.starts_with("work_list")
            || name_lower.starts_with("work_inspect")
            || name_lower == "work_command_info"
            || name_lower == "work_validate_artifact"
            || name_lower.starts_with("web_")
            || name_lower == "browser_navigate"
            || name_lower == "browser_snapshot"
            || name_lower == "browser_take_screenshot"
            || name_lower == "browser_wait_for"
            || name_lower == "browser_tabs"
            || name_lower == "browser_scroll"
            || name_lower == "browser_close"
            || name_lower == "desktop_list_apps"
            || name_lower == "desktop_probe_app"
            || name_lower == "desktop_observe"
            || name_lower == "desktop_screenshot"
            || name_lower == "desktop_release"
            || name_lower == "find_roots"
            || name_lower == "observe_ui"
            || name_lower == "search_ui"
            || name_lower == "expand_ui"
            || name_lower == "inspect_ui"
            || name_lower == "read_text"
            || name_lower == "wait_for"
            || name_lower.starts_with("read_")
            || name_lower.starts_with("view_")
            || name_lower.starts_with("list_")
            || name_lower.starts_with("library_")
        {
            ToolRiskClass::Read
        } else if name_lower.starts_with("work_write")
            || name_lower.starts_with("work_edit")
            || name_lower == "work_update_context"
            || name_lower.starts_with("work_scratch")
            || name_lower.starts_with("work_artifact_create")
            || name_lower == "work_register_artifact"
            || name_lower == "work_deliver"
            || name_lower == "browser_click"
            || name_lower == "browser_type"
            || name_lower == "browser_select_option"
            || name_lower == "desktop_open_app"
            || name_lower == "desktop_click"
            || name_lower == "desktop_type"
            || name_lower == "desktop_key"
            || name_lower == "desktop_scroll"
            || name_lower == "desktop_act_batch"
            || name_lower == "launch_app"
            || name_lower == "act_ui"
            || name_lower.starts_with("write_")
            || name_lower.starts_with("edit_")
        {
            ToolRiskClass::WriteLocal
        } else if name_lower.starts_with("work_execute")
            || name_lower.starts_with("work_run_command")
            || name_lower.starts_with("bash")
            || name_lower.starts_with("run_command")
            || name_lower.starts_with("exec_")
        {
            ToolRiskClass::Exec
        } else {
            // Default unknown MCP tools / external connectors to External (high risk, fail-closed unless standing rule or confirmed)
            ToolRiskClass::External
        }
    }

    /// Check if a given tool invocation has a matching task-scoped standing rule.
    pub fn find_matching_rule<'a>(
        policy: &'a WorkPolicy,
        tool_name: &str,
        target: &str,
    ) -> Option<&'a TaskStandingRule> {
        policy
            .standing_rules
            .iter()
            .find(|rule| Self::matches_rule(rule, tool_name, target))
    }

    /// Match rule against tool name and target pattern (supports glob wildcard *).
    pub fn matches_rule(rule: &TaskStandingRule, tool_name: &str, target: &str) -> bool {
        if rule.tool_name != "*" && !rule.tool_name.eq_ignore_ascii_case(tool_name) {
            return false;
        }

        let pattern = &rule.target_pattern;
        if pattern == "*" || pattern == target {
            return true;
        }

        let parts: Vec<&str> = pattern.split('*').collect();
        match parts.as_slice() {
            [prefix, suffix] => target.starts_with(prefix) && target.ends_with(suffix),
            _ => false,
        }
    }

    /// Check whether a tool call requires explicit human confirmation under the current policy.
    pub fn requires_confirmation(policy: &WorkPolicy, tool_name: &str, target: &str) -> bool {
        matches!(
            Self::evaluate_decision(policy, tool_name, target, ExecutionContext::Attended),
            WorkPolicyDecision::Ask
        )
    }

    /// Autonomy level of an execution mode (higher = more permissive).
    /// PlanFirst (0) < Direct (1) < Auto (2) < FullAccess (3).
    pub fn autonomy_level(mode: WorkExecutionMode) -> u8 {
        match mode {
            WorkExecutionMode::PlanFirst => 0,
            WorkExecutionMode::Direct => 1,
            WorkExecutionMode::Auto => 2,
            WorkExecutionMode::FullAccess => 3,
        }
    }

    /// Clamp a task-level execution mode to the workspace ceiling.
    /// A task may be stricter than the workspace default but never more permissive.
    pub fn clamp_execution_mode_to_ceiling(
        task_mode: WorkExecutionMode,
        ceiling: WorkExecutionMode,
    ) -> WorkExecutionMode {
        if Self::autonomy_level(task_mode) > Self::autonomy_level(ceiling) {
            ceiling
        } else {
            task_mode
        }
    }

    /// Resolve the risk class of a single capability action (`work_execute`).
    /// Uses the action-level declaration from the resource execution manifest;
    /// undeclared actions fail closed at the tool-level Exec classification.
    pub fn classify_capability_action_risk(
        resource_manifest: &WorkResourceManifest,
        action: &str,
    ) -> ToolRiskClass {
        if let Some(execution) = &resource_manifest.execution {
            if let Some(risk) = execution.action_risk_classes.get(action) {
                return *risk;
            }
        }
        ToolRiskClass::Exec
    }

    /// Evaluate a task-scoped action with explicit CollaborationMode.
    pub fn evaluate_with_collaboration_mode(
        policy: &WorkPolicy,
        collaboration_mode: CollaborationMode,
        tool_name: &str,
        target: &str,
        execution_context: ExecutionContext,
    ) -> WorkPolicyDecision {
        let risk = Self::classify_tool_risk(tool_name);
        Self::evaluate_with_risk(
            policy,
            collaboration_mode,
            risk,
            tool_name,
            target,
            execution_context,
        )
    }

    /// Evaluate a task-scoped action with an explicitly resolved risk class.
    /// `tool_name` and `target` are still used for standing-rule matching.
    pub fn evaluate_with_risk(
        policy: &WorkPolicy,
        collaboration_mode: CollaborationMode,
        risk: ToolRiskClass,
        tool_name: &str,
        target: &str,
        execution_context: ExecutionContext,
    ) -> WorkPolicyDecision {
        // In Plan collaboration mode, mutating actions are denied (true read-only).
        if collaboration_mode == CollaborationMode::Plan && risk != ToolRiskClass::Read {
            return WorkPolicyDecision::Deny;
        }

        // Under PlanFirst (Discuss) mode, any mutating or external action is denied —
        // a strictly read-only exploration mode that never raises an approval card.
        if policy.execution_mode == WorkExecutionMode::PlanFirst && risk != ToolRiskClass::Read {
            return WorkPolicyDecision::Deny;
        }

        // Workspace knowledge updates ALWAYS require explicit human approval,
        // regardless of Auto or FullAccess execution modes.
        if tool_name == "work_update_context" {
            return WorkPolicyDecision::Ask;
        }

        // Full access is an explicit user opt-in that bypasses Work's approval
        // and connector-policy gates. The lower-level tool implementation still
        // records the operation and enforces its input/output contracts.
        if policy.execution_mode == WorkExecutionMode::FullAccess {
            return WorkPolicyDecision::Allow;
        }

        if risk == ToolRiskClass::External && !policy.allow_external_connectors {
            return WorkPolicyDecision::Deny;
        }

        // Read actions within workspace bounds are auto-allowed
        if risk == ToolRiskClass::Read {
            return WorkPolicyDecision::Allow;
        }

        // Check if covered by a standing rule
        if Self::find_matching_rule(policy, tool_name, target).is_some() {
            return WorkPolicyDecision::Allow;
        }

        // In Unattended execution context, mutating/interactive operations must ask for human attention
        // (entering Inbox as Needs Attention) unless explicitly covered by a standing rule or FullAccess.
        if execution_context == ExecutionContext::Unattended {
            return WorkPolicyDecision::Ask;
        }

        // Auto mode is the explicit task-level opt-in for attended execution.
        // Unattended mode NEVER increases autonomy ceiling beyond configured policy.
        if policy.execution_mode == WorkExecutionMode::Auto {
            return WorkPolicyDecision::Allow;
        }

        // Default AgentCabin safe principle: Unknown / write / exec / external require explicit confirmation
        WorkPolicyDecision::Ask
    }

    /// Evaluate a task-scoped action at the runtime boundary.
    pub fn evaluate_decision(
        policy: &WorkPolicy,
        tool_name: &str,
        target: &str,
        execution_context: ExecutionContext,
    ) -> WorkPolicyDecision {
        Self::evaluate_with_collaboration_mode(
            policy,
            CollaborationMode::Default,
            tool_name,
            target,
            execution_context,
        )
    }

    /// Evaluate whether a capability execution request is authorized under the execution policy.
    pub fn evaluate_execution(
        resource_manifest: &WorkResourceManifest,
        request: &crate::work::executor::WorkExecutionRequest,
        is_builtin_trusted: bool,
    ) -> Result<(), String> {
        if resource_manifest.kind != crate::work::models::WorkResourceKind::Capability {
            return Err(format!(
                "Resource '{}' is of kind '{:?}', but work_execute only executes resources of kind 'Capability'",
                resource_manifest.id, resource_manifest.kind
            ));
        }

        let exec_manifest = resource_manifest.execution.as_ref().ok_or_else(|| {
            format!(
                "Resource '{}' has no execution manifest",
                resource_manifest.id
            )
        })?;

        if !resource_manifest.enabled {
            return Err(format!("Resource '{}' is disabled", resource_manifest.id));
        }

        if !is_builtin_trusted || !exec_manifest.trusted {
            return Err(format!(
                "Resource '{}' contains executable code, but Restricted Host Executor only permits trusted built-in Work Resources. Third-party executable capabilities will be supported in future isolated VM executors.",
                resource_manifest.id
            ));
        }

        if exec_manifest.network != crate::work::models::ExecutionNetworkPolicy::None {
            return Err(format!(
                "Current Restricted Host Executor does not support network-enabled executable capabilities (requested network: {:?})",
                exec_manifest.network
            ));
        }

        if !exec_manifest.actions.contains(&request.action) {
            return Err(format!(
                "Action '{}' is not declared in execution manifest for resource '{}'",
                request.action, resource_manifest.id
            ));
        }

        Ok(())
    }

    /// Check if a path string references sensitive credentials or host security directories.
    pub fn is_credential_or_sensitive_path(path: &str) -> bool {
        let p = path.trim().to_lowercase().replace('\\', "/");
        p.contains("/.ssh")
            || p.starts_with(".ssh")
            || p.contains("/id_rsa")
            || p.contains("/id_ed25519")
            || p.contains("/.aws")
            || p.starts_with(".aws")
            || p.contains("/.config/gcloud")
            || p.contains("/.config/gh")
            || p.contains("/.netrc")
            || p.contains("/.kube")
            || p.starts_with(".kube")
            || p.contains("/.gnupg")
            || p.starts_with(".gnupg")
            || p == "/etc/shadow"
            || p.starts_with("/etc/shadow")
            || p == "/etc/sudoers"
            || p.starts_with("/etc/sudoers")
            || p.contains("/keychains/")
            || p.ends_with(".keychain")
            || p.ends_with(".keychain-db")
    }

    /// Classify the specific risk profile of an execution command.
    pub fn classify_command_risk(
        command: &str,
        args: &[String],
        cwd: &str,
        target_path: Option<&str>,
    ) -> CommandRiskClassification {
        // 1. Check credential or sensitive path references across command, args, cwd, and target_path
        if Self::is_credential_or_sensitive_path(command)
            || Self::is_credential_or_sensitive_path(cwd)
            || target_path.is_some_and(Self::is_credential_or_sensitive_path)
            || args
                .iter()
                .any(|arg| Self::is_credential_or_sensitive_path(arg))
        {
            let offending = target_path
                .filter(|p| Self::is_credential_or_sensitive_path(p))
                .or_else(|| {
                    args.iter()
                        .find(|arg| Self::is_credential_or_sensitive_path(arg))
                        .map(String::as_str)
                })
                .unwrap_or(command);
            return CommandRiskClassification::CredentialOrSensitivePath {
                path: offending.to_string(),
            };
        }

        let cmd_base = std::path::Path::new(command)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(command)
            .to_lowercase();

        // 2. Office structured host capability
        if matches!(
            cmd_base.as_str(),
            "soffice" | "soffice.bin" | "libreoffice" | "ooffice"
        ) {
            let cmd_path = std::path::Path::new(command);
            if (cmd_path.is_absolute() || command.contains('/') || command.contains('\\'))
                && !crate::work::command_info::is_trusted_office_executable_path(cmd_path)
            {
                return CommandRiskClassification::SandboxedCommand;
            }
            return CommandRiskClassification::TrustedHostCapability;
        }

        if cmd_base == "work_command_info" {
            return CommandRiskClassification::TrustedHostCapability;
        }

        // 3. Destructive commands: rm, rmdir, unlink, shred, git reset --hard, git clean -f, git push --force
        if matches!(cmd_base.as_str(), "rm" | "rmdir" | "unlink" | "shred") {
            return CommandRiskClassification::Destructive {
                reason: format!("文件删除或清除命令: {command}"),
            };
        }
        if cmd_base == "git" {
            if args.iter().any(|a| a == "--hard") && args.iter().any(|a| a == "reset") {
                return CommandRiskClassification::Destructive {
                    reason: "git reset --hard 破坏性变更重置".to_string(),
                };
            }
            if args.iter().any(|a| a == "-f" || a == "-fd" || a == "-fdx")
                && args.iter().any(|a| a == "clean")
            {
                return CommandRiskClassification::Destructive {
                    reason: "git clean 强制清理未跟踪文件".to_string(),
                };
            }
            if args.iter().any(|a| a == "push")
                && args
                    .iter()
                    .any(|a| a == "--force" || a == "-f" || a.starts_with("--force-with-lease"))
            {
                return CommandRiskClassification::Destructive {
                    reason: "git push --force 强制推送到远程仓库".to_string(),
                };
            }
        }

        // 4. Dependency installation: pip, npm, yarn, pnpm, cargo add, brew install
        if matches!(cmd_base.as_str(), "pip" | "pip3")
            || (matches!(cmd_base.as_str(), "python" | "python3")
                && args.windows(2).any(|w| w[0] == "-m" && w[1] == "pip"))
        {
            let install_pos = args.iter().position(|a| a == "install");
            if let Some(idx) = install_pos {
                let mut packages = Vec::new();
                let mut source = None;
                let mut skip_next = false;
                for arg in &args[idx + 1..] {
                    if skip_next {
                        skip_next = false;
                        continue;
                    }
                    if arg == "-i" || arg == "--index-url" || arg == "--extra-index-url" {
                        skip_next = true;
                        source = Some(arg.clone());
                        continue;
                    }
                    if arg.starts_with("--index-url=") || arg.starts_with("--extra-index-url=") {
                        source = Some(arg.clone());
                        continue;
                    }
                    if arg.starts_with('-') {
                        continue;
                    }
                    packages.push(arg.clone());
                }
                return CommandRiskClassification::DependencyInstall {
                    package_manager: "pip".to_string(),
                    packages,
                    source,
                };
            }
        }

        if matches!(cmd_base.as_str(), "npm" | "pnpm" | "yarn" | "bun") {
            let is_install = args
                .iter()
                .any(|a| matches!(a.as_str(), "install" | "i" | "add"));
            if is_install {
                let mut packages = Vec::new();
                let mut source = None;
                let mut skip_next = false;
                for arg in args {
                    if skip_next {
                        skip_next = false;
                        continue;
                    }
                    if arg == "--registry" {
                        skip_next = true;
                        source = Some(arg.clone());
                        continue;
                    }
                    if arg.starts_with("--registry=") {
                        source = Some(arg.clone());
                        continue;
                    }
                    if arg.starts_with('-') || matches!(arg.as_str(), "install" | "i" | "add") {
                        continue;
                    }
                    packages.push(arg.clone());
                }
                return CommandRiskClassification::DependencyInstall {
                    package_manager: cmd_base,
                    packages,
                    source,
                };
            }
        }

        if cmd_base == "cargo" && args.iter().any(|a| a == "add") {
            let packages: Vec<String> = args
                .iter()
                .filter(|a| !a.starts_with('-') && *a != "add")
                .cloned()
                .collect();
            return CommandRiskClassification::DependencyInstall {
                package_manager: "cargo".to_string(),
                packages,
                source: None,
            };
        }

        if cmd_base == "brew" && args.iter().any(|a| a == "install") {
            let packages: Vec<String> = args
                .iter()
                .filter(|a| !a.starts_with('-') && *a != "install")
                .cloned()
                .collect();
            return CommandRiskClassification::DependencyInstall {
                package_manager: "brew".to_string(),
                packages,
                source: None,
            };
        }

        // 5. Read-only analysis commands inside workspace
        let is_workspace_read = if matches!(
            cmd_base.as_str(),
            "rg" | "grep"
                | "cat"
                | "head"
                | "tail"
                | "ls"
                | "wc"
                | "diff"
                | "find"
                | "stat"
                | "file"
                | "tree"
                | "du"
                | "df"
                | "pwd"
                | "which"
                | "echo"
                | "true"
                | "false"
                | "test"
        ) {
            true
        } else if cmd_base == "git" {
            if let Some(subcmd) = Self::extract_git_subcommand(args) {
                match subcmd {
                    "status" | "log" | "diff" | "show" | "rev-parse" | "describe" | "shortlog"
                    | "check-ignore" | "ls-files" | "cat-file" | "merge-base" | "blame" => true,
                    "branch" => !args.iter().any(|a| {
                        a == "-d" || a == "-D" || a == "-m" || a == "-M" || a == "--delete"
                    }),
                    "tag" => !args.iter().any(|a| a == "-d" || a == "--delete"),
                    "remote" => !args.iter().any(|a| {
                        a == "add" || a == "remove" || a == "rm" || a == "set-url" || a == "rename"
                    }),
                    _ => false,
                }
            } else {
                false
            }
        } else {
            false
        };

        if is_workspace_read {
            return CommandRiskClassification::WorkspaceRead;
        }

        // 6. Otherwise standard sandboxed command
        CommandRiskClassification::SandboxedCommand
    }

    /// Extract the git subcommand from args, skipping global options like `-C <dir>`, `--no-pager`, etc.
    pub fn extract_git_subcommand(args: &[String]) -> Option<&str> {
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            if arg == "-C" || arg == "-c" || arg == "--git-dir" || arg == "--work-tree" {
                let _ = iter.next();
                continue;
            }
            if arg.starts_with('-') {
                continue;
            }
            return Some(arg.as_str());
        }
        None
    }

    /// Evaluate command risk classification against the policy.
    pub fn evaluate_command_risk(
        policy: &WorkPolicy,
        classification: &CommandRiskClassification,
        tool_name: &str,
        target: &str,
    ) -> WorkPolicyDecision {
        // Full access bypasses approval except for security-boundary credential paths
        if policy.execution_mode == WorkExecutionMode::FullAccess {
            return match classification {
                CommandRiskClassification::CredentialOrSensitivePath { .. } => {
                    WorkPolicyDecision::Deny
                }
                _ => WorkPolicyDecision::Allow,
            };
        }

        // Standing rule matches
        if Self::find_matching_rule(policy, tool_name, target).is_some() {
            return match classification {
                CommandRiskClassification::CredentialOrSensitivePath { .. } => {
                    WorkPolicyDecision::Deny
                }
                _ => WorkPolicyDecision::Allow,
            };
        }

        // PlanFirst mode (read-only)
        if policy.execution_mode == WorkExecutionMode::PlanFirst {
            return match classification {
                CommandRiskClassification::WorkspaceRead => WorkPolicyDecision::Allow,
                _ => WorkPolicyDecision::Deny,
            };
        }

        // Under Default Permissions (Auto)
        match classification {
            CommandRiskClassification::CredentialOrSensitivePath { .. } => WorkPolicyDecision::Deny,
            CommandRiskClassification::WorkspaceRead => WorkPolicyDecision::Allow,
            CommandRiskClassification::WorkspaceWrite => WorkPolicyDecision::Allow,
            CommandRiskClassification::TrustedHostCapability => WorkPolicyDecision::Allow,
            CommandRiskClassification::SandboxedCommand => WorkPolicyDecision::Allow,
            CommandRiskClassification::HostCommandFallback => WorkPolicyDecision::Ask,
            CommandRiskClassification::DependencyInstall { .. } => WorkPolicyDecision::Ask,
            CommandRiskClassification::ExternalPathAccess { .. } => WorkPolicyDecision::Ask,
            CommandRiskClassification::Destructive { .. } => WorkPolicyDecision::Ask,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_tool_risks_correctly() {
        assert_eq!(
            PolicyEvaluator::classify_tool_risk("work_read_file"),
            ToolRiskClass::Read
        );
        assert_eq!(
            PolicyEvaluator::classify_tool_risk("work_write_output"),
            ToolRiskClass::WriteLocal
        );
        assert_eq!(
            PolicyEvaluator::classify_tool_risk("work_execute_python"),
            ToolRiskClass::Exec
        );
        assert_eq!(
            PolicyEvaluator::classify_tool_risk("work_run_command"),
            ToolRiskClass::Exec
        );
        assert_eq!(
            PolicyEvaluator::classify_tool_risk("feishu_send_message"),
            ToolRiskClass::External
        );
        assert_eq!(
            PolicyEvaluator::classify_tool_risk("desktop_observe"),
            ToolRiskClass::Read
        );
        assert_eq!(
            PolicyEvaluator::classify_tool_risk("desktop_click"),
            ToolRiskClass::WriteLocal
        );
        assert_eq!(
            PolicyEvaluator::classify_tool_risk("observe_ui"),
            ToolRiskClass::Read
        );
        assert_eq!(
            PolicyEvaluator::classify_tool_risk("act_ui"),
            ToolRiskClass::WriteLocal
        );
    }

    #[test]
    fn matches_standing_rules_with_wildcards() {
        let rule = TaskStandingRule {
            id: "r1".to_string(),
            tool_name: "slack_send".to_string(),
            target_pattern: "channel:#dev-*".to_string(),
            risk_class: ToolRiskClass::External,
            granted_at: "2026-08-13T00:00:00Z".to_string(),
        };

        assert!(PolicyEvaluator::matches_rule(
            &rule,
            "slack_send",
            "channel:#dev-team"
        ));
        assert!(!PolicyEvaluator::matches_rule(
            &rule,
            "slack_send",
            "channel:#general"
        ));
        assert!(!PolicyEvaluator::matches_rule(
            &rule,
            "feishu_send",
            "channel:#dev-team"
        ));
    }

    #[test]
    fn evaluates_confirmation_requirements() {
        let mut policy = WorkPolicy::default();

        // Default direct mode: read is allowed, write is fail-closed
        assert!(!PolicyEvaluator::requires_confirmation(
            &policy,
            "work_read_file",
            "scratch/test.txt"
        ));
        assert!(PolicyEvaluator::requires_confirmation(
            &policy,
            "work_write_file",
            "output/report.xlsx"
        ));

        // Add standing rule
        policy.standing_rules.push(TaskStandingRule {
            id: "r1".to_string(),
            tool_name: "work_write_file".to_string(),
            target_pattern: "output/*.xlsx".to_string(),
            risk_class: ToolRiskClass::WriteLocal,
            granted_at: "2026-08-13T00:00:00Z".to_string(),
        });

        // Now writing to output/report.xlsx is auto-allowed
        assert!(!PolicyEvaluator::requires_confirmation(
            &policy,
            "work_write_file",
            "output/report.xlsx"
        ));
        assert!(PolicyEvaluator::requires_confirmation(
            &policy,
            "work_write_file",
            "output/other.pdf"
        ));
    }

    #[test]
    fn evaluates_runtime_decisions_as_allow_ask_or_deny() {
        let mut policy = WorkPolicy::default();
        assert_eq!(
            PolicyEvaluator::evaluate_decision(
                &policy,
                "work_read_file",
                "input/source.txt",
                ExecutionContext::Unattended,
            ),
            WorkPolicyDecision::Allow
        );
        assert_eq!(
            PolicyEvaluator::evaluate_decision(
                &policy,
                "work_execute",
                "builtin.transform",
                ExecutionContext::Unattended,
            ),
            WorkPolicyDecision::Ask
        );

        policy.execution_mode = WorkExecutionMode::Auto;
        assert_eq!(
            PolicyEvaluator::evaluate_decision(
                &policy,
                "work_execute",
                "builtin.transform",
                ExecutionContext::Attended,
            ),
            WorkPolicyDecision::Allow
        );
        assert_eq!(
            PolicyEvaluator::evaluate_decision(
                &policy,
                "work_execute",
                "builtin.transform",
                ExecutionContext::Unattended,
            ),
            WorkPolicyDecision::Ask
        );

        assert_eq!(
            PolicyEvaluator::evaluate_decision(
                &WorkPolicy::default(),
                "mcp",
                "connector.send",
                ExecutionContext::Unattended,
            ),
            WorkPolicyDecision::Deny
        );

        policy.execution_mode = WorkExecutionMode::FullAccess;
        policy.allow_external_connectors = false;
        assert_eq!(
            PolicyEvaluator::evaluate_decision(
                &policy,
                "mcp",
                "connector.send",
                ExecutionContext::Unattended,
            ),
            WorkPolicyDecision::Allow
        );
        assert_eq!(
            PolicyEvaluator::autonomy_level(WorkExecutionMode::FullAccess),
            3
        );
        assert_eq!(
            PolicyEvaluator::clamp_execution_mode_to_ceiling(
                WorkExecutionMode::FullAccess,
                WorkExecutionMode::Auto,
            ),
            WorkExecutionMode::Auto
        );
    }

    fn manifest_with_action_risks(risks: &[(&str, ToolRiskClass)]) -> WorkResourceManifest {
        use crate::agent::capability_resolver::RuntimeProviderKind;
        use crate::work::models::{
            AppMode, ExecutionNetworkPolicy, ResourceOrigin, WorkExecutionManifest,
            WorkExecutionRuntime, WorkResourceDiscovery, WorkResourceKind,
        };
        let mut action_risk_classes = std::collections::HashMap::new();
        for (action, risk) in risks {
            action_risk_classes.insert(action.to_string(), *risk);
        }
        WorkResourceManifest {
            id: "test-cap".to_string(),
            name: "Test".to_string(),
            description: String::new(),
            kind: WorkResourceKind::Capability,
            origin: ResourceOrigin::Builtin,
            modes: vec![AppMode::Work],
            runtimes: vec![RuntimeProviderKind::Pi],
            entry: "entry.mjs".to_string(),
            permissions: Vec::new(),
            enabled: true,
            discovery: WorkResourceDiscovery::default(),
            execution: Some(WorkExecutionManifest {
                trusted: true,
                runtime: WorkExecutionRuntime::Node,
                entry: "entry.mjs".to_string(),
                actions: risks.iter().map(|(a, _)| a.to_string()).collect(),
                action_risk_classes,
                network: ExecutionNetworkPolicy::None,
                timeout_seconds: 60,
                readable_areas: vec!["input".to_string()],
                writable_areas: Vec::new(),
            }),
        }
    }

    #[test]
    fn classifies_capability_action_risk_from_manifest() {
        let manifest = manifest_with_action_risks(&[
            ("inspect_dataset", ToolRiskClass::Read),
            ("export_table", ToolRiskClass::WriteLocal),
        ]);
        assert_eq!(
            PolicyEvaluator::classify_capability_action_risk(&manifest, "inspect_dataset"),
            ToolRiskClass::Read
        );
        assert_eq!(
            PolicyEvaluator::classify_capability_action_risk(&manifest, "export_table"),
            ToolRiskClass::WriteLocal
        );
        // Undeclared action fails closed at Exec
        assert_eq!(
            PolicyEvaluator::classify_capability_action_risk(&manifest, "unknown_action"),
            ToolRiskClass::Exec
        );
    }

    #[test]
    fn classify_capability_action_risk_without_execution_fails_closed() {
        let mut manifest = manifest_with_action_risks(&[("x", ToolRiskClass::Read)]);
        manifest.execution = None;
        assert_eq!(
            PolicyEvaluator::classify_capability_action_risk(&manifest, "x"),
            ToolRiskClass::Exec
        );
    }

    #[test]
    fn evaluate_with_risk_allows_readonly_capability_action_in_direct_mode() {
        let policy = WorkPolicy::default(); // Direct
        assert_eq!(
            PolicyEvaluator::evaluate_with_risk(
                &policy,
                CollaborationMode::Default,
                ToolRiskClass::Read,
                "work_execute",
                "work-data-analysis.inspect_dataset",
                ExecutionContext::Attended,
            ),
            WorkPolicyDecision::Allow
        );
    }

    #[test]
    fn evaluate_with_risk_asks_for_write_capability_action_in_direct_mode() {
        let policy = WorkPolicy::default();
        assert_eq!(
            PolicyEvaluator::evaluate_with_risk(
                &policy,
                CollaborationMode::Default,
                ToolRiskClass::WriteLocal,
                "work_execute",
                "work-excel.export_table",
                ExecutionContext::Attended,
            ),
            WorkPolicyDecision::Ask
        );
    }

    #[test]
    fn classifies_office_commands_as_trusted_host_capability() {
        assert_eq!(
            PolicyEvaluator::classify_command_risk(
                "soffice",
                &["--headless".into(), "--convert-to".into(), "xlsx".into()],
                "scratch",
                None
            ),
            CommandRiskClassification::TrustedHostCapability
        );
        assert_eq!(
            PolicyEvaluator::classify_command_risk(
                "/Applications/LibreOffice.app/Contents/MacOS/soffice",
                &["--headless".into()],
                "scratch",
                None
            ),
            CommandRiskClassification::TrustedHostCapability
        );
        assert_eq!(
            PolicyEvaluator::classify_command_risk("work_command_info", &[], "scratch", None),
            CommandRiskClassification::TrustedHostCapability
        );
    }

    #[test]
    fn classifies_untrusted_office_path_as_sandboxed_command() {
        assert_eq!(
            PolicyEvaluator::classify_command_risk("/tmp/soffice", &[], "scratch", None),
            CommandRiskClassification::SandboxedCommand
        );
        assert_eq!(
            PolicyEvaluator::classify_command_risk("./soffice", &[], "scratch", None),
            CommandRiskClassification::SandboxedCommand
        );
    }

    #[test]
    fn classifies_pip_and_npm_install_as_dependency_install() {
        let pip_risk = PolicyEvaluator::classify_command_risk(
            "pip",
            &["install".into(), "formulas".into(), "openpyxl".into()],
            "scratch",
            None,
        );
        match pip_risk {
            CommandRiskClassification::DependencyInstall {
                package_manager,
                packages,
                ..
            } => {
                assert_eq!(package_manager, "pip");
                assert_eq!(packages, vec!["formulas", "openpyxl"]);
            }
            other => panic!("expected DependencyInstall, got {other:?}"),
        }

        let npm_risk = PolicyEvaluator::classify_command_risk(
            "npm",
            &["install".into(), "exceljs".into()],
            "scratch",
            None,
        );
        match npm_risk {
            CommandRiskClassification::DependencyInstall {
                package_manager,
                packages,
                ..
            } => {
                assert_eq!(package_manager, "npm");
                assert_eq!(packages, vec!["exceljs"]);
            }
            other => panic!("expected DependencyInstall, got {other:?}"),
        }
    }

    #[test]
    fn classifies_destructive_commands() {
        assert!(matches!(
            PolicyEvaluator::classify_command_risk(
                "rm",
                &["-rf".into(), "output".into()],
                "scratch",
                None
            ),
            CommandRiskClassification::Destructive { .. }
        ));

        assert!(matches!(
            PolicyEvaluator::classify_command_risk(
                "git",
                &["reset".into(), "--hard".into(), "HEAD".into()],
                "scratch",
                None
            ),
            CommandRiskClassification::Destructive { .. }
        ));
    }

    #[test]
    fn classifies_credential_and_sensitive_paths_and_denies_them() {
        let sensitive = PolicyEvaluator::classify_command_risk(
            "cat",
            &["~/.ssh/id_rsa".into()],
            "scratch",
            None,
        );
        assert!(matches!(
            sensitive,
            CommandRiskClassification::CredentialOrSensitivePath { .. }
        ));

        let auto_policy = WorkPolicy {
            execution_mode: WorkExecutionMode::Auto,
            ..Default::default()
        };
        assert_eq!(
            PolicyEvaluator::evaluate_command_risk(
                &auto_policy,
                &sensitive,
                "work_run_command",
                "cat"
            ),
            WorkPolicyDecision::Deny
        );

        let full_access_policy = WorkPolicy {
            execution_mode: WorkExecutionMode::FullAccess,
            ..Default::default()
        };
        assert_eq!(
            PolicyEvaluator::evaluate_command_risk(
                &full_access_policy,
                &sensitive,
                "work_run_command",
                "cat"
            ),
            WorkPolicyDecision::Deny
        );
    }

    #[test]
    fn default_permissions_auto_mode_allows_office_and_workspace_reads_but_asks_install() {
        let auto_policy = WorkPolicy {
            execution_mode: WorkExecutionMode::Auto,
            ..Default::default()
        };

        let office = CommandRiskClassification::TrustedHostCapability;
        assert_eq!(
            PolicyEvaluator::evaluate_command_risk(
                &auto_policy,
                &office,
                "work_run_command",
                "soffice"
            ),
            WorkPolicyDecision::Allow
        );

        let read = CommandRiskClassification::WorkspaceRead;
        assert_eq!(
            PolicyEvaluator::evaluate_command_risk(&auto_policy, &read, "work_run_command", "grep"),
            WorkPolicyDecision::Allow
        );

        let install = CommandRiskClassification::DependencyInstall {
            package_manager: "pip".into(),
            packages: vec!["formulas".into()],
            source: None,
        };
        assert_eq!(
            PolicyEvaluator::evaluate_command_risk(
                &auto_policy,
                &install,
                "work_run_command",
                "pip"
            ),
            WorkPolicyDecision::Ask
        );

        let destructive = CommandRiskClassification::Destructive {
            reason: "rm -rf".into(),
        };
        assert_eq!(
            PolicyEvaluator::evaluate_command_risk(
                &auto_policy,
                &destructive,
                "work_run_command",
                "rm"
            ),
            WorkPolicyDecision::Ask
        );
    }

    #[test]
    fn classifies_git_read_only_commands_with_global_options_as_workspace_read() {
        let cmd1 = PolicyEvaluator::classify_command_risk(
            "git",
            &[
                "-C".into(),
                "/workspace".into(),
                "log".into(),
                "--oneline".into(),
                "-5".into(),
            ],
            "scratch",
            None,
        );
        assert_eq!(cmd1, CommandRiskClassification::WorkspaceRead);

        let cmd2 = PolicyEvaluator::classify_command_risk(
            "git",
            &[
                "--no-pager".into(),
                "-C".into(),
                "/workspace".into(),
                "show".into(),
                "HEAD".into(),
            ],
            "scratch",
            None,
        );
        assert_eq!(cmd2, CommandRiskClassification::WorkspaceRead);

        let cmd3 = PolicyEvaluator::classify_command_risk(
            "git",
            &["status".into(), "-s".into()],
            "scratch",
            None,
        );
        assert_eq!(cmd3, CommandRiskClassification::WorkspaceRead);

        let cmd4 = PolicyEvaluator::classify_command_risk(
            "git",
            &["diff".into(), "HEAD~1".into()],
            "scratch",
            None,
        );
        assert_eq!(cmd4, CommandRiskClassification::WorkspaceRead);

        let cmd5 = PolicyEvaluator::classify_command_risk(
            "git",
            &["rev-parse".into(), "HEAD".into()],
            "scratch",
            None,
        );
        assert_eq!(cmd5, CommandRiskClassification::WorkspaceRead);

        let cmd_push_force = PolicyEvaluator::classify_command_risk(
            "git",
            &[
                "push".into(),
                "origin".into(),
                "main".into(),
                "--force".into(),
            ],
            "scratch",
            None,
        );
        assert!(matches!(
            cmd_push_force,
            CommandRiskClassification::Destructive { .. }
        ));

        let cmd_which =
            PolicyEvaluator::classify_command_risk("which", &["node".into()], "scratch", None);
        assert_eq!(cmd_which, CommandRiskClassification::WorkspaceRead);

        let cmd_pwd = PolicyEvaluator::classify_command_risk("pwd", &[], "scratch", None);
        assert_eq!(cmd_pwd, CommandRiskClassification::WorkspaceRead);
    }
}
