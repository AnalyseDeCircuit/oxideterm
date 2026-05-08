// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashMap, fs, sync::Arc};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::AnimationDecoder;
use image::ImageReader;
use image::codecs::gif::GifDecoder;
use thiserror::Error;

// Protocol pieces stay in the crate root module to preserve the public API
// while separating model types, ingress state, decoding, parsing, and tests.
include!("graphics/types.rs");
include!("graphics/ingress.rs");
include!("graphics/decode.rs");
include!("graphics/parse.rs");
#[cfg(test)]
include!("graphics/tests.rs");
