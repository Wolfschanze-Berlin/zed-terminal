use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// Declares what an extension is allowed to access through IPC.
/// Constructed from the extension's manifest permissions section.
#[derive(Debug, Clone, Default)]
pub struct PermissionSet {
    http_domains: HashSet<String>,
    fs_read_paths: Vec<PathBuf>,
    fs_write_paths: Vec<PathBuf>,
    can_execute_commands: bool,
    can_use_storage: bool,
}

/// Sensitive filename patterns that are always blocked regardless of path permissions.
const SENSITIVE_EXACT_NAMES: &[&str] = &[
    ".env",
    "credentials",
    "credentials.json",
];

const SENSITIVE_PREFIXES: &[&str] = &[
    ".env.",
    "id_",
];

const SENSITIVE_EXTENSIONS: &[&str] = &[
    "pem",
    "key",
];

const SENSITIVE_FULL_PATHS: &[&str] = &[
    ".git/config",
];

fn is_sensitive_filename(filename: &str) -> bool {
    let lower = filename.to_lowercase();

    if SENSITIVE_EXACT_NAMES.iter().any(|&name| lower == name) {
        return true;
    }

    if SENSITIVE_PREFIXES.iter().any(|prefix| lower.starts_with(prefix)) {
        return true;
    }

    if let Some(ext) = Path::new(&lower).extension().and_then(|e| e.to_str()) {
        if SENSITIVE_EXTENSIONS.contains(&ext) {
            return true;
        }
    }

    false
}

fn contains_sensitive_path_segment(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    SENSITIVE_FULL_PATHS
        .iter()
        .any(|&pattern| normalized.contains(pattern))
}

impl PermissionSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_http_domains(mut self, domains: impl IntoIterator<Item = String>) -> Self {
        self.http_domains.extend(domains);
        self
    }

    pub fn with_fs_read_paths(mut self, paths: impl IntoIterator<Item = PathBuf>) -> Self {
        self.fs_read_paths.extend(paths);
        self
    }

    pub fn with_fs_write_paths(mut self, paths: impl IntoIterator<Item = PathBuf>) -> Self {
        self.fs_write_paths.extend(paths);
        self
    }

    pub fn with_commands(mut self, allowed: bool) -> Self {
        self.can_execute_commands = allowed;
        self
    }

    pub fn with_storage(mut self, allowed: bool) -> Self {
        self.can_use_storage = allowed;
        self
    }

    pub fn check_http(&self, domain: &str) -> Result<(), PermissionError> {
        if self.http_domains.contains("*") || self.http_domains.contains(domain) {
            Ok(())
        } else {
            Err(PermissionError::HttpDomainDenied(domain.to_string()))
        }
    }

    pub fn check_fs_read(&self, path: &Path) -> Result<(), PermissionError> {
        check_sensitive(path)?;

        let canonical = normalize_path(path);
        for allowed in &self.fs_read_paths {
            let allowed_canonical = normalize_path(allowed);
            if canonical.starts_with(&allowed_canonical) {
                return Ok(());
            }
        }

        Err(PermissionError::FsReadDenied(path.to_path_buf()))
    }

    pub fn check_fs_write(&self, path: &Path) -> Result<(), PermissionError> {
        check_sensitive(path)?;

        let canonical = normalize_path(path);
        for allowed in &self.fs_write_paths {
            let allowed_canonical = normalize_path(allowed);
            if canonical.starts_with(&allowed_canonical) {
                return Ok(());
            }
        }

        Err(PermissionError::FsWriteDenied(path.to_path_buf()))
    }

    pub fn check_commands(&self) -> Result<(), PermissionError> {
        if self.can_execute_commands {
            Ok(())
        } else {
            Err(PermissionError::CommandsDenied)
        }
    }

    pub fn check_storage(&self) -> Result<(), PermissionError> {
        if self.can_use_storage {
            Ok(())
        } else {
            Err(PermissionError::StorageDenied)
        }
    }

    /// Returns a PermissionSet that allows everything (for built-in panels).
    pub fn allow_all() -> Self {
        let mut http_domains = HashSet::new();
        http_domains.insert("*".to_string());
        Self {
            http_domains,
            fs_read_paths: vec![PathBuf::from("/")],
            fs_write_paths: vec![PathBuf::from("/")],
            can_execute_commands: true,
            can_use_storage: true,
        }
    }
}

fn check_sensitive(path: &Path) -> Result<(), PermissionError> {
    if contains_sensitive_path_segment(path) {
        return Err(PermissionError::SensitiveFileDenied(path.to_path_buf()));
    }

    if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
        if is_sensitive_filename(filename) {
            return Err(PermissionError::SensitiveFileDenied(path.to_path_buf()));
        }
    }

    Ok(())
}

/// Best-effort path normalization without touching the filesystem.
/// Uses `std::path::absolute` when available, otherwise falls back to the raw path.
fn normalize_path(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

#[derive(Debug, Clone)]
pub enum PermissionError {
    HttpDomainDenied(String),
    FsReadDenied(PathBuf),
    FsWriteDenied(PathBuf),
    SensitiveFileDenied(PathBuf),
    CommandsDenied,
    StorageDenied,
}

impl std::fmt::Display for PermissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HttpDomainDenied(domain) => {
                write!(f, "HTTP access denied for domain: {domain}")
            }
            Self::FsReadDenied(path) => {
                write!(f, "filesystem read denied: {}", path.display())
            }
            Self::FsWriteDenied(path) => {
                write!(f, "filesystem write denied: {}", path.display())
            }
            Self::SensitiveFileDenied(path) => {
                write!(f, "access to sensitive file denied: {}", path.display())
            }
            Self::CommandsDenied => write!(f, "command execution denied"),
            Self::StorageDenied => write!(f, "storage access denied"),
        }
    }
}

impl std::error::Error for PermissionError {}

/// Generate a Content Security Policy string for a webview extension.
///
/// Callers should inject this as a `<meta>` tag or via `WebviewConfig.initialization_scripts`
/// to enforce the policy inside the webview.
pub fn generate_csp() -> String {
    [
        "default-src 'none'",
        "script-src 'self' 'unsafe-inline'",
        "style-src 'self' 'unsafe-inline'",
        "img-src 'self' data: blob:",
        "font-src 'self' data:",
        "connect-src 'none'",
        "frame-src 'none'",
        "object-src 'none'",
        "base-uri 'none'",
    ]
    .join("; ")
}

/// Simple sliding-window rate limiter for IPC requests.
///
/// Uses a one-second window that resets when the current second elapses.
/// Thread-safe via atomic counter and a mutex-protected window start time.
pub struct RateLimiter {
    window_start: Mutex<Instant>,
    count: AtomicU64,
    max_per_second: u64,
}

impl RateLimiter {
    pub fn new(max_per_second: u64) -> Self {
        Self {
            window_start: Mutex::new(Instant::now()),
            count: AtomicU64::new(0),
            max_per_second,
        }
    }

    /// Returns Ok(()) if under limit, Err with message if rate exceeded.
    pub fn check(&self) -> Result<(), String> {
        let mut window_start = self
            .window_start
            .lock()
            .map_err(|err| format!("rate limiter lock poisoned: {err}"))?;

        let now = Instant::now();
        let elapsed = now.duration_since(*window_start);

        if elapsed.as_secs() >= 1 {
            *window_start = now;
            self.count.store(1, Ordering::Relaxed);
            return Ok(());
        }

        let current = self.count.fetch_add(1, Ordering::Relaxed) + 1;
        if current > self.max_per_second {
            return Err(format!(
                "rate limit exceeded: {current}/{} requests per second",
                self.max_per_second
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_domain_allowed() {
        let permissions = PermissionSet::new()
            .with_http_domains(["example.com".to_string(), "api.github.com".to_string()]);

        assert!(permissions.check_http("example.com").is_ok());
        assert!(permissions.check_http("api.github.com").is_ok());
    }

    #[test]
    fn test_http_domain_denied() {
        let permissions =
            PermissionSet::new().with_http_domains(["example.com".to_string()]);

        let result = permissions.check_http("evil.com");
        assert!(result.is_err());
        assert!(
            matches!(result, Err(PermissionError::HttpDomainDenied(domain)) if domain == "evil.com")
        );
    }

    #[test]
    fn test_fs_read_allowed() {
        let permissions =
            PermissionSet::new().with_fs_read_paths([PathBuf::from("/home/user/project")]);

        assert!(permissions
            .check_fs_read(Path::new("/home/user/project/src/main.rs"))
            .is_ok());
    }

    #[test]
    fn test_fs_read_denied_outside_path() {
        let permissions =
            PermissionSet::new().with_fs_read_paths([PathBuf::from("/home/user/project")]);

        let result = permissions.check_fs_read(Path::new("/etc/passwd"));
        assert!(result.is_err());
        assert!(matches!(result, Err(PermissionError::FsReadDenied(_))));
    }

    #[test]
    fn test_sensitive_file_blocked() {
        let permissions =
            PermissionSet::new().with_fs_read_paths([PathBuf::from("/home/user")]);

        let cases = [
            "/home/user/.env",
            "/home/user/.env.production",
            "/home/user/id_rsa",
            "/home/user/id_ed25519",
            "/home/user/server.pem",
            "/home/user/private.key",
            "/home/user/credentials",
            "/home/user/credentials.json",
            "/home/user/repo/.git/config",
        ];

        for path in cases {
            let result = permissions.check_fs_read(Path::new(path));
            assert!(
                matches!(result, Err(PermissionError::SensitiveFileDenied(_))),
                "expected SensitiveFileDenied for {path}, got {result:?}"
            );
        }
    }

    #[test]
    fn test_sensitive_file_blocked_on_write() {
        let permissions =
            PermissionSet::new().with_fs_write_paths([PathBuf::from("/home/user")]);

        let result = permissions.check_fs_write(Path::new("/home/user/.env"));
        assert!(matches!(result, Err(PermissionError::SensitiveFileDenied(_))));
    }

    #[test]
    fn test_rate_limiter_allows_under_limit() {
        let limiter = RateLimiter::new(100);

        for _ in 0..100 {
            assert!(limiter.check().is_ok());
        }
    }

    #[test]
    fn test_rate_limiter_rejects_over_limit() {
        let limiter = RateLimiter::new(5);

        for _ in 0..5 {
            assert!(limiter.check().is_ok());
        }

        assert!(limiter.check().is_err());
    }

    #[test]
    fn test_allow_all_permits_everything() {
        let permissions = PermissionSet::allow_all();

        assert!(permissions.check_http("anything.example.com").is_ok());
        assert!(permissions.check_commands().is_ok());
        assert!(permissions.check_storage().is_ok());
        // Note: allow_all still blocks sensitive files (defense in depth)
    }

    #[test]
    fn test_commands_denied_by_default() {
        let permissions = PermissionSet::new();
        assert!(matches!(
            permissions.check_commands(),
            Err(PermissionError::CommandsDenied)
        ));
    }

    #[test]
    fn test_storage_denied_by_default() {
        let permissions = PermissionSet::new();
        assert!(matches!(
            permissions.check_storage(),
            Err(PermissionError::StorageDenied)
        ));
    }

    #[test]
    fn test_generate_csp_contains_required_directives() {
        let csp = generate_csp();
        assert!(csp.contains("default-src 'none'"));
        assert!(csp.contains("script-src 'self' 'unsafe-inline'"));
        assert!(csp.contains("frame-src 'none'"));
        assert!(csp.contains("object-src 'none'"));
    }
}
