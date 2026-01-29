//! Configuration Storage
//!
//! Handles reading/writing configuration files to disk.
//! Config location: ~/.oxideterm on macOS/Linux, %APPDATA%\OxideTerm on Windows

use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;

use super::types::{ConfigFile, CONFIG_VERSION};

/// Configuration storage errors
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Failed to determine config directory")]
    NoConfigDir,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Config version {found} is newer than supported {supported}")]
    VersionTooNew { found: u32, supported: u32 },
}

/// Get the OxideTerm configuration directory
/// Returns %APPDATA%\OxideTerm on Windows, ~/.oxideterm on macOS/Linux
pub fn config_dir() -> Result<PathBuf, StorageError> {
    #[cfg(windows)]
    {
        // On Windows, prefer APPDATA for better compatibility
        if let Some(app_data) = dirs::config_dir() {
            return Ok(app_data.join("OxideTerm"));
        }
        // Fallback to home directory
        dirs::home_dir()
            .map(|home| home.join(".oxideterm"))
            .ok_or(StorageError::NoConfigDir)
    }

    #[cfg(not(windows))]
    {
        dirs::home_dir()
            .map(|home| home.join(".oxideterm"))
            .ok_or(StorageError::NoConfigDir)
    }
}

/// Get the log directory for storing application logs
pub fn log_dir() -> Result<PathBuf, StorageError> {
    Ok(config_dir()?.join("logs"))
}

/// Get the connections file path
pub fn connections_file() -> Result<PathBuf, StorageError> {
    Ok(config_dir()?.join("connections.json"))
}

/// Configuration storage manager
pub struct ConfigStorage {
    path: PathBuf,
}

impl ConfigStorage {
    /// Create a new storage manager with default path
    pub fn new() -> Result<Self, StorageError> {
        Ok(Self {
            path: connections_file()?,
        })
    }

    /// Create storage manager with custom path (for testing)
    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    /// Ensure the config directory exists
    async fn ensure_dir(&self) -> Result<(), StorageError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await?;
        }
        Ok(())
    }

    /// Load configuration from disk
    /// Returns default config if file doesn't exist
    /// If config is corrupted, creates a backup and returns default config
    pub async fn load(&self) -> Result<ConfigFile, StorageError> {
        match fs::read_to_string(&self.path).await {
            Ok(contents) => {
                match serde_json::from_str::<ConfigFile>(&contents) {
                    Ok(config) => {
                        // Check version
                        if config.version > CONFIG_VERSION {
                            return Err(StorageError::VersionTooNew {
                                found: config.version,
                                supported: CONFIG_VERSION,
                            });
                        }
                        // TODO: Run migrations if config.version < CONFIG_VERSION
                        Ok(config)
                    }
                    Err(e) => {
                        // JSON 解析失败 - 配置文件损坏
                        tracing::warn!("Config file corrupted: {}", e);
                        
                        // 创建备份
                        match self.backup().await {
                            Ok(backup_path) => {
                                tracing::warn!(
                                    "Corrupted config backed up to {:?}, using defaults",
                                    backup_path
                                );
                            }
                            Err(backup_err) => {
                                tracing::error!(
                                    "Failed to backup corrupted config: {}",
                                    backup_err
                                );
                            }
                        }
                        
                        // 返回默认配置
                        Ok(ConfigFile::default())
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ConfigFile::default()),
            Err(e) => Err(StorageError::Io(e)),
        }
    }

    /// Save configuration to disk
    pub async fn save(&self, config: &ConfigFile) -> Result<(), StorageError> {
        self.ensure_dir().await?;

        // Write to temp file first, then rename (atomic write)
        let temp_path = self.path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(config)?;

        let mut file = fs::File::create(&temp_path).await?;
        file.write_all(json.as_bytes()).await?;
        file.sync_all().await?;

        fs::rename(&temp_path, &self.path).await?;

        Ok(())
    }

    /// Check if config file exists
    pub async fn exists(&self) -> bool {
        fs::metadata(&self.path).await.is_ok()
    }

    /// Get config file path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Create a backup of the current config
    pub async fn backup(&self) -> Result<PathBuf, StorageError> {
        let backup_path = self.path.with_extension(format!(
            "json.backup.{}",
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        ));

        if self.exists().await {
            fs::copy(&self.path, &backup_path).await?;
        }

        Ok(backup_path)
    }
}

impl Default for ConfigStorage {
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| {
            panic!(
                "Failed to create ConfigStorage with default path: {}. \
                This is likely a system configuration issue.",
                e
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_load_nonexistent() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("test.json");
        let storage = ConfigStorage::with_path(path);

        let config = storage.load().await.unwrap();
        assert_eq!(config.version, CONFIG_VERSION);
        assert!(config.connections.is_empty());
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("test.json");
        let storage = ConfigStorage::with_path(path);

        let mut config = ConfigFile::default();
        config.groups.push("Work".to_string());

        storage.save(&config).await.unwrap();

        let loaded = storage.load().await.unwrap();
        assert_eq!(loaded.groups, vec!["Work"]);
    }
}
