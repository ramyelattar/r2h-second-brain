use std::{fs, io, path::PathBuf};

use r2h_second_brain_desktop::COMMAND_NAMES;

/// Knowledge-core commands registered under `commands::*::tauri_handlers::`.
const NAMESPACED_COMMANDS: [&str; 13] = [
    "workspace_create",
    "workspace_list",
    "source_ingest_files",
    "source_list",
    "source_get",
    "search_execute",
    "citation_resolve",
    "audit_list",
    "integrity_verify",
    "create_consistent_backup",
    "verify_workspace_integrity",
    "scan_orphan_blobs",
    "restore_workspace_backup",
];

/// Local AI runtime lifecycle and legacy chat commands registered directly on the
/// app handle. Every addition here must be audited against the local/offline
/// architecture before this list is extended.
const DIRECT_COMMANDS: [&str; 21] = [
    "khoj_runtime_status",
    "khoj_runtime_start",
    "khoj_runtime_stop",
    "khoj_runtime_restart",
    "local_model_runtime_status",
    "local_model_runtime_start",
    "local_model_runtime_stop",
    "local_model_runtime_restart",
    "embedding_runtime_status",
    "embedding_runtime_start",
    "embedding_runtime_stop",
    "embedding_runtime_restart",
    "reranker_runtime_status",
    "reranker_runtime_start",
    "reranker_runtime_stop",
    "reranker_runtime_restart",
    "chat_send",
    "chat_sessions",
    "chat_history",
    "chat_stream_start",
    "chat_stream_cancel",
];

/// Source files that intentionally perform local process execution, loopback
/// socket probes, or loopback HTTP for the local AI runtime and legacy Khoj chat.
/// `live_probe.rs` is a test-only manual probe that starts/stops the runtime
/// managers (taskkill lifecycle control).
const LOCAL_RUNTIME_BOUNDARY_FILES: [&str; 8] = [
    "khoj_runtime.rs",
    "local_model_runtime.rs",
    "retrieval_runtime.rs",
    "embedding_client.rs",
    "reranker_client.rs",
    "chat_bridge.rs",
    "local_ai_client.rs",
    "live_probe.rs",
];

/// ELE trusted-bridge modules kept on disk as recovery artifacts but excluded
/// from compilation.
const DISABLED_PRIME_MODULES: [&str; 2] = ["trusted_bridge", "named_pipe"];

#[test]
fn capability_and_csp_are_local_and_minimal() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let capability: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("capabilities/default.json"))?)?;
    let permissions = capability["permissions"]
        .as_array()
        .ok_or_else(|| io::Error::other("capability permissions must be an array"))?;
    let approved: Vec<serde_json::Value> = [
        "dialog:allow-open",
        "core:event:allow-listen",
        "core:event:allow-unlisten",
    ]
    .iter()
    .map(|permission| serde_json::Value::String((*permission).to_owned()))
    .collect();
    assert_eq!(
        permissions, &approved,
        "capability permissions drifted; every addition must be justified and audited"
    );
    assert_eq!(capability["windows"][0], "main");

    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("tauri.conf.json"))?)?;
    let csp = config["app"]["security"]["csp"]
        .as_str()
        .ok_or_else(|| io::Error::other("CSP must be a string"))?;
    for directive in [
        "default-src 'self'",
        "script-src 'self'",
        "style-src 'self'",
        "connect-src ipc: http://ipc.localhost",
        "object-src 'none'",
        "frame-src 'none'",
        "base-uri 'self'",
    ] {
        assert!(csp.contains(directive));
    }
    assert!(!csp.contains('*'));
    assert!(!csp.contains("'unsafe-eval'"));
    assert!(!csp.contains("https://"));

    let browser_args = config["app"]["windows"][0]["additionalBrowserArgs"]
        .as_str()
        .ok_or_else(|| io::Error::other("WebView2 browser arguments must be configured"))?;
    assert_eq!(
        browser_args,
        "--disable-background-networking \
         --disable-component-update \
         --no-proxy-server \
         --disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,msOneAuthWAM"
    );
    for prohibited in [
        "--remote-debugging",
        "--proxy-server",
        "--disable-web-security",
        "--allow-running-insecure-content",
    ] {
        assert!(!browser_args.contains(prohibited));
    }
    Ok(())
}

#[test]
fn desktop_manifest_stays_layered_with_one_minimal_http_client()
-> Result<(), Box<dyn std::error::Error>> {
    let manifest =
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))?;
    for prohibited in [
        "knowledge-db",
        "knowledge-storage",
        "knowledge-ingestion",
        "knowledge-search",
        "rusqlite",
        "hyper",
        "ureq",
    ] {
        assert!(
            !manifest.contains(prohibited),
            "desktop manifest must not depend on {prohibited}"
        );
    }
    let http_client_lines: Vec<&str> = manifest
        .lines()
        .filter(|line| line.contains("reqwest"))
        .collect();
    assert_eq!(
        http_client_lines.len(),
        1,
        "exactly one HTTP client dependency (loopback AI provider) is approved"
    );
    assert!(
        http_client_lines[0].contains("default-features = false"),
        "reqwest must keep default features (proxy, native-tls, ...) disabled"
    );
    Ok(())
}

#[test]
fn command_sources_keep_generic_access_off_and_confine_runtime_io()
-> Result<(), Box<dyn std::error::Error>> {
    let mut sources = Vec::new();
    collect_rust_sources(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut sources,
    )?;
    for (file_name, content) in &sources {
        if is_runtime_boundary_or_disabled(file_name) {
            continue;
        }
        for prohibited in [
            "std::process",
            "Command::new",
            "TcpStream",
            "UdpSocket",
            "reqwest",
            "hyper::",
            "ureq",
            "execute_sql",
            "read_file",
            "write_file",
            "list_directory",
            "open_arbitrary_path",
        ] {
            assert!(
                !content.contains(prohibited),
                "{file_name} must not contain {prohibited}"
            );
        }
    }
    for (file_name, content) in &sources {
        // Negative-test fixtures live in `#[cfg(test)]` modules at the end of each
        // file; the strict URL boundary applies to the production prefix only.
        assert_urls_are_loopback(file_name, strip_cfg_test_suffix(content));
    }
    for required in [
        "khoj_runtime.rs",
        "local_model_runtime.rs",
        "retrieval_runtime.rs",
    ] {
        let content = sources
            .iter()
            .find(|(file_name, _)| file_name == required)
            .map(|(_, content)| content)
            .ok_or_else(|| io::Error::other(format!("{required} was not found under src")))?;
        assert!(
            content.contains("is_loopback"),
            "{required} must enforce loopback-only endpoints"
        );
    }
    let mut expected = NAMESPACED_COMMANDS.to_vec();
    expected.extend_from_slice(&DIRECT_COMMANDS);
    expected.sort_unstable();
    let mut actual: Vec<&str> = COMMAND_NAMES.to_vec();
    actual.sort_unstable();
    assert_eq!(
        actual, expected,
        "command registry drifted; every addition must be audited before approval"
    );
    Ok(())
}

#[test]
fn production_registry_contains_each_approved_command_once()
-> Result<(), Box<dyn std::error::Error>> {
    let source = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))?;
    let registry = source
        .split("tauri::generate_handler![")
        .nth(1)
        .and_then(|value| value.split("])").next())
        .ok_or_else(|| io::Error::other("generated handler registry was not found"))?;
    let entries: Vec<&str> = registry
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect();
    assert_eq!(
        entries.len(),
        COMMAND_NAMES.len(),
        "registry entry count must equal COMMAND_NAMES"
    );
    for command in COMMAND_NAMES {
        let registrations = entries
            .iter()
            .filter(|entry| entry.rsplit("::").next() == Some(command))
            .count();
        assert_eq!(
            registrations, 1,
            "{command} must be registered exactly once"
        );
    }
    let namespaced = entries
        .iter()
        .filter(|entry| entry.contains("tauri_handlers::"))
        .count();
    assert_eq!(
        namespaced,
        NAMESPACED_COMMANDS.len(),
        "knowledge-core commands must stay under commands::*::tauri_handlers::"
    );
    Ok(())
}

#[test]
fn khoj_runtime_is_loopback_only_and_exposes_only_lifecycle_commands()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(root.join("src/khoj_runtime.rs"))?;
    assert!(source.contains("127.0.0.1:42110"));
    assert!(source.contains("is_loopback"));
    assert!(
        !source.contains("#[tauri::command]"),
        "khoj_runtime.rs must not define commands directly"
    );
    let khoj_commands: Vec<&str> = COMMAND_NAMES
        .iter()
        .copied()
        .filter(|name| name.contains("khoj"))
        .collect();
    assert_eq!(
        khoj_commands,
        [
            "khoj_runtime_status",
            "khoj_runtime_start",
            "khoj_runtime_stop",
            "khoj_runtime_restart"
        ]
    );
    let prime_module = fs::read_to_string(root.join("src/prime_integration/mod.rs"))?;
    for disabled in DISABLED_PRIME_MODULES {
        assert!(
            !prime_module.contains(&format!("mod {disabled};")),
            "the disabled ELE bridge module {disabled} must stay out of compilation"
        );
    }
    Ok(())
}

fn is_runtime_boundary_or_disabled(file_name: &str) -> bool {
    if LOCAL_RUNTIME_BOUNDARY_FILES.contains(&file_name) {
        return true;
    }
    DISABLED_PRIME_MODULES
        .iter()
        .any(|module| file_name == format!("{module}.rs"))
}

fn assert_urls_are_loopback(file_name: &str, content: &str) {
    let mut index = 0;
    while let Some(found) = content[index..].find("http://") {
        let after_scheme = index + found + "http://".len();
        let rest = &content[after_scheme..];
        let host_end = rest.find(['"', '/', ' ', ':']).unwrap_or(rest.len());
        let host = &rest[..host_end];
        let reserved_test_host = host.ends_with(".invalid");
        assert!(
            rest.starts_with('"') || host.starts_with("127.0.0.1") || reserved_test_host,
            "{file_name} exposes a non-loopback http URL"
        );
        index = after_scheme;
    }
    assert!(
        !content.contains("https://"),
        "{file_name} must not target remote https endpoints"
    );
}

/// Cuts the trailing `#[cfg(test)]` module (this crate keeps test modules at the
/// end of each file) so rejection-test fixtures do not trip production scans.
fn strip_cfg_test_suffix(content: &str) -> &str {
    match content.find("#[cfg(test)]") {
        Some(position) => &content[..position],
        None => content,
    }
}

fn collect_rust_sources(
    directory: &std::path::Path,
    output: &mut Vec<(String, String)>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_rust_sources(&path, output)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| io::Error::other("non-UTF8 source file name"))?
                .to_owned();
            output.push((file_name, fs::read_to_string(&path)?));
        }
    }
    Ok(())
}
