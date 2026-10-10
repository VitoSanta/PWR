//! Opt-in constraints for one caller-numbered conversation turn. Never grants authority.
use pwr_domain::{ToolCall, ToolCatalog};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionContract {
    pub turn: u32,
    pub expires_unix_ms: u64,
    pub read_paths: Vec<String>,
    pub replace_paths: Vec<String>,
    pub command: Option<ExactCommand>,
    pub max_actions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactCommand {
    pub executable: String,
    pub cwd: String,
    pub args: Vec<String>,
}

pub fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| u64::try_from(time.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(u64::MAX)
}

fn relative(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && !path.contains('\\')
        && !path.contains('\0')
}

fn file(root: &Path, path: &str) -> Result<(), String> {
    if !relative(path) {
        return Err("file path must be a normal workspace-relative path".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let joined = root.join(path);
    let target = joined.canonicalize().map_err(|e| e.to_string())?;
    if target != joined {
        return Err("contract file paths cannot traverse symlinks".into());
    }
    if !target.starts_with(&root) || !target.is_file() {
        return Err("file path leaves the workspace or is not an existing file".into());
    }
    Ok(())
}

impl ActionContract {
    pub fn validate(&self, root: &Path, turn: u32) -> Result<(), String> {
        self.active(turn)?;
        if self.expires_unix_ms.saturating_sub(unix_ms()) > 1_800_000 {
            return Err("contract expiry exceeds thirty minutes".into());
        }
        if !(1..=32).contains(&self.max_actions) {
            return Err("maxActions must be 1..32".into());
        }
        for path in self.read_paths.iter().chain(&self.replace_paths) {
            file(root, path)?;
        }
        if self
            .replace_paths
            .iter()
            .any(|path| !self.read_paths.contains(path))
        {
            return Err("replacePaths must also be readable".into());
        }
        if let Some(command) = &self.command {
            if command.executable.is_empty()
                || command.executable.contains('\0')
                || command.cwd != "."
            {
                return Err("command requires an executable and root cwd '.'".into());
            }
            if !self.read_paths.is_empty()
                || !self.replace_paths.is_empty()
                || self.max_actions != 1
            {
                return Err(
                    "a command contract permits exactly one command and no file tools".into(),
                );
            }
        } else if self.read_paths.is_empty() {
            return Err("empty contract".into());
        }
        Ok(())
    }

    pub fn active(&self, turn: u32) -> Result<(), String> {
        if self.turn != turn {
            return Err("contract belongs to another turn".into());
        }
        if unix_ms() >= self.expires_unix_ms {
            return Err("contract expired".into());
        }
        Ok(())
    }

    /// Check the original wire call, before coercions and special tool dispatch.
    pub fn check(
        &self,
        root: &Path,
        turn: u32,
        used: usize,
        call: &ToolCall,
    ) -> Result<(), String> {
        self.active(turn)?;
        if used >= self.max_actions {
            return Err("contract action allowance exhausted".into());
        }
        let arguments = call
            .arguments
            .as_object()
            .ok_or("arguments must be an object")?;
        match call.name.as_str() {
            "run_command" => {
                let command = self
                    .command
                    .as_ref()
                    .ok_or("commands are outside this phase")?;
                if arguments.keys().any(|key| {
                    !["executable", "cwd", "args", "stdin", "outside_sandbox"]
                        .contains(&key.as_str())
                }) || arguments.get("executable") != Some(&serde_json::json!(command.executable))
                    || arguments.get("cwd") != Some(&serde_json::json!(command.cwd))
                    || arguments.get("args") != Some(&serde_json::json!(command.args))
                    || arguments.get("stdin").is_some_and(|v| !v.is_null())
                    || arguments
                        .get("outside_sandbox")
                        .is_some_and(|v| v != &serde_json::json!(false))
                {
                    return Err("command must match executable/cwd/argv exactly, with null or absent stdin and no privilege override".into());
                }
            }
            "read_file" | "replace_text" => {
                let allowed = if call.name == "read_file" {
                    &self.read_paths
                } else {
                    &self.replace_paths
                };
                let path = arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or("missing file path")?;
                if !allowed.iter().any(|allowed| allowed == path) {
                    return Err("file is outside this phase".into());
                }
                file(root, path)?;
                let keys = if call.name == "read_file" {
                    vec!["path", "first_line", "max_lines"]
                } else {
                    vec!["path", "expected_hash", "find", "replace"]
                };
                if arguments.keys().any(|key| !keys.contains(&key.as_str())) {
                    return Err("unexpected file tool field".into());
                }
                let mut typed = call.arguments.clone();
                typed["capability"] = serde_json::json!(call.name);
                // Deserialize the original input, without the ordinary decoder's coercions.
                let _: pwr_tools::ActionProposal =
                    serde_json::from_value(typed).map_err(|e| e.to_string())?;
                crate::action_from_tool_call(call).map_err(|e| e.problem)?;
            }
            _ => return Err("capability is outside this phase".into()),
        }
        Ok(())
    }

    pub fn catalog(&self, catalog: ToolCatalog) -> ToolCatalog {
        let mut tools = catalog
            .tools
            .into_iter()
            .filter(|tool| match tool.name.as_str() {
                "run_command" => self.command.is_some(),
                "read_file" => !self.read_paths.is_empty(),
                "replace_text" => !self.replace_paths.is_empty(),
                _ => false,
            })
            .collect::<Vec<_>>();
        for tool in &mut tools {
            tool.description.push_str(&format!(
                " One-turn action contract: {}",
                serde_json::to_string(self).unwrap_or_default()
            ));
        }
        ToolCatalog::new(tools).expect("a restricted catalogue is valid")
    }
}

/// No timer when the caller supplied no contract.
pub async fn expired(contract: Option<&ActionContract>) {
    match contract {
        Some(contract) => {
            tokio::time::sleep(std::time::Duration::from_millis(
                contract.expires_unix_ms.saturating_sub(unix_ms()),
            ))
            .await
        }
        None => std::future::pending::<()>().await,
    }
}
