//! Archive Commands
//!
//! Tauri commands for file compression and extraction.

use serde::Serialize;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// Archive entry info for preview
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub compressed_size: u64,
    pub modified: Option<String>,
}

/// Archive info for preview
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInfo {
    pub entries: Vec<ArchiveEntry>,
    pub total_files: usize,
    pub total_dirs: usize,
    pub total_size: u64,
    pub compressed_size: u64,
}

/// List contents of a zip archive for preview
#[tauri::command]
pub async fn list_archive_contents(archive_path: String) -> Result<ArchiveInfo, String> {
    let archive_path = Path::new(&archive_path);
    
    let file = File::open(archive_path)
        .map_err(|e| format!("Failed to open archive: {}", e))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| format!("Failed to read archive: {}", e))?;
    
    let mut entries = Vec::new();
    let mut total_files = 0;
    let mut total_dirs = 0;
    let mut total_size: u64 = 0;
    let mut compressed_size: u64 = 0;
    
    for i in 0..archive.len() {
        let file = archive.by_index(i)
            .map_err(|e| format!("Failed to read entry {}: {}", i, e))?;
        
        let name = file.name().to_string();
        let is_dir = file.is_dir();
        let size = file.size();
        let comp_size = file.compressed_size();
        
        // Get modification time
        let modified = file.last_modified().map(|dt| {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                dt.year(), dt.month(), dt.day(),
                dt.hour(), dt.minute(), dt.second()
            )
        });
        
        if is_dir {
            total_dirs += 1;
        } else {
            total_files += 1;
            total_size += size;
            compressed_size += comp_size;
        }
        
        // Extract just the filename for display
        let display_name = Path::new(&name)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| name.clone());
        
        entries.push(ArchiveEntry {
            name: display_name,
            path: name,
            is_dir,
            size,
            compressed_size: comp_size,
            modified,
        });
    }
    
    // Sort: directories first, then by path
    entries.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.path.cmp(&b.path),
        }
    });
    
    Ok(ArchiveInfo {
        entries,
        total_files,
        total_dirs,
        total_size,
        compressed_size,
    })
}

/// Compress files into a zip archive
#[tauri::command]
pub async fn compress_files(files: Vec<String>, archive_path: String) -> Result<(), String> {
    let archive_path = Path::new(&archive_path);
    
    // Create parent directory if needed
    if let Some(parent) = archive_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    
    let file = File::create(archive_path)
        .map_err(|e| format!("Failed to create archive: {}", e))?;
    let mut zip = ZipWriter::new(file);
    
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);
    
    for file_path in &files {
        let path = Path::new(file_path);
        
        if !path.exists() {
            continue;
        }
        
        let base_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        
        if path.is_dir() {
            // Walk directory recursively
            for entry in WalkDir::new(path) {
                let entry = entry.map_err(|e| format!("Failed to read directory: {}", e))?;
                let entry_path = entry.path();
                
                // Calculate relative path
                let relative_path = entry_path.strip_prefix(path.parent().unwrap_or(path))
                    .map_err(|e| format!("Failed to calculate relative path: {}", e))?;
                let name = relative_path.to_string_lossy();
                
                if entry_path.is_dir() {
                    // Add directory entry
                    let dir_name = if name.ends_with('/') {
                        name.to_string()
                    } else {
                        format!("{}/", name)
                    };
                    zip.add_directory(&dir_name, options)
                        .map_err(|e| format!("Failed to add directory: {}", e))?;
                } else {
                    // Add file
                    zip.start_file(name.to_string(), options)
                        .map_err(|e| format!("Failed to add file: {}", e))?;
                    
                    let mut f = File::open(entry_path)
                        .map_err(|e| format!("Failed to open file: {}", e))?;
                    let mut buffer = Vec::new();
                    f.read_to_end(&mut buffer)
                        .map_err(|e| format!("Failed to read file: {}", e))?;
                    zip.write_all(&buffer)
                        .map_err(|e| format!("Failed to write file: {}", e))?;
                }
            }
        } else {
            // Single file
            zip.start_file(base_name, options)
                .map_err(|e| format!("Failed to add file: {}", e))?;
            
            let mut f = File::open(path)
                .map_err(|e| format!("Failed to open file: {}", e))?;
            let mut buffer = Vec::new();
            f.read_to_end(&mut buffer)
                .map_err(|e| format!("Failed to read file: {}", e))?;
            zip.write_all(&buffer)
                .map_err(|e| format!("Failed to write file: {}", e))?;
        }
    }
    
    zip.finish().map_err(|e| format!("Failed to finalize archive: {}", e))?;
    Ok(())
}

/// Extract a zip archive to a destination directory
#[tauri::command]
pub async fn extract_archive(archive_path: String, dest_path: String) -> Result<(), String> {
    let archive_path = Path::new(&archive_path);
    let dest_path = Path::new(&dest_path);
    
    // Create destination directory
    fs::create_dir_all(dest_path)
        .map_err(|e| format!("Failed to create destination directory: {}", e))?;
    
    let file = File::open(archive_path)
        .map_err(|e| format!("Failed to open archive: {}", e))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| format!("Failed to read archive: {}", e))?;
    
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)
            .map_err(|e| format!("Failed to read entry: {}", e))?;
        
        let outpath = match file.enclosed_name() {
            Some(path) => dest_path.join(path),
            None => continue,
        };
        
        if file.is_dir() {
            fs::create_dir_all(&outpath)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        } else {
            // Create parent directories
            if let Some(parent) = outpath.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create directory: {}", e))?;
                }
            }
            
            let mut outfile = File::create(&outpath)
                .map_err(|e| format!("Failed to create file: {}", e))?;
            
            std::io::copy(&mut file, &mut outfile)
                .map_err(|e| format!("Failed to write file: {}", e))?;
        }
        
        // Set permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&outpath, fs::Permissions::from_mode(mode)).ok();
            }
        }
    }
    
    Ok(())
}
