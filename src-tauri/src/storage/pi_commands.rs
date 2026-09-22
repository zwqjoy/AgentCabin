//! Discover Pi slash commands without starting a Pi session.
//!
//! Pi extensions register commands by executing JavaScript/TypeScript inside the
//! Pi process.  That is the authoritative runtime source, but starting a
//! process just to populate a picker is wasteful.  This module reads the same
//! configured extension entry points and extracts the statically declared
//! `registerCommand(...)` calls.  Runtime discovery still wins when a session
//! is available, so dynamically generated commands remain supported.

use crate::models::CliCommand;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const MAX_EXTENSION_SOURCE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy)]
enum PackageScope {
    User,
    Project,
}

#[derive(Default)]
struct CommandCollector {
    commands: Vec<CliCommand>,
    by_name: HashMap<String, usize>,
    scanned_paths: HashSet<PathBuf>,
}

impl CommandCollector {
    fn add(&mut self, command: CliCommand) {
        let key = command.name.to_lowercase();
        if let Some(index) = self.by_name.get(&key).copied() {
            // Project entries are scanned after user entries and should win.
            // Within one source, keep the first useful description when the
            // second registration is only a duplicate import/re-export.
            if !command.description.is_empty() || self.commands[index].description.is_empty() {
                self.commands[index] = command;
            }
            return;
        }
        self.by_name.insert(key, self.commands.len());
        self.commands.push(command);
    }

    fn scan_source(&mut self, path: &Path, fallback_description: &str) {
        let path = match path.canonicalize() {
            Ok(path) => path,
            Err(_) => return,
        };
        if !self.scanned_paths.insert(path.clone()) {
            return;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(content) if content.len() <= MAX_EXTENSION_SOURCE_BYTES => content,
            _ => return,
        };
        for import in collect_relative_imports(&content) {
            if let Some(import_path) = resolve_import_path(&path, &import) {
                self.scan_source(&import_path, fallback_description);
            }
        }
        let constants = collect_string_constants(&content);
        let mut offset = 0;
        while let Some(relative) = content[offset..].find("registerCommand(") {
            let call_start = offset + relative;
            let args_start = call_start + "registerCommand(".len();
            let name_start = skip_whitespace(&content, args_start);
            let (name, name_end) = if content
                .as_bytes()
                .get(name_start)
                .is_some_and(|b| *b == b'\'' || *b == b'"' || *b == b'`')
            {
                match parse_quoted_string(&content, name_start) {
                    Some((name, end)) => (name, end),
                    None => {
                        offset = args_start;
                        continue;
                    }
                }
            } else {
                let (identifier, end) = parse_identifier(&content, name_start);
                let name = match constants.get(&identifier) {
                    Some(name) => name.clone(),
                    None => {
                        offset = args_start;
                        continue;
                    }
                };
                (name, end)
            };

            if !is_valid_command_name(&name) {
                offset = name_end;
                continue;
            }

            // A command registration's options object is normally a few dozen
            // lines. Keep this bounded so a malformed extension cannot make the
            // cold-start scan expensive.
            let next_registration = content[name_end..]
                .find("registerCommand(")
                .map(|relative| name_end + relative);
            let segment_end =
                next_registration.unwrap_or_else(|| (name_end + 16 * 1024).min(content.len()));
            let segment = &content[name_end..segment_end];
            let description = parse_property_string(segment, "description")
                .unwrap_or_else(|| fallback_description.to_string());
            self.add(CliCommand {
                name,
                description,
                aliases: Vec::new(),
                extra: HashMap::new(),
            });
            offset = name_end;
        }
    }
}

/// Return statically discoverable Pi commands from global and project resources.
pub fn list_pi_commands(cwd: Option<&str>) -> Vec<CliCommand> {
    let mut collector = CommandCollector::default();

    // /llama is shipped by Pi itself rather than by a configured user package.
    collector.add(command("llama", "Manage llama.cpp router models"));

    if let Ok(agent_dir) = crate::storage::pi_profile_bindings::pi_profile_dir("code") {
        scan_extension_dir(&agent_dir.join("extensions"), &mut collector);
        scan_package_settings(
            &agent_dir.join("settings.json"),
            PackageScope::User,
            cwd,
            &agent_dir,
            &mut collector,
        );
    }

    if let Some(cwd) = cwd.filter(|cwd| !cwd.trim().is_empty()) {
        let cwd_path = Path::new(cwd);
        scan_extension_dir(&cwd_path.join(".pi").join("extensions"), &mut collector);
        scan_package_settings(
            &cwd_path.join(".pi").join("settings.json"),
            PackageScope::Project,
            Some(cwd),
            &PathBuf::from(cwd),
            &mut collector,
        );
    }

    log::debug!(
        "[pi_commands] statically discovered {} commands",
        collector.commands.len()
    );
    collector.commands
}

fn command(name: &str, description: &str) -> CliCommand {
    CliCommand {
        name: name.to_string(),
        description: description.to_string(),
        aliases: Vec::new(),
        extra: HashMap::new(),
    }
}

fn scan_package_settings(
    settings_path: &Path,
    scope: PackageScope,
    cwd: Option<&str>,
    agent_dir: &Path,
    collector: &mut CommandCollector,
) {
    let settings = match read_json::<Value>(settings_path) {
        Some(settings) => settings,
        None => return,
    };
    let packages = match settings.get("packages").and_then(Value::as_array) {
        Some(packages) => packages,
        None => return,
    };

    for entry in packages {
        let (source, enabled) = match entry {
            Value::String(source) => (source.clone(), true),
            Value::Object(object) => {
                let source = match object.get("source").and_then(Value::as_str) {
                    Some(source) => source.to_string(),
                    None => continue,
                };
                let enabled = object
                    .get("extensions")
                    .and_then(Value::as_array)
                    .map(|extensions| !extensions.is_empty())
                    .unwrap_or(true);
                (source, enabled)
            }
            _ => continue,
        };
        if !enabled {
            continue;
        }

        let Some(package_dir) = resolve_package_dir(&source, scope, cwd, agent_dir) else {
            continue;
        };
        scan_package_dir(&package_dir, collector);
    }
}

fn resolve_package_dir(
    source: &str,
    scope: PackageScope,
    cwd: Option<&str>,
    agent_dir: &Path,
) -> Option<PathBuf> {
    let source = source.strip_prefix("npm:").unwrap_or(source).trim();
    if source.is_empty() {
        return None;
    }

    if source.starts_with('.') || source.starts_with('/') || source.contains('\\') {
        let base = match scope {
            PackageScope::Project => Path::new(cwd?).to_path_buf(),
            PackageScope::User => agent_dir.to_path_buf(),
        };
        let path = if Path::new(source).is_absolute() {
            PathBuf::from(source)
        } else {
            base.join(source)
        };
        return path.is_dir().then_some(path);
    }

    let root = match scope {
        PackageScope::Project => Path::new(cwd?).join(".pi").join("npm"),
        PackageScope::User => agent_dir.join("npm"),
    };
    let path = root.join("node_modules").join(source);
    path.is_dir().then_some(path)
}

fn scan_package_dir(package_dir: &Path, collector: &mut CommandCollector) {
    let manifest_path = package_dir.join("package.json");
    let manifest = read_json::<Value>(&manifest_path);
    let fallback_description = manifest
        .as_ref()
        .and_then(|manifest| manifest.get("description"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let extension_paths = manifest
        .as_ref()
        .and_then(|manifest| manifest.get("pi"))
        .and_then(|pi| pi.get("extensions"))
        .and_then(Value::as_array)
        .map(|extensions| {
            extensions
                .iter()
                .filter_map(Value::as_str)
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if extension_paths.is_empty() {
        for candidate in ["index.ts", "index.js", "index.mts", "index.mjs"] {
            let path = package_dir.join(candidate);
            if path.is_file() {
                collector.scan_source(&path, fallback_description);
                return;
            }
        }
        return;
    }

    for relative_path in extension_paths {
        let path = package_dir.join(relative_path);
        if path.is_file() {
            collector.scan_source(&path, fallback_description);
        }
    }
}

fn scan_extension_dir(dir: &Path, collector: &mut CommandCollector) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && is_extension_file(&path) {
            collector.scan_source(&path, "");
            continue;
        }
        if !path.is_dir() {
            continue;
        }

        if path.join("package.json").is_file() {
            scan_package_dir(&path, collector);
            continue;
        }
        for candidate in ["index.ts", "index.js", "index.mts", "index.mjs"] {
            let index = path.join(candidate);
            if index.is_file() {
                collector.scan_source(&index, "");
                break;
            }
        }
    }
}

fn is_extension_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("ts" | "js" | "mts" | "mjs" | "cts" | "cjs")
    )
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn collect_string_constants(source: &str) -> HashMap<String, String> {
    let mut constants = HashMap::new();
    for keyword in ["const ", "let ", "var "] {
        let mut offset = 0;
        while let Some(relative) = source[offset..].find(keyword) {
            let start = offset + relative + keyword.len();
            let name_start = skip_whitespace(source, start);
            let (name, name_end) = parse_identifier(source, name_start);
            if name.is_empty() {
                offset = start;
                continue;
            }
            let equals = source[name_end..]
                .find('=')
                .map(|position| name_end + position);
            let Some(equals) = equals else {
                offset = name_end;
                continue;
            };
            if equals - name_end > 96 {
                offset = name_end;
                continue;
            }
            let value_start = skip_whitespace(source, equals + 1);
            if let Some((value, _)) = parse_quoted_string(source, value_start) {
                constants.insert(name, value);
            }
            offset = name_end;
        }
    }
    constants
}

fn parse_property_string(source: &str, property: &str) -> Option<String> {
    let mut offset = 0;
    while let Some(relative) = source[offset..].find(property) {
        let start = offset + relative;
        let before = start
            .checked_sub(1)
            .and_then(|index| source.as_bytes().get(index));
        if before.is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_') {
            offset = start + property.len();
            continue;
        }
        let colon = source[start + property.len()..]
            .find(':')
            .map(|position| start + property.len() + position)?;
        let value_start = skip_whitespace(source, colon + 1);
        if let Some((value, _)) = parse_quoted_string(source, value_start) {
            return Some(value);
        }
        offset = value_start;
    }
    None
}

fn collect_relative_imports(source: &str) -> Vec<String> {
    let mut imports = Vec::new();
    for marker in ["from", "import"] {
        let mut offset = 0;
        while let Some(relative) = source[offset..].find(marker) {
            let start = offset + relative + marker.len();
            let quote_start = skip_whitespace(source, start);
            let Some((path, end)) = parse_quoted_string(source, quote_start) else {
                offset = start;
                continue;
            };
            if path.starts_with('.') && !imports.contains(&path) {
                imports.push(path);
            }
            offset = end.max(start);
        }
    }
    imports
}

fn resolve_import_path(source_path: &Path, import: &str) -> Option<PathBuf> {
    let raw = source_path.parent()?.join(import);
    let mut candidates = vec![raw.clone()];
    if let Some(extension) = raw.extension().and_then(|extension| extension.to_str()) {
        if matches!(extension, "js" | "mjs" | "cjs" | "ts" | "mts" | "cts") {
            candidates.push(raw.with_extension("ts"));
            candidates.push(raw.with_extension("js"));
        }
    } else {
        for extension in ["ts", "js", "mts", "mjs", "cts", "cjs"] {
            candidates.push(raw.with_extension(extension));
        }
    }
    for candidate in candidates {
        if candidate.is_file() {
            return Some(candidate);
        }
        if candidate.is_dir() {
            for index in ["index.ts", "index.js", "index.mts", "index.mjs"] {
                let index_path = candidate.join(index);
                if index_path.is_file() {
                    return Some(index_path);
                }
            }
        }
    }
    None
}

fn skip_whitespace(source: &str, mut offset: usize) -> usize {
    while let Some(byte) = source.as_bytes().get(offset) {
        if !byte.is_ascii_whitespace() {
            break;
        }
        offset += 1;
    }
    offset
}

fn parse_identifier(source: &str, start: usize) -> (String, usize) {
    let mut end = start;
    while let Some(byte) = source.as_bytes().get(end) {
        if byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'$' {
            end += 1;
        } else {
            break;
        }
    }
    (source[start..end].to_string(), end)
}

fn parse_quoted_string(source: &str, start: usize) -> Option<(String, usize)> {
    let quote = *source.as_bytes().get(start)? as char;
    if !matches!(quote, '\'' | '"' | '`') {
        return None;
    }

    let mut value = String::new();
    let mut offset = start + 1;
    while offset < source.len() {
        let character = source[offset..].chars().next()?;
        let width = character.len_utf8();
        if character == quote {
            return Some((value, offset + width));
        }
        if character == '\\' {
            offset += width;
            let escaped = source[offset..].chars().next()?;
            let escaped_width = escaped.len_utf8();
            value.push(match escaped {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            offset += escaped_width;
        } else {
            value.push(character);
            offset += width;
        }
    }
    None
}

fn is_valid_command_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.chars().any(char::is_whitespace)
        && !name.contains('/')
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn extracts_literal_and_constant_command_names() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("extension.ts");
        std::fs::write(
            &source,
            r#"
const WEB_TOOLS_COMMAND_NAME = "web-tools";
pi.registerCommand(WEB_TOOLS_COMMAND_NAME, {
  description: "Configure web tools",
});
pi.registerCommand("fff-mode", { description: "Show or set mode" });
"#,
        )
        .unwrap();

        let mut collector = CommandCollector::default();
        collector.scan_source(&source, "");
        let names: Vec<_> = collector
            .commands
            .iter()
            .map(|command| command.name.as_str())
            .collect();
        assert_eq!(names, vec!["web-tools", "fff-mode"]);
        assert_eq!(collector.commands[0].description, "Configure web tools");
    }

    #[test]
    fn scans_configured_npm_package_manifest() {
        let home = tempdir().unwrap();
        let agent_dir = home.path().join("profile");
        let package_dir = agent_dir.join("npm/node_modules/example-package");
        std::fs::create_dir_all(&package_dir).unwrap();
        std::fs::write(
            package_dir.join("package.json"),
            r#"{"description":"Example package","pi":{"extensions":["./src/index.ts"]}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(package_dir.join("src")).unwrap();
        std::fs::write(
            package_dir.join("src/index.ts"),
            r#"import { registerExample } from "./commands.js";
registerExample(pi);"#,
        )
        .unwrap();
        std::fs::write(
            package_dir.join("src/commands.ts"),
            r#"export function registerExample(pi) {
  pi.registerCommand("example", { description: "Example command" });
}"#,
        )
        .unwrap();

        let settings = agent_dir.join("settings.json");
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(&settings, r#"{"packages":["npm:example-package"]}"#).unwrap();

        let mut collector = CommandCollector::default();
        scan_package_settings(
            &settings,
            PackageScope::User,
            None,
            &agent_dir,
            &mut collector,
        );
        assert!(collector
            .commands
            .iter()
            .any(|command| command.name == "example"));
    }
}
