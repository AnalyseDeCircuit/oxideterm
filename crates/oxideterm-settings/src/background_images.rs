// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Persistent storage for the application background-image gallery.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow};

const BACKGROUND_DIRECTORY_NAME: &str = "backgrounds";
const BACKGROUND_FILE_PREFIX: &str = "background";

/// Returns whether a path uses an image format supported by the GPUI renderer.
pub fn is_supported_background_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
            )
        })
        .unwrap_or(false)
}

/// Resolves the managed gallery beside the active settings file.
pub fn background_images_directory(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(BACKGROUND_DIRECTORY_NAME)
}

/// Installs a versioned bundled image into the managed gallery without replacing user files.
pub fn ensure_bundled_background_image(
    settings_path: &Path,
    file_name: &str,
    image_bytes: &[u8],
) -> Result<PathBuf> {
    let file_name_path = Path::new(file_name);
    if file_name_path.components().count() != 1
        || !matches!(
            file_name_path.components().next(),
            Some(Component::Normal(_))
        )
        || !is_supported_background_image(file_name_path)
    {
        return Err(anyhow!("invalid bundled background image filename"));
    }

    let directory = background_images_directory(settings_path);
    fs::create_dir_all(&directory).with_context(|| {
        format!(
            "failed to create background gallery {}",
            directory.display()
        )
    })?;
    let destination = directory.join(file_name_path);
    if destination.is_file() {
        return Ok(destination);
    }
    if destination.exists() {
        return Err(anyhow!(
            "bundled background destination is not a file: {}",
            destination.display()
        ));
    }

    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
    {
        Ok(mut destination_file) => {
            if let Err(error) = destination_file.write_all(image_bytes) {
                // Never expose a partially written bundled asset in the gallery.
                let _ = fs::remove_file(&destination);
                return Err(error).with_context(|| {
                    format!(
                        "failed to install bundled background image {}",
                        destination.display()
                    )
                });
            }
            Ok(destination)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists && destination.is_file() => {
            // Another process may finish the same first-run installation concurrently.
            Ok(destination)
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to create bundled background image {}",
                destination.display()
            )
        }),
    }
}

/// Returns whether an existing image is owned by the managed gallery.
pub fn is_managed_background_image(settings_path: &Path, image_path: &Path) -> bool {
    let directory = background_images_directory(settings_path);
    let Ok(canonical_directory) = directory.canonicalize() else {
        return false;
    };
    let Ok(canonical_image) = image_path.canonicalize() else {
        return false;
    };
    canonical_image.starts_with(canonical_directory)
}

/// Lists managed gallery images from newest to oldest without reading image bytes.
pub fn list_background_images(settings_path: &Path) -> Result<Vec<PathBuf>> {
    let directory = background_images_directory(settings_path);
    if !directory.exists() {
        return Ok(Vec::new());
    }

    let mut images = Vec::new();
    for entry in fs::read_dir(&directory)
        .with_context(|| format!("failed to read background gallery {}", directory.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to read an entry from background gallery {}",
                directory.display()
            )
        })?;
        let path = entry.path();
        if !entry.file_type()?.is_file() || !is_supported_background_image(&path) {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        images.push((path, modified));
    }

    // Modification time matches the gallery's user-facing "newest first" order.
    images.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| right.0.cmp(&left.0)));
    Ok(images.into_iter().map(|(path, _)| path).collect())
}

/// Copies selected source images into the managed gallery and returns stored paths.
pub fn import_background_images(
    settings_path: &Path,
    source_paths: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    let directory = background_images_directory(settings_path);
    fs::create_dir_all(&directory).with_context(|| {
        format!(
            "failed to create background gallery {}",
            directory.display()
        )
    })?;

    // Validate the complete batch before copying so an invalid selection cannot
    // leave only a prefix of the requested gallery update behind.
    for source_path in source_paths {
        if !source_path.is_file() {
            return Err(anyhow!(
                "background image does not exist: {}",
                source_path.display()
            ));
        }
        if !is_supported_background_image(source_path) {
            return Err(anyhow!(
                "unsupported background image format: {}",
                source_path.display()
            ));
        }
    }

    let mut imported_paths = Vec::with_capacity(source_paths.len());
    for source_path in source_paths {
        match copy_background_image(source_path, &directory) {
            Ok(imported_path) => imported_paths.push(imported_path),
            Err(error) => {
                // Roll back files from this batch when a later copy fails.
                for imported_path in &imported_paths {
                    let _ = fs::remove_file(imported_path);
                }
                return Err(error);
            }
        }
    }
    Ok(imported_paths)
}

/// Deletes one managed image while refusing paths outside the gallery directory.
pub fn remove_background_image(settings_path: &Path, image_path: &Path) -> Result<()> {
    if !image_path.exists() {
        return Ok(());
    }

    let directory = background_images_directory(settings_path);
    let canonical_directory = directory.canonicalize().with_context(|| {
        format!(
            "failed to resolve background gallery {}",
            directory.display()
        )
    })?;
    let canonical_image = image_path.canonicalize().with_context(|| {
        format!(
            "failed to resolve background image {}",
            image_path.display()
        )
    })?;
    if !canonical_image.starts_with(&canonical_directory) {
        return Err(anyhow!(
            "refusing to delete a file outside the background gallery"
        ));
    }

    fs::remove_file(&canonical_image).with_context(|| {
        format!(
            "failed to remove background image {}",
            canonical_image.display()
        )
    })
}

/// Removes every file owned by the managed background gallery.
pub fn clear_background_images(settings_path: &Path) -> Result<()> {
    let directory = background_images_directory(settings_path);
    if !directory.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&directory)
        .with_context(|| format!("failed to read background gallery {}", directory.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to read an entry from background gallery {}",
                directory.display()
            )
        })?;
        if entry.file_type()?.is_file() {
            fs::remove_file(entry.path()).with_context(|| {
                format!(
                    "failed to clear background image {}",
                    entry.path().display()
                )
            })?;
        }
    }
    Ok(())
}

fn copy_background_image(source_path: &Path, directory: &Path) -> Result<PathBuf> {
    let extension = source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| anyhow!("background image has no file extension"))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut collision_index = 0_u32;

    loop {
        let destination = directory.join(format!(
            "{BACKGROUND_FILE_PREFIX}_{timestamp}_{collision_index}.{extension}"
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
        {
            Ok(mut destination_file) => {
                let copy_result = fs::File::open(source_path)
                    .and_then(|mut source_file| io::copy(&mut source_file, &mut destination_file));
                if let Err(error) = copy_result {
                    // A partial gallery file must never be exposed as a selectable image.
                    let _ = fs::remove_file(&destination);
                    return Err(error).with_context(|| {
                        format!("failed to copy background image {}", source_path.display())
                    });
                }
                return Ok(destination);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                collision_index = collision_index
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("background image filename space exhausted"))?;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to create a stored background for {}",
                        source_path.display()
                    )
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gallery_import_retains_multiple_images() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let settings_path = temporary.path().join("profile/settings.json");
        let first_source = temporary.path().join("first.png");
        let second_source = temporary.path().join("second.jpg");
        fs::write(&first_source, b"first image").expect("first source");
        fs::write(&second_source, b"second image").expect("second source");

        let imported = import_background_images(&settings_path, &[first_source, second_source])
            .expect("import images");
        let listed = list_background_images(&settings_path).expect("list gallery");

        assert_eq!(imported.len(), 2);
        assert_eq!(listed.len(), 2);
        assert!(imported.iter().all(|path| listed.contains(path)));
        assert_eq!(
            fs::read(&imported[0]).expect("stored first"),
            b"first image"
        );
        assert_eq!(
            fs::read(&imported[1]).expect("stored second"),
            b"second image"
        );
    }

    #[test]
    fn bundled_background_install_is_stable_and_non_destructive() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let settings_path = temporary.path().join("profile/settings.json");

        let installed = ensure_bundled_background_image(
            &settings_path,
            "oxide-ambient-v1.png",
            b"bundled image",
        )
        .expect("install bundled background");
        let repeated = ensure_bundled_background_image(
            &settings_path,
            "oxide-ambient-v1.png",
            b"replacement image",
        )
        .expect("reuse bundled background");

        assert_eq!(installed, repeated);
        assert_eq!(
            installed,
            background_images_directory(&settings_path).join("oxide-ambient-v1.png")
        );
        assert_eq!(
            fs::read(installed).expect("installed bytes"),
            b"bundled image"
        );
    }

    #[test]
    fn bundled_background_install_rejects_unsafe_filenames() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let settings_path = temporary.path().join("settings.json");

        let result =
            ensure_bundled_background_image(&settings_path, "../outside.png", b"bundled image");

        assert!(result.is_err());
        assert!(!temporary.path().join("outside.png").exists());
    }

    #[test]
    fn gallery_rejects_unsupported_sources() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let settings_path = temporary.path().join("settings.json");
        let source = temporary.path().join("notes.txt");
        fs::write(&source, b"not an image").expect("source");

        let result = import_background_images(&settings_path, &[source]);

        assert!(result.is_err());
        assert!(
            list_background_images(&settings_path)
                .expect("list gallery")
                .is_empty()
        );
    }

    #[test]
    fn gallery_delete_refuses_external_files() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let settings_path = temporary.path().join("profile/settings.json");
        let managed_source = temporary.path().join("managed.png");
        let external_image = temporary.path().join("external.png");
        fs::write(&managed_source, b"managed image").expect("managed source");
        fs::write(&external_image, b"external image").expect("external source");
        import_background_images(&settings_path, &[managed_source]).expect("managed gallery");

        let result = remove_background_image(&settings_path, &external_image);

        assert!(result.is_err());
        assert!(external_image.exists());
    }

    #[test]
    fn gallery_clear_removes_all_managed_images() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let settings_path = temporary.path().join("profile/settings.json");
        let source = temporary.path().join("source.webp");
        fs::write(&source, b"image").expect("source");
        import_background_images(&settings_path, &[source]).expect("import image");

        clear_background_images(&settings_path).expect("clear gallery");

        assert!(
            list_background_images(&settings_path)
                .expect("list gallery")
                .is_empty()
        );
    }
}
