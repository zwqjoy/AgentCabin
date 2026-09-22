use crate::agent::session_actor::AttachmentData;
use serde_json::{json, Value};

/// Convert AgentCabin attachments to ACP prompt content.
///
/// ACP's image prompt block is base64-backed. Documents and arbitrary files
/// are intentionally rejected because Grok's live prompt capability only
/// advertises `image`, not a general file input primitive.
pub(super) fn prompt_content(
    text: &str,
    attachments: &[AttachmentData],
) -> Result<Vec<Value>, String> {
    let mut content = vec![json!({"type": "text", "text": text})];
    for attachment in attachments {
        if !attachment.media_type.starts_with("image/") {
            return Err(format!(
                "Grok ACP image prompts do not support '{}' attachments",
                attachment.media_type
            ));
        }
        if attachment.content_base64.trim().is_empty() {
            return Err(format!(
                "Grok ACP image attachment '{}' has no data",
                attachment.filename
            ));
        }
        content.push(json!({
            "type": "image",
            "data": attachment.content_base64,
            "mimeType": attachment.media_type,
        }));
    }
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(media_type: &str) -> AttachmentData {
        AttachmentData {
            content_base64: "AAAA".to_string(),
            media_type: media_type.to_string(),
            filename: "asset".to_string(),
        }
    }

    #[test]
    fn builds_text_and_image_blocks() {
        let content = prompt_content("look", &[attachment("image/png")]).expect("content");
        assert_eq!(content[0], json!({"type": "text", "text": "look"}));
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["mimeType"], "image/png");
        assert_eq!(content[1]["data"], "AAAA");
    }

    #[test]
    fn rejects_documents_and_empty_images() {
        let error = prompt_content("look", &[attachment("application/pdf")]).unwrap_err();
        assert!(error.contains("do not support"));
        let mut empty = attachment("image/png");
        empty.content_base64.clear();
        let error = prompt_content("look", &[empty]).unwrap_err();
        assert!(error.contains("has no data"));
    }
}
