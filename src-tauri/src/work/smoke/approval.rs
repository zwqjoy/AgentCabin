use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::work::models::{InboxItem, InboxItemType};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UnexpectedApproval {
    pub item_id: String,
    pub item_type: String,
    pub tool_name: Option<String>,
    pub requested_path: Option<String>,
    pub writable: Option<bool>,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct SmokeApprovalPolicy {
    pub allowed_contracts_dir: PathBuf,
}

impl SmokeApprovalPolicy {
    pub fn new(allowed_contracts_dir: PathBuf) -> Self {
        let canonical = fs::canonicalize(&allowed_contracts_dir)
            .unwrap_or_else(|_| allowed_contracts_dir.clone());
        Self {
            allowed_contracts_dir: canonical,
        }
    }

    pub fn evaluate_inbox_item(&self, item: &InboxItem) -> Result<(), UnexpectedApproval> {
        let item_type_str = format!("{:?}", item.item_type);

        if item.item_type != InboxItemType::AccessRootRequest {
            return Err(UnexpectedApproval {
                item_id: item.id.clone(),
                item_type: item_type_str,
                tool_name: item.payload.tool_name.clone(),
                requested_path: None,
                writable: None,
                reason: format!(
                    "Non-AccessRootRequest inbox item received: {:?}",
                    item.item_type
                ),
            });
        }

        // Extract path and writable status from parameters
        let params = item.payload.parameters.as_ref();
        let requested_path = params
            .and_then(|p| p.get("path"))
            .and_then(|v| v.as_str())
            .or_else(|| {
                // fallback if in payload root
                params
                    .and_then(|p| p.get("directory"))
                    .and_then(|v| v.as_str())
            });

        let writable = params
            .and_then(|p| p.get("writable"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let Some(req_path) = requested_path else {
            return Err(UnexpectedApproval {
                item_id: item.id.clone(),
                item_type: item_type_str,
                tool_name: item.payload.tool_name.clone(),
                requested_path: None,
                writable: Some(writable),
                reason: "AccessRootRequest missing path parameter".to_string(),
            });
        };

        if writable {
            return Err(UnexpectedApproval {
                item_id: item.id.clone(),
                item_type: item_type_str,
                tool_name: item.payload.tool_name.clone(),
                requested_path: Some(req_path.to_string()),
                writable: Some(true),
                reason: "AccessRootRequest requested writable=true, only read access is permitted"
                    .to_string(),
            });
        }

        let canonical_requested =
            fs::canonicalize(Path::new(req_path)).unwrap_or_else(|_| PathBuf::from(req_path));

        if canonical_requested != self.allowed_contracts_dir {
            return Err(UnexpectedApproval {
                item_id: item.id.clone(),
                item_type: item_type_str,
                tool_name: item.payload.tool_name.clone(),
                requested_path: Some(req_path.to_string()),
                writable: Some(writable),
                reason: format!(
                    "Path mismatch: requested '{}' ({}) does not match allowed contracts directory '{}'",
                    req_path,
                    canonical_requested.display(),
                    self.allowed_contracts_dir.display()
                ),
            });
        }

        Ok(())
    }
}
