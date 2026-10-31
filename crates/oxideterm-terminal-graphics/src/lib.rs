// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::AnimationDecoder;
use image::ImageDecoder;
use image::ImageReader;
use image::codecs::gif::GifDecoder;
use oxideterm_image_decoder::{ImageMemoryLease, reserve_image_memory};
use thiserror::Error;
use zeroize::Zeroizing;

// Protocol pieces stay in the crate root module to preserve the public API
// while separating model types, ingress state, decoding, parsing, and tests.
include!("graphics/types.rs");
include!("graphics/ingress.rs");
include!("graphics/decode.rs");
include!("graphics/sixel.rs");
include!("graphics/parse.rs");
include!("graphics/kitty.rs");
include!("graphics/grid.rs");
include!("graphics/relative.rs");
include!("graphics/placeholders.rs");
#[cfg(test)]
include!("graphics/tests.rs");
