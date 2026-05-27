// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs::{File, OpenOptions},
    path::PathBuf,
};

use fs2::FileExt;
use parking_lot::RwLock;

use crate::{PortableError, portable_info};

static PORTABLE_INSTANCE_LOCK: std::sync::LazyLock<RwLock<Option<PortableInstanceLock>>> =
    std::sync::LazyLock::new(|| RwLock::new(None));

#[derive(Debug)]
struct PortableInstanceLock {
    _file: File,
}

pub fn portable_instance_lock_path() -> Result<Option<PathBuf>, PortableError> {
    let info = portable_info()?;
    Ok(info.is_portable.then(|| info.instance_lock_path.clone()))
}

pub fn acquire_portable_instance_lock() -> Result<(), PortableError> {
    let info = portable_info()?;
    if !info.is_portable {
        return Ok(());
    }

    if PORTABLE_INSTANCE_LOCK.read().is_some() {
        return Ok(());
    }

    std::fs::create_dir_all(&info.data_dir).map_err(PortableError::InstanceLockIo)?;
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&info.instance_lock_path)
        .map_err(PortableError::InstanceLockIo)?;

    match file.try_lock_exclusive() {
        Ok(()) => {
            *PORTABLE_INSTANCE_LOCK.write() = Some(PortableInstanceLock { _file: file });
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            Err(PortableError::InstanceLocked(info.data_dir.clone()))
        }
        Err(error) => Err(PortableError::InstanceLockIo(error)),
    }
}
