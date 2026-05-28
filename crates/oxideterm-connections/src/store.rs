use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

// Store internals remain included at the crate-root store module so saved
// connection serialization and keychain helper visibility stay unchanged.
include!("store/types.rs");
include!("store/encrypted_config.rs");
include!("store/connection_store.rs");
include!("store/helpers.rs");
include!("store/sync.rs");
#[cfg(test)]
include!("store/tests.rs");
