use std::{
    collections::BTreeMap,
    env, fmt, fs,
    path::{Component, Path, PathBuf},
};

use serde::Deserialize;

pub(crate) const DEFAULT_AI_PACK_ROOT: &str = r"C:\ProgramData\R2H.AI-ELE";
const MANIFEST_RELATIVE_PATH: &str = "local-ai/manifests/r2h-multi-evidence-models.json";
/// Approved ProgramData pack layout ships the full llama.cpp HTTP suite at
/// `local-ai/runtimes/` (llama-server.exe + llama-server-impl.dll + shared DLLs).
/// The nested `llama-cpp/` folder is a CLI-only subset and must not be the only
/// candidate. Prefer the full suite; keep the nested path as a secondary layout.
const LLAMA_SERVER_RELATIVE_CANDIDATES: &[&str] = &[
    "local-ai/runtimes/llama-server.exe",
    "local-ai/runtimes/llama-cpp/llama-server.exe",
];
const PYTHON_RUNTIME_RELATIVE_PATH: &str = "local-ai/python-runtime/python.exe";
const RERANKER_WORKER_RELATIVE_PATH: &str = "local-ai/workers/reranker_worker.py";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AiPackRole {
    Generation,
    Embedding,
    Reranker,
}

impl AiPackRole {
    fn manifest_key(self) -> &'static str {
        match self {
            Self::Generation => "generation",
            Self::Embedding => "embedding",
            Self::Reranker => "reranker",
        }
    }

    fn manifest_role(self) -> &'static str {
        self.manifest_key()
    }

    fn missing_model_code(self) -> &'static str {
        match self {
            Self::Generation => "GENERATION_MODEL_MISSING",
            Self::Embedding => "EMBEDDING_MODEL_MISSING",
            Self::Reranker => "RERANKER_MODEL_MISSING",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Generation => "generation",
            Self::Embedding => "embedding",
            Self::Reranker => "reranker",
        }
    }
}

// Capability records keep the full manifest contract for probing, tests, and
// upstream consumers even when the active runtime path reads a subset.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct AiPackCapability {
    pub(crate) role: AiPackRole,
    pub(crate) model_id: String,
    pub(crate) model_path: PathBuf,
    pub(crate) runtime_path: PathBuf,
    pub(crate) worker_path: Option<PathBuf>,
    pub(crate) model_revision: Option<String>,
    pub(crate) provider: String,
    pub(crate) model_type: String,
    pub(crate) expected_size_bytes: Option<u64>,
    pub(crate) expected_sha256: Option<String>,
    pub(crate) pack_root: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct R2hAiPackResolver {
    root: PathBuf,
}

impl R2hAiPackResolver {
    pub(crate) fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub(crate) fn from_environment() -> Self {
        let root = env::var_os("R2H_LOCAL_AI_ROOT")
            .filter(|value| !value.to_string_lossy().trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| Self::default_root().to_path_buf());
        Self::new(root)
    }

    pub(crate) fn default_root() -> &'static Path {
        Path::new(DEFAULT_AI_PACK_ROOT)
    }

    #[allow(dead_code)]
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn manifest_path(&self) -> PathBuf {
        self.root.join(MANIFEST_RELATIVE_PATH)
    }

    pub(crate) fn resolve(&self, role: AiPackRole) -> Result<AiPackCapability, AiPackError> {
        if !self.root.is_dir() {
            return Err(AiPackError::new(
                "AI_PACK_ROOT_MISSING",
                format!(
                    "authoritative AI pack root is not a directory: {}",
                    self.root.display()
                ),
            ));
        }

        let manifest_path = self.manifest_path();
        let manifest_bytes = fs::read(&manifest_path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                AiPackError::new(
                    "AI_PACK_MANIFEST_MISSING",
                    format!("AI pack manifest is missing: {}", manifest_path.display()),
                )
            } else {
                AiPackError::new(
                    "AI_PACK_MANIFEST_UNREADABLE",
                    format!("AI pack manifest could not be read: {error}"),
                )
            }
        })?;

        let manifest: PackManifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
            AiPackError::new(
                "AI_PACK_MANIFEST_INVALID",
                format!("AI pack manifest is invalid JSON/schema: {error}"),
            )
        })?;

        if manifest.schema_version != 1 || !manifest.offline_only {
            return Err(AiPackError::new(
                "AI_PACK_MANIFEST_INVALID",
                "AI pack manifest must use schemaVersion=1 and offlineOnly=true",
            ));
        }

        let entry = manifest.models.get(role.manifest_key()).ok_or_else(|| {
            AiPackError::new(
                "MODEL_ROLE_MISSING",
                format!(
                    "active {} role is absent from the AI pack manifest",
                    role.display_name()
                ),
            )
        })?;

        if entry.role != role.manifest_role() {
            return Err(AiPackError::new(
                "AI_PACK_MANIFEST_INVALID",
                format!(
                    "manifest key {} declares role {:?}, expected {}",
                    role.manifest_key(),
                    entry.role,
                    role.manifest_role()
                ),
            ));
        }

        if !entry.required {
            return Err(AiPackError::new(
                "MODEL_ROLE_DISABLED",
                format!(
                    "active {} role is marked required=false",
                    role.display_name()
                ),
            ));
        }

        if entry.allow_remote_code.unwrap_or(false) {
            return Err(AiPackError::new(
                "MODEL_ROLE_UNSUPPORTED",
                format!("{} role requests remote code", role.display_name()),
            ));
        }

        let model_path = resolve_relative_path(&self.root, &entry.path)?;
        validate_expected_files(&self.root, role, &model_path, &entry.expected_files)?;

        let (runtime_path, worker_path) = match role {
            AiPackRole::Generation | AiPackRole::Embedding => {
                if entry.provider != "node-llama-cpp" {
                    return Err(AiPackError::new(
                        "MODEL_ROLE_UNSUPPORTED",
                        format!(
                            "{} role provider {} is not supported by the HTTP lifecycle manager",
                            role.display_name(),
                            entry.provider
                        ),
                    ));
                }
                (resolve_llama_server_runtime(&self.root)?, None)
            }
            AiPackRole::Reranker => {
                if entry.provider != "python-worker" {
                    return Err(AiPackError::new(
                        "MODEL_ROLE_UNSUPPORTED",
                        format!(
                            "reranker provider {} is not supported by the current worker contract",
                            entry.provider
                        ),
                    ));
                }
                let python_path = resolve_relative_path(&self.root, PYTHON_RUNTIME_RELATIVE_PATH)?;
                if !python_path.is_file() {
                    return Err(AiPackError::new(
                        "PYTHON_RUNTIME_MISSING",
                        format!(
                            "ProgramData Python runtime is missing: {}",
                            python_path.display()
                        ),
                    ));
                }
                ensure_existing_path_inside_root(&self.root, &python_path)?;
                let worker_path = resolve_relative_path(&self.root, RERANKER_WORKER_RELATIVE_PATH)?;
                if !worker_path.is_file() {
                    return Err(AiPackError::new(
                        "RERANKER_WORKER_MISSING",
                        format!(
                            "ProgramData reranker worker entrypoint is missing: {}",
                            worker_path.display()
                        ),
                    ));
                }
                ensure_existing_path_inside_root(&self.root, &worker_path)?;
                (python_path, Some(worker_path))
            }
        };

        let model_revision = entry
            .sha256
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| format!("sha256:{value}"));

        Ok(AiPackCapability {
            role,
            model_id: entry.id.clone(),
            model_path,
            runtime_path,
            worker_path,
            model_revision,
            provider: entry.provider.clone(),
            model_type: entry.model_type.clone(),
            expected_size_bytes: entry.expected_size_bytes,
            expected_sha256: entry.sha256.clone(),
            pack_root: self.root.clone(),
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AiPackError {
    code: &'static str,
    message: String,
}

impl AiPackError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for AiPackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AiPackError {}

#[derive(Debug, Deserialize)]
struct PackManifest {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    #[serde(rename = "offlineOnly")]
    offline_only: bool,
    models: BTreeMap<String, ManifestModel>,
}

#[derive(Debug, Deserialize)]
struct ManifestModel {
    id: String,
    role: String,
    path: String,
    #[serde(rename = "type")]
    model_type: String,
    provider: String,
    required: bool,
    #[serde(rename = "allowRemoteCode")]
    allow_remote_code: Option<bool>,
    #[serde(rename = "expectedSizeBytes")]
    expected_size_bytes: Option<u64>,
    sha256: Option<String>,
    #[serde(rename = "expectedFiles", default)]
    expected_files: Vec<String>,
}

fn resolve_llama_server_runtime(root: &Path) -> Result<PathBuf, AiPackError> {
    let mut checked = Vec::with_capacity(LLAMA_SERVER_RELATIVE_CANDIDATES.len());

    for relative in LLAMA_SERVER_RELATIVE_CANDIDATES {
        let candidate = resolve_relative_path(root, relative)?;
        checked.push(candidate.display().to_string());

        if !candidate.is_file() {
            continue;
        }

        ensure_existing_path_inside_root(root, &candidate)?;
        return Ok(candidate);
    }

    Err(AiPackError::new(
        "LLAMA_RUNTIME_MISSING",
        format!(
            "ProgramData HTTP runtime is missing. Checked: {}",
            checked.join("; ")
        ),
    ))
}

fn resolve_relative_path(root: &Path, relative: &str) -> Result<PathBuf, AiPackError> {
    let relative_path = Path::new(relative);
    if relative.trim().is_empty()
        || relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AiPackError::new(
            "AI_PACK_MANIFEST_INVALID",
            format!("manifest path must be a relative normal path: {relative}"),
        ));
    }

    let resolved = root.join(relative_path);
    if !resolved.starts_with(root) {
        return Err(AiPackError::new(
            "AI_PACK_MANIFEST_INVALID",
            format!("manifest path escapes the authoritative pack root: {relative}"),
        ));
    }

    Ok(resolved)
}

fn validate_expected_files(
    root: &Path,
    role: AiPackRole,
    model_path: &Path,
    expected_files: &[String],
) -> Result<(), AiPackError> {
    if !model_path.exists() {
        return Err(AiPackError::new(
            role.missing_model_code(),
            format!(
                "{} model asset is missing: {}",
                role.display_name(),
                model_path.display()
            ),
        ));
    }
    ensure_existing_path_inside_root(root, model_path)?;

    for expected_file in expected_files {
        let expected_path = Path::new(expected_file);
        if expected_file.trim().is_empty()
            || expected_path.is_absolute()
            || expected_path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(AiPackError::new(
                "AI_PACK_MANIFEST_INVALID",
                format!("manifest expected file is not a relative normal path: {expected_file}"),
            ));
        }

        let candidate = if model_path.is_dir() {
            model_path.join(expected_path)
        } else if expected_path.file_name() == model_path.file_name() {
            model_path.to_path_buf()
        } else {
            model_path
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(expected_path)
        };

        if candidate.exists() {
            ensure_existing_path_inside_root(root, &candidate)?;
        }

        if !candidate.is_file() {
            return Err(AiPackError::new(
                role.missing_model_code(),
                format!(
                    "{} expected asset is missing: {}",
                    role.display_name(),
                    candidate.display()
                ),
            ));
        }
    }

    Ok(())
}

fn ensure_existing_path_inside_root(root: &Path, path: &Path) -> Result<(), AiPackError> {
    let canonical_root = fs::canonicalize(root).map_err(|error| {
        AiPackError::new(
            "AI_PACK_ROOT_MISSING",
            format!("authoritative AI pack root could not be canonicalized: {error}"),
        )
    })?;
    let canonical_path = fs::canonicalize(path).map_err(|error| {
        AiPackError::new(
            "AI_PACK_MANIFEST_INVALID",
            format!("AI pack asset could not be canonicalized: {error}"),
        )
    })?;

    if !canonical_path.starts_with(&canonical_root) {
        return Err(AiPackError::new(
            "AI_PACK_MANIFEST_INVALID",
            format!(
                "AI pack asset escapes the authoritative root: {}",
                path.display()
            ),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use serde_json::json;
    use tempfile::{TempDir, tempdir};

    use super::*;

    fn manifest_for(model_path: &str, provider: &str, required: bool) -> serde_json::Value {
        json!({
            "schemaVersion": 1,
            "profileName": "TEST_PACK",
            "offlineOnly": true,
            "models": {
                "generation": {
                    "id": "generation-test",
                    "role": "generation",
                    "path": model_path,
                    "type": "gguf",
                    "provider": provider,
                    "required": required,
                    "expectedFiles": ["model.gguf"]
                },
                "embedding": {
                    "id": "embedding-test",
                    "role": "embedding",
                    "path": "local-ai/models/embedding/model.gguf",
                    "type": "gguf",
                    "provider": "node-llama-cpp",
                    "required": true,
                    "expectedFiles": ["model.gguf"]
                },
                "reranker": {
                    "id": "reranker-test",
                    "role": "reranker",
                    "path": "local-ai/models/reranker/model",
                    "type": "cross-encoder",
                    "provider": "python-worker",
                    "required": true,
                    "expectedFiles": ["config.json"]
                }
            }
        })
    }

    fn write_manifest(root: &Path, manifest: &serde_json::Value) -> std::io::Result<()> {
        let path = root.join("local-ai/manifests");
        fs::create_dir_all(&path)?;
        fs::write(
            path.join("r2h-multi-evidence-models.json"),
            serde_json::to_vec_pretty(manifest)?,
        )?;
        Ok(())
    }

    fn create_generation_assets(
        root: &Path,
        model_path: &str,
        include_server: bool,
    ) -> std::io::Result<()> {
        let model = root.join(model_path.replace('/', "\\"));
        fs::create_dir_all(model.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "model parent missing")
        })?)?;
        fs::write(&model, b"test model")?;

        if include_server {
            // Match the approved ProgramData layout: full suite under local-ai/runtimes/.
            let runtime = root.join("local-ai/runtimes");
            fs::create_dir_all(&runtime)?;
            fs::write(runtime.join("llama-server.exe"), b"test runtime")?;
        }
        Ok(())
    }

    fn write_server_placeholder(root: &Path, relative_directory: &str) -> std::io::Result<()> {
        let runtime = root.join(relative_directory.replace('/', "\\"));
        fs::create_dir_all(&runtime)?;
        fs::write(runtime.join("llama-server.exe"), b"test runtime")?;
        Ok(())
    }

    fn pack_with_generation(include_server: bool) -> std::io::Result<TempDir> {
        let directory = tempdir()?;
        let manifest = manifest_for(
            "local-ai/models/generation/model.gguf",
            "node-llama-cpp",
            true,
        );
        write_manifest(directory.path(), &manifest)?;
        create_generation_assets(
            directory.path(),
            "local-ai/models/generation/model.gguf",
            include_server,
        )?;
        Ok(directory)
    }

    #[test]
    fn default_root_is_the_authoritative_programdata_pack() {
        assert_eq!(
            R2hAiPackResolver::default_root(),
            Path::new(r"C:\ProgramData\R2H.AI-ELE")
        );
    }

    #[test]
    fn valid_generation_role_resolves_manifest_identity_and_runtime()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = pack_with_generation(true)?;
        let resolver = R2hAiPackResolver::new(directory.path());

        let capability = resolver.resolve(AiPackRole::Generation)?;

        assert_eq!(capability.model_id, "generation-test");
        assert_eq!(
            capability.model_path,
            directory
                .path()
                .join("local-ai/models/generation/model.gguf")
        );
        assert_eq!(
            capability.runtime_path,
            directory.path().join("local-ai/runtimes/llama-server.exe")
        );
        assert_eq!(capability.role, AiPackRole::Generation);
        Ok(())
    }

    #[test]
    #[ignore = "manual probe of the installed ProgramData pack; run with --ignored"]
    fn real_programdata_pack_probe() {
        let resolver = R2hAiPackResolver::from_environment();
        for role in [
            AiPackRole::Generation,
            AiPackRole::Embedding,
            AiPackRole::Reranker,
        ] {
            match resolver.resolve(role) {
                Ok(capability) => println!(
                    "{role:?}: OK model={} runtime={} worker={:?}",
                    capability.model_id,
                    capability.runtime_path.display(),
                    capability
                        .worker_path
                        .as_ref()
                        .map(|p| p.display().to_string())
                ),
                Err(error) => println!("{role:?}: {} ({error})", error.code()),
            }
        }
    }

    #[test]
    fn nested_llama_cpp_server_layout_is_accepted_when_full_suite_is_absent()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = pack_with_generation(false)?;
        write_server_placeholder(directory.path(), "local-ai/runtimes/llama-cpp")?;
        let resolver = R2hAiPackResolver::new(directory.path());

        let capability = resolver.resolve(AiPackRole::Generation)?;

        assert_eq!(
            capability.runtime_path,
            directory
                .path()
                .join("local-ai/runtimes/llama-cpp/llama-server.exe")
        );
        Ok(())
    }

    #[test]
    fn full_suite_runtime_is_preferred_over_nested_llama_cpp_layout()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = pack_with_generation(true)?;
        write_server_placeholder(directory.path(), "local-ai/runtimes/llama-cpp")?;
        let resolver = R2hAiPackResolver::new(directory.path());

        let capability = resolver.resolve(AiPackRole::Generation)?;

        assert_eq!(
            capability.runtime_path,
            directory.path().join("local-ai/runtimes/llama-server.exe")
        );
        Ok(())
    }

    #[test]
    fn missing_manifest_has_explicit_failure_code() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempdir()?;
        let Err(error) = R2hAiPackResolver::new(directory.path()).resolve(AiPackRole::Generation)
        else {
            return Err("missing manifest must fail closed".into());
        };

        assert_eq!(error.code(), "AI_PACK_MANIFEST_MISSING");
        Ok(())
    }

    #[test]
    fn invalid_manifest_has_explicit_failure_code() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempdir()?;
        let manifest = directory.path().join("local-ai/manifests");
        fs::create_dir_all(&manifest)?;
        fs::write(manifest.join("r2h-multi-evidence-models.json"), b"not-json")?;

        let Err(error) = R2hAiPackResolver::new(directory.path()).resolve(AiPackRole::Generation)
        else {
            return Err("invalid manifest must fail closed".into());
        };

        assert_eq!(error.code(), "AI_PACK_MANIFEST_INVALID");
        Ok(())
    }

    #[test]
    fn missing_model_has_role_specific_failure_code() -> Result<(), Box<dyn std::error::Error>> {
        let directory = pack_with_generation(true)?;
        let manifest = manifest_for(
            "local-ai/models/generation/missing.gguf",
            "node-llama-cpp",
            true,
        );
        write_manifest(directory.path(), &manifest)?;

        let Err(error) = R2hAiPackResolver::new(directory.path()).resolve(AiPackRole::Generation)
        else {
            return Err("missing model must fail closed".into());
        };

        assert_eq!(error.code(), "GENERATION_MODEL_MISSING");
        Ok(())
    }

    #[test]
    fn missing_llama_server_does_not_fall_back_to_repository_runtime()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = pack_with_generation(false)?;
        let Err(error) = R2hAiPackResolver::new(directory.path()).resolve(AiPackRole::Generation)
        else {
            return Err("missing HTTP runtime must fail closed".into());
        };

        assert_eq!(error.code(), "LLAMA_RUNTIME_MISSING");
        Ok(())
    }

    #[test]
    fn traversal_model_path_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let directory = pack_with_generation(true)?;
        let manifest = manifest_for("../outside.gguf", "node-llama-cpp", true);
        write_manifest(directory.path(), &manifest)?;

        let Err(error) = R2hAiPackResolver::new(directory.path()).resolve(AiPackRole::Generation)
        else {
            return Err("traversal path must fail closed".into());
        };

        assert_eq!(error.code(), "AI_PACK_MANIFEST_INVALID");
        Ok(())
    }

    #[test]
    fn disabled_active_role_has_explicit_failure_code() -> Result<(), Box<dyn std::error::Error>> {
        let directory = pack_with_generation(true)?;
        let manifest = manifest_for(
            "local-ai/models/generation/model.gguf",
            "node-llama-cpp",
            false,
        );
        write_manifest(directory.path(), &manifest)?;

        let Err(error) = R2hAiPackResolver::new(directory.path()).resolve(AiPackRole::Generation)
        else {
            return Err("disabled required role must fail closed".into());
        };

        assert_eq!(error.code(), "MODEL_ROLE_DISABLED");
        Ok(())
    }

    #[test]
    fn repository_local_runtime_is_never_used_as_a_fallback()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = pack_with_generation(false)?;
        let repository_runtime = directory.path().join("legacy-runtime/llama-server.exe");
        fs::create_dir_all(repository_runtime.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "runtime parent missing")
        })?)?;
        fs::write(&repository_runtime, b"legacy runtime")?;

        let Err(error) = R2hAiPackResolver::new(directory.path()).resolve(AiPackRole::Generation)
        else {
            return Err("pack runtime failure must not use repository-like fallback".into());
        };

        assert_eq!(error.code(), "LLAMA_RUNTIME_MISSING");
        Ok(())
    }

    #[test]
    fn python_cross_encoder_without_worker_entrypoint_fails_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = pack_with_generation(true)?;
        let manifest = manifest_for(
            "local-ai/models/generation/model.gguf",
            "node-llama-cpp",
            true,
        );
        write_manifest(directory.path(), &manifest)?;

        let reranker = directory.path().join("local-ai/models/reranker/model");
        fs::create_dir_all(&reranker)?;
        fs::write(reranker.join("config.json"), b"{}")?;
        let python = directory.path().join("local-ai/python-runtime/python.exe");
        fs::create_dir_all(python.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "python parent missing")
        })?)?;
        fs::write(&python, b"python")?;

        let Err(error) = R2hAiPackResolver::new(directory.path()).resolve(AiPackRole::Reranker)
        else {
            return Err("missing worker entrypoint must fail closed".into());
        };

        assert_eq!(error.code(), "RERANKER_WORKER_MISSING");
        Ok(())
    }

    #[test]
    fn python_cross_encoder_with_worker_entrypoint_resolves()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = pack_with_generation(true)?;
        let manifest = manifest_for(
            "local-ai/models/generation/model.gguf",
            "node-llama-cpp",
            true,
        );
        write_manifest(directory.path(), &manifest)?;

        let reranker = directory.path().join("local-ai/models/reranker/model");
        fs::create_dir_all(&reranker)?;
        fs::write(reranker.join("config.json"), b"{}")?;
        let python = directory.path().join("local-ai/python-runtime/python.exe");
        fs::create_dir_all(python.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "python parent missing")
        })?)?;
        fs::write(&python, b"python")?;
        let worker = directory
            .path()
            .join(RERANKER_WORKER_RELATIVE_PATH.replace('/', "\\"));
        fs::create_dir_all(worker.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "worker parent missing")
        })?)?;
        fs::write(&worker, b"worker")?;

        let capability = R2hAiPackResolver::new(directory.path()).resolve(AiPackRole::Reranker)?;

        assert_eq!(capability.runtime_path, python);
        assert_eq!(capability.worker_path, Some(worker));
        assert_eq!(capability.provider, "python-worker");
        Ok(())
    }
}
