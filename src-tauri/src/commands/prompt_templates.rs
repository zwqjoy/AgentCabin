use crate::models::PluginOperationResult;
use crate::storage::prompt_templates::PromptTemplate;

#[tauri::command]
pub fn list_prompt_templates() -> Result<Vec<PromptTemplate>, String> {
    log::debug!("[prompt_templates] list");
    crate::storage::prompt_templates::list_prompt_templates()
}

#[tauri::command]
pub fn create_prompt_template(
    name: String,
    description: Option<String>,
    content: String,
) -> Result<PromptTemplate, String> {
    log::debug!("[prompt_templates] create: name={}", name);
    crate::storage::prompt_templates::create_prompt_template(
        &name,
        description.as_deref(),
        &content,
    )
}

#[tauri::command]
pub fn update_prompt_template(
    id: String,
    name: String,
    description: Option<String>,
    content: String,
) -> Result<PluginOperationResult, String> {
    log::debug!("[prompt_templates] update: id={}", id);
    crate::storage::prompt_templates::update_prompt_template(
        &id,
        &name,
        description.as_deref(),
        &content,
    )
}

#[tauri::command]
pub fn delete_prompt_template(id: String) -> Result<PluginOperationResult, String> {
    log::debug!("[prompt_templates] delete: id={}", id);
    crate::storage::prompt_templates::delete_prompt_template(&id)
}
