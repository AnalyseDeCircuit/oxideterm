use std::{
    borrow::Cow,
    fs, io,
    path::{Component, Path},
};

/// Return a command path suitable for direct stdio launch. cmd.exe cannot use a
/// verbatim script name; ordinary executables keep their original path semantics.
pub fn windows_batch_launch_path(executable: &Path) -> io::Result<Cow<'_, Path>> {
    let verbatim = matches!(executable.components().next(), Some(Component::Prefix(prefix)) if prefix.kind().is_verbatim());
    if !verbatim
        || !executable
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"))
    {
        return Ok(Cow::Borrowed(executable));
    }
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Plugin batch entry cannot be passed to the Windows command prompt unchanged",
        )
    };
    let text = executable.to_str().ok_or_else(invalid)?;
    if text.contains('%') {
        return Err(invalid());
    }
    let rest = text.strip_prefix(r"\\?\").ok_or_else(invalid)?;
    let path = if let Some(share) = rest.strip_prefix(r"UNC\") {
        std::path::PathBuf::from(format!(r"\\{share}"))
    } else if matches!(rest.as_bytes(), [drive, b':', b'\\', ..] if drive.is_ascii_alphabetic()) {
        rest.into()
    } else {
        return Err(invalid());
    };
    // Dropping the prefix must not change which validated file Windows opens.
    if fs::canonicalize(&path)? != executable {
        return Err(invalid());
    }
    // Rust probes this sibling before choosing the batch interpreter.
    let mut sibling = path.as_os_str().to_os_string();
    sibling.push(".exe");
    match fs::symlink_metadata(sibling) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Cow::Owned(path)),
        Err(e) => Err(e),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "A sibling .exe shadows the plugin batch entry",
        )),
    }
}
