use std::path::{Path, PathBuf};

use serde::Deserialize;
use webview_runtime::PermissionSet;

/// Top-level manifest parsed from `extension.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ExtensionManifest {
    pub extension: ExtensionMeta,
    pub panel: PanelConfig,
    #[serde(default)]
    pub permissions: PermissionConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExtensionMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PanelConfig {
    /// HTML entry point relative to extension directory.
    pub entry: PathBuf,
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(default)]
    pub tooltip: String,
    #[serde(default)]
    pub starts_open: bool,
}

fn default_icon() -> String {
    "tool_web".into()
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PermissionConfig {
    #[serde(default)]
    pub http: Vec<String>,
    #[serde(default)]
    pub fs_read: Vec<String>,
    #[serde(default)]
    pub fs_write: Vec<String>,
    #[serde(default)]
    pub commands: bool,
    #[serde(default)]
    pub storage: bool,
}

impl PermissionConfig {
    pub fn to_permission_set(&self) -> PermissionSet {
        PermissionSet::new()
            .with_http_domains(self.http.iter().cloned())
            .with_fs_read_paths(self.fs_read.iter().map(PathBuf::from))
            .with_fs_write_paths(self.fs_write.iter().map(PathBuf::from))
            .with_commands(self.commands)
            .with_storage(self.storage)
    }
}

/// Discovered extension with its resolved paths.
#[derive(Debug, Clone)]
pub struct DiscoveredExtension {
    pub manifest: ExtensionManifest,
    pub extension_dir: PathBuf,
    pub entry_path: PathBuf,
}

/// Scan a directory for webview extensions.
///
/// Each subdirectory containing an `extension.toml` is treated as an extension.
/// Returns successfully parsed extensions; logs warnings for invalid ones.
pub fn discover_extensions(extensions_dir: &Path) -> Vec<DiscoveredExtension> {
    if !extensions_dir.is_dir() {
        return Vec::new();
    }

    let entries = match std::fs::read_dir(extensions_dir) {
        Ok(entries) => entries,
        Err(err) => {
            log::warn!(
                "Failed to read extensions directory {}: {err}",
                extensions_dir.display()
            );
            return Vec::new();
        }
    };

    let mut discovered = Vec::new();

    for entry in entries.flatten() {
        let extension_dir = entry.path();
        if !extension_dir.is_dir() {
            continue;
        }

        let manifest_path = extension_dir.join("extension.toml");
        if !manifest_path.is_file() {
            continue;
        }

        let manifest_content = match std::fs::read_to_string(&manifest_path) {
            Ok(content) => content,
            Err(err) => {
                log::warn!(
                    "Failed to read {}: {err}",
                    manifest_path.display()
                );
                continue;
            }
        };

        let manifest: ExtensionManifest = match toml::from_str(&manifest_content) {
            Ok(manifest) => manifest,
            Err(err) => {
                log::warn!(
                    "Failed to parse {}: {err}",
                    manifest_path.display()
                );
                continue;
            }
        };

        let entry_path = extension_dir.join(&manifest.panel.entry);
        if !entry_path.is_file() {
            log::warn!(
                "Extension '{}': entry file not found at {}",
                manifest.extension.id,
                entry_path.display()
            );
            continue;
        }

        discovered.push(DiscoveredExtension {
            manifest,
            extension_dir,
            entry_path,
        });
    }

    discovered
}

/// Returns the default extensions directory: `~/.zed-terminal/extensions/`
pub fn default_extensions_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".zed-terminal")
        .join("extensions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_manifest() {
        let toml_str = r#"
[extension]
id = "test-ext"
name = "Test Extension"
version = "0.1.0"

[panel]
entry = "panel/index.html"
"#;
        let manifest: ExtensionManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.extension.id, "test-ext");
        assert_eq!(manifest.panel.entry, PathBuf::from("panel/index.html"));
        assert_eq!(manifest.panel.icon, "tool_web");
        assert!(!manifest.panel.starts_open);
    }

    #[test]
    fn parse_full_manifest() {
        let toml_str = r#"
[extension]
id = "github-activity"
name = "GitHub Activity"
version = "1.0.0"
description = "GitHub activity feed"
author = "zed-terminal"

[panel]
entry = "panel/index.html"
icon = "github"
tooltip = "GitHub Activity"
starts_open = true

[permissions]
http = ["api.github.com", "github.com"]
fs_read = ["~/.config/gh"]
commands = true
storage = true
"#;
        let manifest: ExtensionManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.permissions.http.len(), 2);
        assert!(manifest.permissions.commands);
        assert!(manifest.panel.starts_open);
    }

    #[test]
    fn permission_config_to_permission_set() {
        let config = PermissionConfig {
            http: vec!["api.github.com".into()],
            fs_read: vec!["~/.config".into()],
            ..Default::default()
        };
        let permission_set = config.to_permission_set();
        assert!(permission_set.check_http("api.github.com").is_ok());
        assert!(permission_set.check_http("evil.com").is_err());
    }

    #[test]
    fn discover_extensions_empty_dir() {
        let dir = std::env::temp_dir().join("zed_test_empty_exts");
        std::fs::create_dir_all(&dir).unwrap();
        let result = discover_extensions(&dir);
        assert!(result.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn discover_extensions_nonexistent_dir() {
        let dir = std::env::temp_dir().join("zed_test_nonexistent_dir_12345");
        let result = discover_extensions(&dir);
        assert!(result.is_empty());
    }

    #[test]
    fn discover_extensions_valid_extension() {
        let dir = std::env::temp_dir().join("zed_test_valid_ext");
        let ext_dir = dir.join("my-extension");
        let panel_dir = ext_dir.join("panel");
        std::fs::create_dir_all(&panel_dir).unwrap();

        std::fs::write(
            ext_dir.join("extension.toml"),
            r#"
[extension]
id = "my-ext"
name = "My Extension"
version = "0.1.0"

[panel]
entry = "panel/index.html"
"#,
        )
        .unwrap();

        std::fs::write(panel_dir.join("index.html"), "<html></html>").unwrap();

        let result = discover_extensions(&dir);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].manifest.extension.id, "my-ext");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn discover_extensions_missing_entry_file() {
        let dir = std::env::temp_dir().join("zed_test_missing_entry");
        let ext_dir = dir.join("broken-extension");
        std::fs::create_dir_all(&ext_dir).unwrap();

        std::fs::write(
            ext_dir.join("extension.toml"),
            r#"
[extension]
id = "broken"
name = "Broken Extension"
version = "0.1.0"

[panel]
entry = "panel/index.html"
"#,
        )
        .unwrap();

        let result = discover_extensions(&dir);
        assert!(result.is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
