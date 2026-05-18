use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Store internals remain included at the crate-root store module so saved
// connection serialization and keychain helper visibility stay unchanged.
include!("store/types.rs");
include!("store/connection_store.rs");
include!("store/helpers.rs");
#[cfg(test)]
include!("store/tests.rs");
