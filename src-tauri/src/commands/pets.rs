use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

const MAX_PET_ZIP_BYTES: usize = 50 * 1024 * 1024;
const MAX_PET_ANIMATION_BYTES: usize = 8 * 1024 * 1024;
const PET_ID_PREFIX: &str = "custom:";
const REQUIRED_ANIMATIONS: [&str; 5] = ["idle", "running", "waiting", "failed", "review"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomPetAnimationFiles {
    pub idle: String,
    pub running: String,
    pub waiting: String,
    pub failed: String,
    pub review: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomPetManifest {
    pub id: String,
    pub display_name: String,
    pub description: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub animations: Option<CustomPetAnimationFiles>,
    #[serde(default)]
    pub single_frame: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCustomPetInput {
    pub slug: String,
    pub display_name: String,
    pub description: String,
    pub zip_base64: Option<String>,
    pub image_base64: Option<String>,
    pub image_name: Option<String>,
}

fn pets_dir() -> PathBuf {
    crate::storage::data_dir().join("pets")
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 73
        && slug
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
}

fn animation_mime(file_name: &str) -> Option<&'static str> {
    match Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("webp") => Some("image/webp"),
        Some("png") => Some("image/png"),
        Some("gif") => Some("image/gif"),
        Some("apng") => Some("image/apng"),
        _ => None,
    }
}

fn animation_stem(file_name: &str) -> Option<&'static str> {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())?
        .to_ascii_lowercase();
    REQUIRED_ANIMATIONS
        .iter()
        .copied()
        .find(|required| *required == stem)
}

fn manifest_path(dir: &Path) -> PathBuf {
    dir.join("pet.json")
}

fn read_manifest(dir: &Path) -> Result<CustomPetManifest, String> {
    let contents = fs::read_to_string(manifest_path(dir)).map_err(|e| e.to_string())?;
    let manifest: CustomPetManifest = serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    if !manifest.id.starts_with(PET_ID_PREFIX) {
        return Err("Invalid custom pet id".to_string());
    }
    Ok(manifest)
}

fn data_url(file_name: &str, dir: &Path) -> Result<String, String> {
    let bytes = fs::read(dir.join(file_name)).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_PET_ANIMATION_BYTES {
        return Err("Custom pet animation is too large".to_string());
    }
    let mime =
        animation_mime(file_name).ok_or_else(|| "Unsupported pet animation format".to_string())?;
    Ok(format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomPetAnimationUrls {
    pub idle: String,
    pub running: String,
    pub waiting: String,
    pub failed: String,
    pub review: String,
}

fn animation_urls(
    manifest: &CustomPetManifest,
    dir: &Path,
) -> Result<CustomPetAnimationUrls, String> {
    if let Some(animations) = &manifest.animations {
        return Ok(CustomPetAnimationUrls {
            idle: data_url(&animations.idle, dir)?,
            running: data_url(&animations.running, dir)?,
            waiting: data_url(&animations.waiting, dir)?,
            failed: data_url(&animations.failed, dir)?,
            review: data_url(&animations.review, dir)?,
        });
    }

    if let Some(image) = &manifest.image {
        let url = data_url(image, dir)?;
        return Ok(CustomPetAnimationUrls {
            idle: url.clone(),
            running: url.clone(),
            waiting: url.clone(),
            failed: url.clone(),
            review: url,
        });
    }

    Err("Custom pet has no image or animations".to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomPetOption {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub animation_urls: CustomPetAnimationUrls,
    pub single_frame: bool,
    pub custom: bool,
}

fn to_option(manifest: CustomPetManifest, dir: &Path) -> Result<CustomPetOption, String> {
    Ok(CustomPetOption {
        id: manifest.id.clone(),
        display_name: manifest.display_name.clone(),
        description: manifest.description.clone(),
        animation_urls: animation_urls(&manifest, dir)?,
        single_frame: manifest.single_frame,
        custom: true,
    })
}

type ArchiveAnimations = BTreeMap<String, (String, Vec<u8>)>;

fn read_animation_archive(zip_base64: &str) -> Result<ArchiveAnimations, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(zip_base64)
        .map_err(|_| "Invalid ZIP data".to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_PET_ZIP_BYTES {
        return Err("Pet ZIP must be between 1 byte and 50 MB".to_string());
    }

    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|_| "Invalid pet ZIP".to_string())?;
    let mut animations = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| "Could not read pet ZIP".to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let file_name = Path::new(&name)
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "Pet ZIP contains an invalid filename".to_string())?;
        let Some(stem) = animation_stem(file_name) else {
            continue;
        };
        if animation_mime(file_name).is_none() {
            return Err(format!("{file_name} must be WebP, PNG, GIF, or APNG"));
        }
        if animations.contains_key(stem) {
            return Err(format!("Pet ZIP contains duplicate animation: {stem}"));
        }
        let mut content = Vec::new();
        entry
            .read_to_end(&mut content)
            .map_err(|_| "Could not read pet animation".to_string())?;
        if content.is_empty() || content.len() > MAX_PET_ANIMATION_BYTES {
            return Err(format!("{file_name} must be between 1 byte and 8 MB"));
        }
        animations.insert(stem.to_string(), (file_name.to_string(), content));
    }

    for required in REQUIRED_ANIMATIONS {
        if !animations.contains_key(required) {
            return Err(format!("Pet ZIP is missing {required}.webp"));
        }
    }
    Ok(animations)
}

fn animation_manifest(animations: &ArchiveAnimations) -> CustomPetAnimationFiles {
    CustomPetAnimationFiles {
        idle: animations["idle"].0.clone(),
        running: animations["running"].0.clone(),
        waiting: animations["waiting"].0.clone(),
        failed: animations["failed"].0.clone(),
        review: animations["review"].0.clone(),
    }
}

fn read_single_image(image_base64: &str, image_name: &str) -> Result<(String, Vec<u8>), String> {
    let extension = Path::new(image_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .ok_or_else(|| "Image file must be PNG, WebP, GIF, or APNG".to_string())?;
    let source_name = format!("pet.{extension}");
    if animation_mime(&source_name).is_none() {
        return Err("Image file must be PNG, WebP, GIF, or APNG".to_string());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(image_base64)
        .map_err(|_| "Invalid image data".to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_PET_ANIMATION_BYTES {
        return Err("Pet image must be between 1 byte and 8 MB".to_string());
    }
    Ok((source_name, bytes))
}

#[tauri::command]
pub fn inspect_custom_pet_zip(zip_base64: String) -> Result<CustomPetAnimationUrls, String> {
    let animations = read_animation_archive(&zip_base64)?;
    let data = |mode: &str| {
        let file_name = &animations[mode].0;
        let mime = animation_mime(file_name).expect("validated animation format");
        format!(
            "data:{};base64,{}",
            mime,
            base64::engine::general_purpose::STANDARD.encode(&animations[mode].1)
        )
    };
    Ok(CustomPetAnimationUrls {
        idle: data("idle"),
        running: data("running"),
        waiting: data("waiting"),
        failed: data("failed"),
        review: data("review"),
    })
}

#[tauri::command]
pub fn inspect_custom_pet_image(
    image_base64: String,
    image_name: String,
) -> Result<String, String> {
    let (file_name, bytes) = read_single_image(&image_base64, &image_name)?;
    let mime = animation_mime(&file_name).expect("validated image format");
    Ok(format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub fn get_custom_pet(id: String) -> Result<CustomPetOption, String> {
    let slug = id
        .strip_prefix(PET_ID_PREFIX)
        .ok_or_else(|| "Not a custom pet id".to_string())?;
    if !valid_slug(slug) {
        return Err("Invalid custom pet id".to_string());
    }
    let dir = pets_dir().join(slug);
    let manifest = read_manifest(&dir)?;
    if manifest.id != id {
        return Err("Custom pet id does not match its package".to_string());
    }
    to_option(manifest, &dir)
}

#[tauri::command]
pub fn list_custom_pets() -> Result<Vec<CustomPetOption>, String> {
    let root = pets_dir();
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut pets = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let manifest = match read_manifest(&dir) {
            Ok(value) => value,
            Err(error) => {
                log::warn!("[pets] skipping invalid custom pet {:?}: {}", dir, error);
                continue;
            }
        };
        match to_option(manifest, &dir) {
            Ok(pet) => pets.push(pet),
            Err(error) => log::warn!("[pets] skipping custom pet {:?}: {}", dir, error),
        }
    }
    pets.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
    });
    Ok(pets)
}

#[tauri::command]
pub fn create_custom_pet(input: CreateCustomPetInput) -> Result<CustomPetOption, String> {
    let slug = input.slug.trim();
    let display_name = input.display_name.trim();
    let description = input.description.trim();
    if !valid_slug(slug) {
        return Err("Pet id must use lowercase letters, numbers, and hyphens".to_string());
    }
    if display_name.is_empty() || display_name.len() > 80 {
        return Err("Pet name must be between 1 and 80 characters".to_string());
    }
    if description.is_empty() || description.len() > 500 {
        return Err("Pet description must be between 1 and 500 characters".to_string());
    }
    let has_zip = input
        .zip_base64
        .as_deref()
        .is_some_and(|value| !value.is_empty());
    let has_image = input
        .image_base64
        .as_deref()
        .is_some_and(|value| !value.is_empty());
    if has_zip == has_image {
        return Err("Choose exactly one image or ZIP source".to_string());
    }

    let image = if has_image {
        Some(read_single_image(
            input.image_base64.as_deref().unwrap_or_default(),
            input.image_name.as_deref().unwrap_or("pet.webp"),
        )?)
    } else {
        None
    };
    let animations = if has_zip {
        Some(read_animation_archive(
            input.zip_base64.as_deref().unwrap_or_default(),
        )?)
    } else {
        None
    };

    let root = pets_dir();
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let dir = root.join(slug);
    if dir.exists() {
        return Err("A custom pet with this id already exists".to_string());
    }
    fs::create_dir(&dir).map_err(|e| e.to_string())?;

    let manifest = CustomPetManifest {
        id: format!("{}{}", PET_ID_PREFIX, slug),
        display_name: display_name.to_string(),
        description: description.to_string(),
        image: image.as_ref().map(|value| value.0.clone()),
        animations: animations.as_ref().map(animation_manifest),
        single_frame: image.is_some(),
    };

    let result = (|| {
        if let Some((file_name, content)) = &image {
            fs::write(dir.join(file_name), content).map_err(|e| e.to_string())?;
        }
        if let Some(animations) = &animations {
            for (file_name, content) in animations.values() {
                fs::write(dir.join(file_name), content).map_err(|e| e.to_string())?;
            }
        }
        let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
        fs::write(manifest_path(&dir), format!("{}\n", json)).map_err(|e| e.to_string())?;
        to_option(manifest, &dir)
    })();

    if result.is_err() {
        let _ = fs::remove_dir_all(&dir);
    }
    result
}

#[tauri::command]
pub fn open_custom_pets_folder() -> Result<(), String> {
    let root = pets_dir();
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    crate::commands::runs::reveal_in_finder(root.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::{animation_mime, animation_stem, valid_slug};

    #[test]
    fn accepts_safe_pet_slugs_only() {
        assert!(valid_slug("moon-cat"));
        assert!(valid_slug("pet123"));
        assert!(!valid_slug("Moon-Cat"));
        assert!(!valid_slug("../secret"));
        assert!(!valid_slug("-cat"));
        assert!(!valid_slug("cat--toy"));
    }

    #[test]
    fn accepts_fixed_animation_names_and_formats() {
        assert_eq!(animation_stem("pet/idle.webp"), Some("idle"));
        assert_eq!(animation_stem("running.png"), Some("running"));
        assert_eq!(animation_stem("review.gif"), Some("review"));
        assert_eq!(animation_stem("other.webp"), None);
        assert_eq!(animation_mime("idle.webp"), Some("image/webp"));
        assert_eq!(animation_mime("idle.apng"), Some("image/apng"));
        assert_eq!(animation_mime("idle.jpg"), None);
    }
}
