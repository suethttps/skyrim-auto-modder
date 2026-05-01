// ╔════════════════════════════════════════════════════════════════════════════════╗
// ║ SKYRIM AUTO MODDER - TAURI BACKEND (lib.rs)                                  ║
// ║ Complete backend logic for mod manager app                                    ║
// ╚════════════════════════════════════════════════════════════════════════════════╝
//
// FILE ORGANIZATION:
// 1. IMPORTS & CONSTANTS (lines 1-20)
// 2. TYPE DEFINITIONS (lines 22-138)  - Data structures for serialization
// 3. TAURI COMMANDS (lines 140-1211)  - Frontend entry points
// 4. HELPER FUNCTIONS (lines 409-1246) - Internal utilities
// 5. ENTRY POINT (lines 1214-1246)    - App initialization
//
// Find functions by searching: 'fn function_name()' or 'SECTION' marker
// ════════════════════════════════════════════════════════════════════════════════════

// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 1: IMPORTS & CONSTANTS (lines 1-20)
// ════════════════════════════════════════════════════════════════════════════════════
use reqwest::{blocking::Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    env,
    fs,
    io,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;

const SKYRIM_APP_ID: &str = "489830";
const SKYRIM_DIR_NAME: &str = "Skyrim Special Edition";
const NEXUS_GAME_DOMAIN: &str = "skyrimspecialedition";
const NEXUS_API_BASE: &str = "https://api.nexusmods.com/v1";
const APP_NAME: &str = "skyrim-auto-modder";
const APP_VERSION: &str = "0.1.0";
const NEXUS_PROTOCOL_VERSION: &str = "1.0.0";


// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 2: TYPE DEFINITIONS (lines 22-138)
// ════════════════════════════════════════════════════════════════════════════════════
#[derive(Debug, Serialize)]
struct SkyrimInstallation {
    name: String,
    game_dir: String,
    data_dir: Option<String>,
    exe_path: Option<String>,
    skse_loader_path: Option<String>,
    steam_app_manifest: Option<String>,
    valid: bool,
    issues: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SavesLocation {
    name: String,
    path: String,
    exists: bool,
    save_count: usize,
}

#[derive(Debug, Serialize)]
struct ModInstallResult {
    name: String,
    source_url: String,
    archive_path: String,
    staging_dir: String,
    installed_to: String,
    copied_files: usize,
    installed_mod_id: String,
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct InstalledFile {
    path: String,
    existed_before: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct InstalledMod {
    id: String,
    name: String,
    source_url: String,
    archive_path: String,
    staging_dir: String,
    game_dir: String,
    installed_to: String,
    installed_at: u64,
    copied_files: Vec<InstalledFile>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct UninstallResult {
    id: String,
    name: String,
    removed_files: usize,
    skipped_files: usize,
    archive_path: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct InstallLogEntry {
    id: String,
    timestamp: u64,
    action: String,
    url: String,
    ok: bool,
    message: String,
    mod_id: Option<String>,
    mod_name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct NexusConfig {
    api_key: String,
}

#[derive(Debug, Serialize)]
struct NexusAuthStatus {
    configured: bool,
    user_name: Option<String>,
    is_premium: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct NexusValidationResponse {
    name: String,
    is_premium: bool,
}

#[derive(Debug, Deserialize)]
struct NexusFilesResponse {
    files: Vec<NexusFile>,
}

#[derive(Debug, Deserialize)]
struct NexusFile {
    file_id: u64,
    category_name: Option<String>,
    is_primary: Option<bool>,
    uploaded_timestamp: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct NexusDownloadLink {
    #[serde(alias = "URI")]
    uri: String,
}

#[derive(Debug)]
struct NexusResolvedLink {
    mod_id: u64,
    file_id: Option<u64>,
    key: Option<String>,
    expires: Option<String>,
}


// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 3: TAURI COMMANDS - INSTALLATION (lines 141-169)
// Frontend entry points exposed to TypeScript via IPC
// ════════════════════════════════════════════════════════════════════════════════════
#[tauri::command]
// → Function: scan_skyrim_installations()
fn scan_skyrim_installations() -> Result<Vec<SkyrimInstallation>, String> {
    let mut candidates = BTreeSet::new();

    for library in steam_libraries()? {
        candidates.insert(library.join("steamapps").join("common").join(SKYRIM_DIR_NAME));
        candidates.insert(library.join("common").join(SKYRIM_DIR_NAME));
    }

    let installations = candidates
        .into_iter()
        .filter(|path| path.exists())
        .map(|path| inspect_installation(&path))
        .collect();

    Ok(installations)
}

#[tauri::command]
// → Function: validate_skyrim_path()
fn validate_skyrim_path(path: String) -> Result<SkyrimInstallation, String> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err("The selected path does not exist.".to_string());
    }
    if !path.is_dir() {
        return Err("The selected path is not a folder.".to_string());
    }

    Ok(inspect_installation(&path))
}

#[tauri::command]

// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 4: TAURI COMMANDS - NEXUS API (lines 172-206)
// Nexus Mods API authentication and configuration
// ════════════════════════════════════════════════════════════════════════════════════
// → Function: save_nexus_api_key()
fn save_nexus_api_key(api_key: String) -> Result<NexusAuthStatus, String> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err("Paste a Nexus Mods API key first.".to_string());
    }

    let status = validate_nexus_api_key(api_key)?;
    let config_path = nexus_config_path()?;
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Could not create config folder: {err}"))?;
    }

    let config = NexusConfig {
        api_key: api_key.to_string(),
    };
    let json = serde_json::to_string_pretty(&config)
        .map_err(|err| format!("Could not serialize Nexus config: {err}"))?;
    fs::write(&config_path, json).map_err(|err| format!("Could not save Nexus API key: {err}"))?;

    Ok(status)
}

#[tauri::command]
// → Function: get_nexus_auth_status()
fn get_nexus_auth_status() -> Result<NexusAuthStatus, String> {
    let Some(api_key) = load_nexus_api_key()? else {
        return Ok(NexusAuthStatus {
            configured: false,
            user_name: None,
            is_premium: None,
        });
    };

    validate_nexus_api_key(&api_key)
}

#[tauri::command]

// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 5: TAURI COMMANDS - INSTALLED MODS (lines 208-291)
// List and manage installed mods
// ════════════════════════════════════════════════════════════════════════════════════
// → Function: list_installed_mods()
fn list_installed_mods() -> Result<Vec<InstalledMod>, String> {
    let registry_dir = installed_mods_dir()?;
    if !registry_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut mods = fs::read_dir(&registry_dir)
        .map_err(|err| format!("Could not read installed mods registry: {err}"))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|contents| serde_json::from_str::<InstalledMod>(&contents).ok())
        .collect::<Vec<_>>();
    mods.sort_by_key(|item| std::cmp::Reverse(item.installed_at));
    Ok(mods)
}

#[tauri::command]
// → Function: uninstall_mod()
fn uninstall_mod(id: String) -> Result<UninstallResult, String> {
    let manifest_path = installed_mod_manifest_path(&id)?;
    if !manifest_path.is_file() {
        return Err("Installed mod manifest was not found.".to_string());
    }

    let contents = fs::read_to_string(&manifest_path)
        .map_err(|err| format!("Could not read installed mod manifest: {err}"))?;
    let installed = serde_json::from_str::<InstalledMod>(&contents)
        .map_err(|err| format!("Could not parse installed mod manifest: {err}"))?;
    let game_dir = PathBuf::from(&installed.game_dir)
        .canonicalize()
        .map_err(|err| format!("Could not resolve game folder: {err}"))?;

    let mut removed_files = 0;
    let mut skipped_files = 0;

    for file in installed.copied_files.iter().rev() {
        if file.existed_before {
            skipped_files += 1;
            continue;
        }

        let path = PathBuf::from(&file.path);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !canonical.starts_with(&game_dir) {
            skipped_files += 1;
            continue;
        }

        if path.is_file() {
            fs::remove_file(&path).map_err(|err| format!("Could not remove {}: {err}", file.path))?;
            removed_files += 1;
        } else {
            skipped_files += 1;
        }
    }

    fs::remove_file(&manifest_path)
        .map_err(|err| format!("Could not remove installed mod manifest: {err}"))?;

    let result = UninstallResult {
        id: installed.id,
        name: installed.name,
        removed_files,
        skipped_files,
        archive_path: installed.archive_path,
    };
    append_install_log(InstallLogEntry {
        id: format!("uninstall-{}-{}", result.id, timestamp()),
        timestamp: timestamp(),
        action: "uninstall".to_string(),
        url: result.archive_path.clone(),
        ok: true,
        message: format!(
            "Removed {} file{}; preserved {}.",
            result.removed_files,
            if result.removed_files == 1 { "" } else { "s" },
            result.skipped_files
        ),
        mod_id: Some(result.id.clone()),
        mod_name: Some(result.name.clone()),
    })?;

    Ok(result)
}

#[tauri::command]

// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 6: TAURI COMMANDS - LOGS (lines 294-328)
// Installation history management
// ════════════════════════════════════════════════════════════════════════════════════
// → Function: list_install_logs()
fn list_install_logs() -> Result<Vec<InstallLogEntry>, String> {
    let path = install_logs_path()?;
    if !path.is_file() {
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(&path)
        .map_err(|err| format!("Could not read install logs: {err}"))?;
    serde_json::from_str::<Vec<InstallLogEntry>>(&contents)
        .map_err(|err| format!("Could not parse install logs: {err}"))
}

#[tauri::command]
// → Function: clear_install_logs()
fn clear_install_logs() -> Result<(), String> {
    let path = install_logs_path()?;
    if path.is_file() {
        fs::remove_file(path).map_err(|err| format!("Could not clear install logs: {err}"))?;
    }
    Ok(())
}

#[tauri::command]
// → Function: append_install_log_entry()
fn append_install_log_entry(action: String, url: String, ok: bool, message: String) -> Result<(), String> {
    let now = timestamp();
    append_install_log(InstallLogEntry {
        id: format!("manual-{action}-{now}"),
        timestamp: now,
        action,
        url,
        ok,
        message,
        mod_id: None,
        mod_name: None,
    })
}

#[tauri::command]

// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 7: TAURI COMMAND - MOD INSTALLATION (lines 331-407)
// Main workflow: download → extract → validate → install
// ════════════════════════════════════════════════════════════════════════════════════
// → Function: install_mod_from_url()
fn install_mod_from_url(url: String, game_dir: String) -> Result<ModInstallResult, String> {
    let game_dir = PathBuf::from(game_dir);
    let installation = inspect_installation(&game_dir);
    let data_dir = installation
        .data_dir
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| "A valid Skyrim Data folder is required before installing mods.".to_string())?;

    let url = url.trim();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        if !url.starts_with("nxm://") {
            return Err("Only http://, https://, and nxm:// mod links are supported.".to_string());
        }
    }

    ensure_command_exists("bsdtar")?;

    let workspace = app_data_dir()?.join("downloads");
    fs::create_dir_all(&workspace).map_err(|err| format!("Could not create downloads folder: {err}"))?;

    let source_url = resolve_download_source(url)?;
    let filename = filename_from_url(&source_url).unwrap_or_else(|| format!("mod-{}.archive", timestamp()));
    let archive_path = workspace.join(&filename);
    download_file(&source_url, &archive_path)?;

    install_from_archive_file(&archive_path, url, &game_dir, &data_dir, &filename)
}

#[tauri::command]
// → Function: install_mod_from_archive()
fn install_mod_from_archive(path: String, game_dir: String) -> Result<ModInstallResult, String> {
    let game_dir = PathBuf::from(game_dir);
    let installation = inspect_installation(&game_dir);
    let data_dir = installation
        .data_dir
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| "A valid Skyrim Data folder is required before installing mods.".to_string())?;

    let archive_path = PathBuf::from(&path);
    if !archive_path.is_file() {
        return Err(format!("Archive file does not exist: {path}"));
    }

    let extension = archive_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(extension.as_str(), "zip" | "7z" | "rar") {
        return Err("Only .zip, .7z, and .rar archives are supported.".to_string());
    }

    ensure_command_exists("bsdtar")?;

    let filename = archive_path
        .file_name()
        .and_then(|value| value.to_str())
        .map(sanitize_filename)
        .unwrap_or_else(|| format!("mod-{}.archive", timestamp()));

    install_from_archive_file(&archive_path, &path, &game_dir, &data_dir, &filename)
}

// → Function: install_from_archive_file()
fn install_from_archive_file(
    archive_path: &Path,
    source_label: &str,
    game_dir: &Path,
    data_dir: &Path,
    original_filename: &str,
) -> Result<ModInstallResult, String> {
    let mod_name = archive_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("mod")
        .to_string();
    let staging_dir = app_data_dir()?
        .join("staging")
        .join(format!("{}-{}", sanitize_filename(&mod_name), timestamp()));
    fs::create_dir_all(&staging_dir).map_err(|err| format!("Could not create staging folder: {err}"))?;
    extract_archive(archive_path, &staging_dir)?;

    let install_root = detect_install_root(&staging_dir)?;
    let copied_files = install_extracted_mod(&install_root, game_dir, data_dir)
        .map_err(|err| format!("Could not install files: {err}"))?;
    let warnings = detect_install_warnings(&install_root);
    let installed_at = timestamp();
    let installed_mod_id = format!("{}-{installed_at}", sanitize_filename(&mod_name));
    let local_archive_path = copy_archive_to_local_store(archive_path, original_filename)?;
    let installed_mod = InstalledMod {
        id: installed_mod_id.clone(),
        name: mod_name.clone(),
        source_url: source_label.to_string(),
        archive_path: path_to_string(&local_archive_path),
        staging_dir: path_to_string(&staging_dir),
        game_dir: path_to_string(game_dir),
        installed_to: path_to_string(data_dir),
        installed_at,
        copied_files: copied_files.clone(),
        warnings: warnings.clone(),
    };
    save_installed_mod(&installed_mod)?;
    append_install_log(InstallLogEntry {
        id: format!("install-{installed_mod_id}-{installed_at}"),
        timestamp: installed_at,
        action: "install".to_string(),
        url: source_label.to_string(),
        ok: true,
        message: format!(
            "Copied {} file{} into Data.",
            copied_files.len(),
            if copied_files.len() == 1 { "" } else { "s" }
        ),
        mod_id: Some(installed_mod_id.clone()),
        mod_name: Some(mod_name.clone()),
    })?;

    Ok(ModInstallResult {
        name: mod_name,
        source_url: source_label.to_string(),
        archive_path: path_to_string(&local_archive_path),
        staging_dir: path_to_string(&staging_dir),
        installed_to: path_to_string(data_dir),
        copied_files: copied_files.len(),
        installed_mod_id,
        warnings,
    })
}


// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 8: HELPER FUNCTIONS - INSTALLATION (lines 417-455)
// Skyrim installation detection and Steam library scanning
// ════════════════════════════════════════════════════════════════════════════════════
// → Function: resolve_download_source()
fn resolve_download_source(input: &str) -> Result<String, String> {
    if let Some(nexus) = parse_nexus_link(input)? {
        return resolve_nexus_download_url(nexus);
    }

    Ok(input.to_string())
}

// → Function: inspect_installation()
fn inspect_installation(game_dir: &Path) -> SkyrimInstallation {
    let canonical_game_dir = game_dir
        .canonicalize()
        .unwrap_or_else(|_| game_dir.to_path_buf());
    let data_dir = canonical_game_dir.join("Data");
    let exe_path = canonical_game_dir.join("SkyrimSE.exe");
    let skse_loader_path = canonical_game_dir.join("skse64_loader.exe");
    let steam_app_manifest = find_manifest_for_game(&canonical_game_dir);
    let mut issues = Vec::new();

    if !exe_path.is_file() {
        issues.push("SkyrimSE.exe was not found in the game folder.".to_string());
    }

    if !data_dir.is_dir() {
        issues.push("The Data folder was not found.".to_string());
    } else {
        for master in ["Skyrim.esm", "Update.esm"] {
            if !data_dir.join(master).is_file() {
                issues.push(format!("{master} was not found in Data."));
            }
        }
    }

    if !skse_loader_path.is_file() {
        issues.push("SKSE loader is not installed yet. Some mods will require it.".to_string());
    }

    SkyrimInstallation {
        name: SKYRIM_DIR_NAME.to_string(),
        game_dir: path_to_string(&canonical_game_dir),
        data_dir: data_dir.is_dir().then(|| path_to_string(&data_dir)),
        exe_path: exe_path.is_file().then(|| path_to_string(&exe_path)),
        skse_loader_path: skse_loader_path.is_file().then(|| path_to_string(&skse_loader_path)),
        steam_app_manifest: steam_app_manifest.map(|path| path_to_string(&path)),
        valid: issues.iter().all(|issue| issue.contains("SKSE")),
        issues,
    }
}


// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 9: HELPER FUNCTIONS - NEXUS API (lines 457-679)
// Nexus API integration and link parsing
// ════════════════════════════════════════════════════════════════════════════════════
// → Function: validate_nexus_api_key()
fn validate_nexus_api_key(api_key: &str) -> Result<NexusAuthStatus, String> {
    let response: NexusValidationResponse = nexus_client()
        .get(format!("{NEXUS_API_BASE}/users/validate"))
        .header("APIKEY", api_key)
        .send()
        .map_err(|err| format!("Could not contact Nexus Mods: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Nexus API key validation failed: {err}"))?
        .json()
        .map_err(|err| format!("Could not read Nexus validation response: {err}"))?;

    Ok(NexusAuthStatus {
        configured: true,
        user_name: Some(response.name),
        is_premium: Some(response.is_premium),
    })
}

// → Function: resolve_nexus_download_url()
fn resolve_nexus_download_url(link: NexusResolvedLink) -> Result<String, String> {
    let api_key = load_nexus_api_key()?
        .ok_or_else(|| "Save your Nexus Mods API key before installing Nexus links.".to_string())?;
    let file_id = match link.file_id {
        Some(file_id) => file_id,
        None => choose_default_nexus_file(link.mod_id, &api_key)?,
    };
    let has_site_download_token = link.key.is_some() && link.expires.is_some();

    if !has_site_download_token {
        let status = validate_nexus_api_key(&api_key)?;
        if !status.is_premium.unwrap_or(false) {
            return Err(
                "Nexus rejected the direct API download. Non-Premium Nexus accounts must start downloads from the Nexus site and paste the generated nxm:// link here, or use a Premium account for direct page URLs."
                    .to_string(),
            );
        }
    }

    let mut endpoint = format!(
        "{NEXUS_API_BASE}/games/{NEXUS_GAME_DOMAIN}/mods/{}/files/{}/download_link",
        link.mod_id, file_id
    );
    if let (Some(key), Some(expires)) = (link.key.as_ref(), link.expires.as_ref()) {
        endpoint.push_str(&format!("?key={key}&expires={expires}"));
    }

    let links: Vec<NexusDownloadLink> = nexus_client()
        .get(endpoint)
        .header("APIKEY", api_key)
        .send()
        .map_err(|err| format!("Could not contact Nexus Mods: {err}"))?
        .pipe_nexus_download_response()?
        .json()
        .map_err(|err| format!("Could not read Nexus download response: {err}"))?;

    links
        .into_iter()
        .find(|link| !link.uri.trim().is_empty())
        .map(|link| link.uri)
        .ok_or_else(|| "Nexus did not return a download URL for that file.".to_string())
}

trait NexusResponseExt {
// → Function: pipe_nexus_download_response()
    fn pipe_nexus_download_response(self) -> Result<reqwest::blocking::Response, String>;
}

impl NexusResponseExt for reqwest::blocking::Response {
// → Function: pipe_nexus_download_response()
    fn pipe_nexus_download_response(self) -> Result<reqwest::blocking::Response, String> {
        let status = self.status();
        if status.is_success() {
            return Ok(self);
        }

        let body = self
            .text()
            .unwrap_or_else(|_| "Nexus did not return an error body.".to_string());
        let message = nexus_download_error_message(status, body.trim());
        Err(message)
    }
}

// → Function: nexus_download_error_message()
fn nexus_download_error_message(status: StatusCode, body: &str) -> String {
    if status == StatusCode::FORBIDDEN {
        return "Could not get Nexus download link: 403 Forbidden. If this is a non-Premium Nexus account, open the mod on nexusmods.com, click Mod Manager Download, and paste the generated nxm:// link here. Direct Nexus page URLs require Premium API download access.".to_string();
    }

    if body.is_empty() {
        format!("Could not get Nexus download link: HTTP status {status}")
    } else {
        format!("Could not get Nexus download link: HTTP status {status}: {body}")
    }
}

// → Function: choose_default_nexus_file()
fn choose_default_nexus_file(mod_id: u64, api_key: &str) -> Result<u64, String> {
    let response: NexusFilesResponse = nexus_client()
        .get(format!(
            "{NEXUS_API_BASE}/games/{NEXUS_GAME_DOMAIN}/mods/{mod_id}/files"
        ))
        .header("APIKEY", api_key)
        .send()
        .map_err(|err| format!("Could not contact Nexus Mods: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Could not list Nexus mod files: {err}"))?
        .json()
        .map_err(|err| format!("Could not read Nexus files response: {err}"))?;

    let files = response.files;

    files
        .iter()
        .into_iter()
        .filter(|file| {
            file.category_name
                .as_deref()
                .map(|category| category.eq_ignore_ascii_case("MAIN"))
                .unwrap_or(false)
                || file.is_primary.unwrap_or(false)
        })
        .max_by_key(|file| file.uploaded_timestamp.unwrap_or(0))
        .or_else(|| {
            files
                .iter()
                .max_by_key(|file| file.uploaded_timestamp.unwrap_or(0))
        })
        .map(|file| file.file_id)
        .ok_or_else(|| "Nexus did not return any downloadable files for that mod.".to_string())
}

// → Function: parse_nexus_link()
fn parse_nexus_link(input: &str) -> Result<Option<NexusResolvedLink>, String> {
    if input.starts_with("nxm://") {
        return parse_nxm_link(input).map(Some);
    }

    let Ok(url) = Url::parse(input) else {
        return Ok(None);
    };
    let Some(host) = url.host_str() else {
        return Ok(None);
    };
    if !host.ends_with("nexusmods.com") {
        return Ok(None);
    }

    let segments = url.path_segments().map(|segments| segments.collect::<Vec<_>>());
    let Some(segments) = segments else {
        return Ok(None);
    };
    let Some(mods_index) = segments.iter().position(|segment| *segment == "mods") else {
        return Ok(None);
    };
    let Some(mod_id_segment) = segments.get(mods_index + 1) else {
        return Err("Nexus URL does not include a mod id.".to_string());
    };

    let mod_id = mod_id_segment
        .parse::<u64>()
        .map_err(|_| "Nexus URL has an invalid mod id.".to_string())?;
    let file_id = url
        .query_pairs()
        .find(|(key, _)| key == "file_id")
        .and_then(|(_, value)| value.parse::<u64>().ok());

    Ok(Some(NexusResolvedLink {
        mod_id,
        file_id,
        key: None,
        expires: None,
    }))
}

// → Function: parse_nxm_link()
fn parse_nxm_link(input: &str) -> Result<NexusResolvedLink, String> {
    let url = Url::parse(input).map_err(|err| format!("Invalid nxm link: {err}"))?;
    let segments = url.path_segments().map(|segments| segments.collect::<Vec<_>>());
    let Some(segments) = segments else {
        return Err("Invalid nxm link path.".to_string());
    };

    let Some(mods_index) = segments.iter().position(|segment| *segment == "mods") else {
        return Err("nxm link does not include a mod id.".to_string());
    };
    let Some(files_index) = segments.iter().position(|segment| *segment == "files") else {
        return Err("nxm link does not include a file id.".to_string());
    };
    let mod_id = segments
        .get(mods_index + 1)
        .ok_or_else(|| "nxm link does not include a mod id.".to_string())?
        .parse::<u64>()
        .map_err(|_| "nxm link has an invalid mod id.".to_string())?;
    let file_id = segments
        .get(files_index + 1)
        .ok_or_else(|| "nxm link does not include a file id.".to_string())?
        .parse::<u64>()
        .map_err(|_| "nxm link has an invalid file id.".to_string())?;

    let key = url
        .query_pairs()
        .find(|(name, _)| name == "key")
        .map(|(_, value)| value.into_owned());
    let expires = url
        .query_pairs()
        .find(|(name, _)| name == "expires")
        .map(|(_, value)| value.into_owned());

    Ok(NexusResolvedLink {
        mod_id,
        file_id: Some(file_id),
        key,
        expires,
    })
}

// → Function: nexus_client()
fn nexus_client() -> Client {
    Client::builder()
        .user_agent(format!("{APP_NAME}/{APP_VERSION}"))
        .default_headers({
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert("Application-Name", APP_NAME.parse().expect("valid app name"));
            headers.insert("Application-Version", APP_VERSION.parse().expect("valid app version"));
            headers.insert("Protocol-Version", NEXUS_PROTOCOL_VERSION.parse().expect("valid protocol version"));
            headers
        })
        .build()
        .expect("valid reqwest client")
}


// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 10: HELPER FUNCTIONS - ARCHIVE OPERATIONS (lines 681-836)
// Download, extract, and install mod archives
// ════════════════════════════════════════════════════════════════════════════════════
// → Function: ensure_command_exists()
fn ensure_command_exists(command: &str) -> Result<(), String> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {command}"))
        .status()
        .map_err(|err| format!("Could not check for {command}: {err}"))?;

    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("{command} is required to extract mod archives."))
}

// → Function: download_file()
fn download_file(url: &str, destination: &Path) -> Result<(), String> {
    let mut response = reqwest::blocking::Client::builder()
        .user_agent("skyrim-auto-modder/0.1")
        .build()
        .map_err(|err| format!("Could not create downloader: {err}"))?
        .get(url)
        .send()
        .map_err(|err| format!("Download failed: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Download failed: {err}"))?;

    let mut output = fs::File::create(destination)
        .map_err(|err| format!("Could not create archive file: {err}"))?;
    io::copy(&mut response, &mut output).map_err(|err| format!("Could not save download: {err}"))?;

    Ok(())
}

// → Function: extract_archive()
fn extract_archive(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let output = Command::new("bsdtar")
        .arg("-xf")
        .arg(archive_path)
        .arg("-C")
        .arg(destination)
        .output()
        .map_err(|err| format!("Could not start bsdtar: {err}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!("Could not extract archive with bsdtar: {}", stderr.trim()))
}

// → Function: detect_install_root()
fn detect_install_root(staging_dir: &Path) -> Result<PathBuf, String> {
    let data_dir = staging_dir.join("Data");
    if data_dir.is_dir() {
        return Ok(data_dir);
    }

    let entries = directory_entries(staging_dir)?;
    if entries.len() == 1 && entries[0].is_dir() {
        if is_skse_runtime_layout(&entries[0]) {
            return Ok(entries[0].clone());
        }
        let nested_data = entries[0].join("Data");
        if nested_data.is_dir() {
            return Ok(nested_data);
        }
        return Ok(entries[0].clone());
    }

    Ok(staging_dir.to_path_buf())
}

// → Function: directory_entries()
fn directory_entries(path: &Path) -> Result<Vec<PathBuf>, String> {
    fs::read_dir(path)
        .map_err(|err| format!("Could not read extracted archive: {err}"))?
        .map(|entry| entry.map(|entry| entry.path()).map_err(|err| err.to_string()))
        .collect()
}

// → Function: copy_tree()
fn copy_tree(source: &Path, destination: &Path) -> io::Result<Vec<InstalledFile>> {
    let mut copied = Vec::new();
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if source_path.is_dir() {
            fs::create_dir_all(&destination_path)?;
            copied.extend(copy_tree(&source_path, &destination_path)?);
        } else if source_path.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let existed_before = destination_path.exists();
            fs::copy(&source_path, &destination_path)?;
            copied.push(InstalledFile {
                path: path_to_string(&destination_path),
                existed_before,
            });
        }
    }

    Ok(copied)
}

// → Function: install_extracted_mod()
fn install_extracted_mod(install_root: &Path, game_dir: &Path, data_dir: &Path) -> io::Result<Vec<InstalledFile>> {
    if is_skse_runtime_layout(install_root) {
        return install_skse_runtime(install_root, game_dir, data_dir);
    }

    copy_tree(install_root, data_dir)
}

// → Function: is_skse_runtime_layout()
fn is_skse_runtime_layout(install_root: &Path) -> bool {
    install_root.join("skse64_loader.exe").is_file()
        || fs::read_dir(install_root)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .any(|entry| {
                entry.path().is_file()
                    && entry
                        .file_name()
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .starts_with("skse64_")
            })
}

// → Function: install_skse_runtime()
fn install_skse_runtime(install_root: &Path, game_dir: &Path, data_dir: &Path) -> io::Result<Vec<InstalledFile>> {
    let mut copied = Vec::new();
    let bundled_data = install_root.join("Data");
    if bundled_data.is_dir() {
        copied.extend(copy_tree(&bundled_data, data_dir)?);
    }

    for entry in fs::read_dir(install_root)? {
        let entry = entry?;
        let source_path = entry.path();
        let name = entry.file_name();
        let name_text = name.to_string_lossy();

        if source_path.is_file() {
            let destination_path = game_dir.join(&name);
            let existed_before = destination_path.exists();
            fs::copy(&source_path, &destination_path)?;
            copied.push(InstalledFile {
                path: path_to_string(&destination_path),
                existed_before,
            });
        } else if source_path.is_dir() && name_text != "Data" && name_text != "src" {
            let destination = game_dir.join(&name);
            fs::create_dir_all(&destination)?;
            copied.extend(copy_tree(&source_path, &destination)?);
        }
    }

    Ok(copied)
}

// → Function: detect_install_warnings()
fn detect_install_warnings(install_root: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    for folder in ["fomod", "FOMOD"] {
        if install_root.join(folder).is_dir() {
            warnings.push("FOMOD installer detected. This first installer copies the default archive layout and does not evaluate FOMOD choices yet.".to_string());
            break;
        }
    }
    if install_root.join("SKSE").is_dir() {
        warnings.push("SKSE plugin files detected in Data layout.".to_string());
    }
    if is_skse_runtime_layout(install_root) {
        warnings.push("SKSE runtime layout detected. Loader and DLL files were installed beside SkyrimSE.exe; bundled scripts were installed into Data.".to_string());
    }
    warnings
}

// → Function: steam_libraries()
fn steam_libraries() -> Result<Vec<PathBuf>, String> {
    let mut libraries = BTreeSet::new();
    let home = home_dir().ok_or_else(|| "Could not resolve the home directory.".to_string())?;

    let roots = [
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ];

    for root in roots {
        if root.exists() {
            libraries.insert(root.clone());
        }

        let config_vdf = root.join("config/libraryfolders.vdf");
        let steamapps_vdf = root.join("steamapps/libraryfolders.vdf");
        for vdf in [config_vdf, steamapps_vdf] {
            for path in parse_libraryfolders(&vdf) {
                libraries.insert(path);
            }
        }
    }

    Ok(libraries.into_iter().collect())
}

// → Function: parse_libraryfolders()
fn parse_libraryfolders(path: &Path) -> Vec<PathBuf> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    contents
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with("\"path\"") {
                return None;
            }

            let value = trimmed
                .split_once("\"path\"")?
                .1
                .trim()
                .trim_matches('"')
                .replace("\\\\", "/");
            (!value.is_empty()).then(|| PathBuf::from(value))
        })
        .collect()
}

// → Function: find_manifest_for_game()
fn find_manifest_for_game(game_dir: &Path) -> Option<PathBuf> {
    let common_dir = game_dir.parent()?;
    let steamapps_dir = common_dir.parent()?;
    let manifest = steamapps_dir.join(format!("appmanifest_{SKYRIM_APP_ID}.acf"));
    manifest.is_file().then_some(manifest)
}


// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 11: HELPER FUNCTIONS - PATHS & CONFIG (lines 913-1020)
// Configuration and data directory management
// ════════════════════════════════════════════════════════════════════════════════════
// → Function: home_dir()
fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

// → Function: path_to_string()
fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

// → Function: app_data_dir()
fn app_data_dir() -> Result<PathBuf, String> {
    let home = home_dir().ok_or_else(|| "Could not resolve the home directory.".to_string())?;
    Ok(home.join(".local/share/skyrim-auto-modder"))
}

// → Function: app_config_dir()
fn app_config_dir() -> Result<PathBuf, String> {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "Could not resolve the project folder.".to_string())?
        .to_path_buf();
    Ok(project_root.join(".local/skyrim-auto-modder"))
}

// → Function: nexus_config_path()
fn nexus_config_path() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("nexus.json"))
}

// → Function: install_logs_path()
fn install_logs_path() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("install-log.json"))
}

// → Function: append_install_log()
fn append_install_log(entry: InstallLogEntry) -> Result<(), String> {
    let path = install_logs_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Could not create log folder: {err}"))?;
    }

    let mut logs = list_install_logs().unwrap_or_default();
    logs.insert(0, entry);
    let json = serde_json::to_string_pretty(&logs)
        .map_err(|err| format!("Could not serialize install logs: {err}"))?;
    fs::write(path, json).map_err(|err| format!("Could not save install logs: {err}"))
}

// → Function: installed_mods_dir()
fn installed_mods_dir() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("installed-mods"))
}

// → Function: installed_mod_manifest_path()
fn installed_mod_manifest_path(id: &str) -> Result<PathBuf, String> {
    Ok(installed_mods_dir()?.join(format!("{}.json", sanitize_filename(id))))
}

// → Function: local_archives_dir()
fn local_archives_dir() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("archives"))
}

// → Function: copy_archive_to_local_store()
fn copy_archive_to_local_store(source: &Path, filename: &str) -> Result<PathBuf, String> {
    let archives_dir = local_archives_dir()?;
    fs::create_dir_all(&archives_dir)
        .map_err(|err| format!("Could not create local archive store: {err}"))?;

    let destination = archives_dir.join(sanitize_filename(filename));
    fs::copy(source, &destination)
        .map_err(|err| format!("Could not save local archive copy: {err}"))?;
    Ok(destination)
}

// → Function: save_installed_mod()
fn save_installed_mod(installed_mod: &InstalledMod) -> Result<(), String> {
    let registry_dir = installed_mods_dir()?;
    fs::create_dir_all(&registry_dir)
        .map_err(|err| format!("Could not create installed mods registry: {err}"))?;

    let json = serde_json::to_string_pretty(installed_mod)
        .map_err(|err| format!("Could not serialize installed mod manifest: {err}"))?;
    fs::write(installed_mod_manifest_path(&installed_mod.id)?, json)
        .map_err(|err| format!("Could not save installed mod manifest: {err}"))
}

// → Function: load_nexus_api_key()
fn load_nexus_api_key() -> Result<Option<String>, String> {
    let path = nexus_config_path()?;
    if !path.is_file() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&path)
        .map_err(|err| format!("Could not read Nexus config: {err}"))?;
    let config = serde_json::from_str::<NexusConfig>(&contents)
        .map_err(|err| format!("Could not parse Nexus config: {err}"))?;
    Ok(Some(config.api_key))
}

// → Function: filename_from_url()
fn filename_from_url(url: &str) -> Option<String> {
    let without_query = url.split('?').next()?;
    let name = without_query.rsplit('/').next()?.trim();
    (!name.is_empty()).then(|| sanitize_filename(name))
}

// → Function: sanitize_filename()
fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => ch,
            _ => '-',
        })
        .collect();

    sanitized.trim_matches('-').to_string()
}

// → Function: timestamp()
fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[tauri::command]

// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 12: TAURI COMMANDS - SAVES & GAME (lines 1028-1211)
// Find save game files and launch the game
// ════════════════════════════════════════════════════════════════════════════════════
// → Function: get_saves_locations()
fn get_saves_locations(game_dir: String) -> Result<Vec<SavesLocation>, String> {
    let mut locations = Vec::new();

    let home = home_dir().ok_or("Could not resolve home directory")?;

    // Standard Skyrim saves location
    let standard_saves = home.join("Documents/My Games/Skyrim Special Edition/Saves");
    let standard_count = count_save_files(&standard_saves);
    locations.push(SavesLocation {
        name: "Standard (Documents)".to_string(),
        path: path_to_string(&standard_saves),
        exists: standard_saves.exists(),
        save_count: standard_count,
    });

    // Proton default prefix location (for Steam)
    let proton_saves = home.join(".local/share/Steam/steamapps/compatdata/489830/pfx/drive_c/users/steamuser/Documents/My Games/Skyrim Special Edition/Saves");
    let proton_count = count_save_files(&proton_saves);
    locations.push(SavesLocation {
        name: "Proton (Steam)".to_string(),
        path: path_to_string(&proton_saves),
        exists: proton_saves.exists(),
        save_count: proton_count,
    });

    // MO2 profile-specific saves (common locations)
    if let Some(mo2_appdata) = env::var_os("APPDATA") {
        let mo2_base = PathBuf::from(mo2_appdata).join("ModOrganizer2");
        if mo2_base.exists() {
            // Check for common MO2 instances
            for entry in fs::read_dir(&mo2_base).unwrap_or_else(|_| {
                fs::read_dir(home.join("AppData/Roaming/ModOrganizer2"))
                    .unwrap_or_else(|_| panic!("Could not read MO2 directory"))
            }) {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_dir() {
                        let profiles_dir = path.join("profiles");
                        if profiles_dir.exists() {
                            if let Ok(profiles) = fs::read_dir(&profiles_dir) {
                                for profile in profiles.filter_map(|e| e.ok()) {
                                    let profile_path = profile.path();
                                    if profile_path.is_dir() {
                                        let saves_dir = profile_path.join("Skyrim Special Edition/Saves");
                                        let profile_name = profile_path
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .map(|s| s.to_string())
                                            .unwrap_or_else(|| "Unknown".to_string());
                                        let save_count = count_save_files(&saves_dir);
                                        locations.push(SavesLocation {
                                            name: format!("MO2 Profile: {}", profile_name),
                                            path: path_to_string(&saves_dir),
                                            exists: saves_dir.exists(),
                                            save_count,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Also check in game directory (sometimes saves are there with mods)
    let game_path = PathBuf::from(&game_dir);
    let game_saves = game_path.join("Saves");
    if game_saves.exists() && game_saves != standard_saves && game_saves != proton_saves {
        let save_count = count_save_files(&game_saves);
        locations.push(SavesLocation {
            name: "Game Directory".to_string(),
            path: path_to_string(&game_saves),
            exists: true,
            save_count,
        });
    }

    Ok(locations)
}


// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 13: HELPER FUNCTIONS - SAVES (lines 1110-1129)
// ════════════════════════════════════════════════════════════════════════════════════
// → Function: count_save_files()
fn count_save_files(dir: &Path) -> usize {
    if !dir.is_dir() {
        return 0;
    }
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|entry| {
                    entry
                        .path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("ess"))
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}

#[tauri::command]
// → Function: run_skyrim()
fn run_skyrim(game_dir: String, use_skse: bool) -> Result<String, String> {
    let game_path = PathBuf::from(&game_dir);
    let installation = inspect_installation(&game_path);
    
    if !installation.valid && installation.issues.iter().any(|i| !i.contains("SKSE")) {
        return Err(format!(
            "Invalid Skyrim installation: {}",
            installation.issues.join(", ")
        ));
    }

    if use_skse && installation.skse_loader_path.is_none() {
        return Err("SKSE loader is not installed. Cannot run with SKSE.".to_string());
    }

    // Determine the executable to run
    let exe_to_run = if use_skse {
        installation
            .skse_loader_path
            .as_ref()
            .ok_or("SKSE loader path not found")?
            .clone()
    } else {
        installation
            .exe_path
            .as_ref()
            .ok_or("SkyrimSE.exe not found")?
            .clone()
    };

    // Check if this is a Steam installation with Proton
    let has_proton = if let Some(manifest_path) = &installation.steam_app_manifest {
        PathBuf::from(manifest_path).exists()
    } else {
        false
    };

    // Run the game
    if has_proton {
        // Use Proton through Steam
        run_with_proton(&exe_to_run, &game_dir)?;
    } else {
        // Direct native execution
        run_natively(&exe_to_run)?;
    }

    Ok(format!("Skyrim started{}", if use_skse { " with SKSE" } else { "" }))
}


// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 14: HELPER FUNCTIONS - GAME LAUNCHING (lines 1181-1211)
// Launch game via native execution or Proton
// ════════════════════════════════════════════════════════════════════════════════════
// → Function: run_natively()
fn run_natively(exe_path: &str) -> Result<(), String> {
    Command::new(exe_path)
        .spawn()
        .map_err(|err| format!("Could not start Skyrim: {err}"))?;
    Ok(())
}

// → Function: run_with_proton()
fn run_with_proton(exe_path: &str, game_dir: &str) -> Result<(), String> {
    let home = home_dir().ok_or("Could not resolve home directory")?;
    
    // Find Steam runtime and Proton
    let steam_root = home.join(".steam/steam");
    
    // Construct Proton environment
    let mut cmd = Command::new("sh");
    cmd.arg("-c");
    
    // Build the Proton command
    let proton_cmd = format!(
        "cd \"{}\" && STEAM_COMPAT_TOOL_PATHS=\"{}\" PROTON_USE_WINED3D=1 \"{}\"",
        game_dir,
        steam_root.join("compatibilitytools.d").to_string_lossy(),
        exe_path
    );
    
    cmd.arg(&proton_cmd);
    cmd.spawn()
        .map_err(|err| format!("Could not start Skyrim with Proton: {err}"))?;
    
    Ok(())
}


// ════════════════════════════════════════════════════════════════════════════════════
// SECTION 15: ENTRY POINT (lines 1214-1246)
// Tauri app initialization and setup
// ════════════════════════════════════════════════════════════════════════════════════
#[cfg_attr(mobile, tauri::mobile_entry_point)]
// → Function: run()
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                app.deep_link()
                    .register_all()
                    .map_err(|err| Box::<dyn std::error::Error>::from(err))?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_skyrim_installations,
            validate_skyrim_path,
            save_nexus_api_key,
            get_nexus_auth_status,
            list_installed_mods,
            uninstall_mod,
            list_install_logs,
            clear_install_logs,
            append_install_log_entry,
            install_mod_from_url,
            install_mod_from_archive,
            get_saves_locations,
            run_skyrim
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
