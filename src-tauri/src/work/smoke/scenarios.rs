use std::fs;
use std::path::{Path, PathBuf};

pub const CORE_CONTRACT_SCENARIO_ID: &str = "core-contract-approval";
pub const REQUIRED_ARTIFACT_PATH: &str = "output/archive_manifest.json";

pub struct ContractFixture {
    pub root_dir: PathBuf,
    pub contracts_dir: PathBuf,
}

impl ContractFixture {
    pub fn setup(parent_dir: Option<&Path>) -> Result<Self, String> {
        let root = match parent_dir {
            Some(dir) => dir.to_path_buf(),
            None => {
                let temp_base = std::env::temp_dir();
                let dir_name = format!("agentcabin-smoke-fixture-{}", uuid::Uuid::new_v4());
                temp_base.join(dir_name)
            }
        };

        let contracts_dir = root.join("contracts");
        fs::create_dir_all(&contracts_dir).map_err(|e| {
            format!(
                "Failed to create fixture directory {}: {e}",
                contracts_dir.display()
            )
        })?;

        fs::write(
            contracts_dir.join("A-001.txt"),
            "Customer: A\nContractId: A-001\nAmount: 1000\n",
        )
        .map_err(|e| format!("Failed to write A-001.txt: {e}"))?;

        fs::write(
            contracts_dir.join("B-001.txt"),
            "Customer: B\nContractId: B-001\nAmount: 2000\n",
        )
        .map_err(|e| format!("Failed to write B-001.txt: {e}"))?;

        fs::write(
            contracts_dir.join("B-002.txt"),
            "Customer: B\nContractId: B-002\nAmount: 3000\n",
        )
        .map_err(|e| format!("Failed to write B-002.txt: {e}"))?;

        let canonical_contracts_dir =
            fs::canonicalize(&contracts_dir).unwrap_or_else(|_| contracts_dir.clone());

        Ok(Self {
            root_dir: root,
            contracts_dir: canonical_contracts_dir,
        })
    }

    pub fn cleanup(&self) {
        if self.root_dir.exists() {
            let _ = fs::remove_dir_all(&self.root_dir);
        }
    }
}

pub fn generate_smoke_workspace_name() -> String {
    format!("smoke-core-contract-{}", uuid::Uuid::new_v4())
}

pub fn generate_core_contract_prompt(contracts_dir: &Path) -> String {
    let path_str = contracts_dir.to_string_lossy();
    format!(
        "请处理以下合同目录：\n\n\
        {path_str}\n\n\
        1. 首先请求访问该外部合同目录，并读取其中全部合同内容；\n\
        2. 使用 work_write_file 工具将合同按 Customer 字段归档到当前 Work Workspace：\n\
           output/archive/A/\n\
           output/archive/B/\n\
           原文件名保持不变；\n\
        3. 使用 work_write_file 工具生成最终清单文件 output/archive_manifest.json，格式：\n\n\
        {{\n  \
          \"totalArchived\": 3,\n  \
          \"customers\": {{\n    \
            \"A\": 1,\n    \
            \"B\": 2\n  \
          }},\n  \
          \"files\": [\n    \
            \"output/archive/A/A-001.txt\",\n    \
            \"output/archive/B/B-001.txt\",\n    \
            \"output/archive/B/B-002.txt\"\n  \
          ]\n\
        }}\n\n\
        4. 清单生成完成后，必须调用 work_register_artifact 将 output/archive_manifest.json 登记为必需交付物。\n\n\
        全部归档和清单生成请直接使用工作区文件工具完成，无需通过命令行执行比较。\n\
        不得访问上述合同目录之外的任何外部目录。"
    )
}
