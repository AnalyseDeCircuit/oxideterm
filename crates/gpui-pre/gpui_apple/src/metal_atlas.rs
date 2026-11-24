use anyhow::{Context as _, Result, ensure};
use etagere::BucketedAtlasAllocator;
use gpui::{
    AtlasBackend, AtlasKey, AtlasState, AtlasTextureId, AtlasTextureKind, AtlasTextureList,
    AtlasTile, Bounds, DevicePixels, PlatformAtlas, Point, Size,
};
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_metal::{
    MTLBlitCommandEncoder, MTLBuffer, MTLCommandBuffer, MTLCommandEncoder, MTLDevice, MTLOrigin,
    MTLPixelFormat, MTLRegion, MTLResourceOptions, MTLSize, MTLStorageMode, MTLTexture,
    MTLTextureDescriptor, MTLTextureUsage,
};
use parking_lot::Mutex;
use std::{borrow::Cow, ptr::NonNull};

const DEFAULT_ATLAS_SIZE: Size<DevicePixels> = Size {
    width: DevicePixels(1024),
    height: DevicePixels(1024),
};

// This is the maximum texture size supported by modern Apple GPUs.
const MAX_ATLAS_SIZE: Size<DevicePixels> = Size {
    width: DevicePixels(16384),
    height: DevicePixels(16384),
};

pub struct MetalAtlas(
    Mutex<AtlasState<MetalAtlasTextures>>,
    Mutex<Vec<gpui::GpuSubmission>>,
    Mutex<Vec<StreamUpload>>,
);

struct StreamUpload {
    texture: AtlasTextureId,
    bounds: Bounds<DevicePixels>,
    bytes: Vec<u8>,
}

impl MetalAtlas {
    pub(crate) fn new(
        device: Retained<ProtocolObject<dyn MTLDevice>>,
        supports_shared_storage: bool,
    ) -> Self {
        MetalAtlas(
            Mutex::new(AtlasState::new(MetalAtlasTextures {
                device,
                supports_shared_storage,
                monochrome_textures: AtlasTextureList::default(),
                polychrome_textures: AtlasTextureList::default(),
            })),
            Mutex::new(Vec::new()),
            Mutex::new(Vec::new()),
        )
    }

    /// Returns the GPU texture backing `id`, or `None` once every tile in it
    /// has been removed. A scene can still reference such a texture when a
    /// cached view replays a paint from before the image was dropped, so
    /// callers must skip those sprites rather than assume the texture exists.
    pub(crate) fn metal_texture(
        &self,
        id: AtlasTextureId,
    ) -> Option<Retained<ProtocolObject<dyn MTLTexture>>> {
        Some(self.0.lock().backend.texture(id)?.metal_texture.clone())
    }

    pub(crate) fn take_gpu_submissions(&self) -> Vec<gpui::GpuSubmission> {
        std::mem::take(&mut *self.1.lock())
    }

    pub(crate) fn encode_stream_uploads(
        &self,
        command: &ProtocolObject<dyn MTLCommandBuffer>,
    ) -> Result<()> {
        let lock = self.0.lock();
        let mut uploads = self.2.lock();
        if uploads.is_empty() {
            return Ok(());
        }
        let encoder = command
            .blitCommandEncoder()
            .context("creating stream blit encoder")?;
        let result = (|| {
            for upload in uploads.iter() {
                let Some(texture) = lock.backend.texture(upload.texture) else {
                    continue;
                };
                let width = upload.bounds.size.width.0 as usize;
                let height = upload.bounds.size.height.0 as usize;
                let pitch = (width * 4).div_ceil(256) * 256;
                let buffer = lock
                    .backend
                    .device
                    .newBufferWithLength_options(
                        pitch * height,
                        MTLResourceOptions::StorageModeShared,
                    )
                    .context("allocating stream staging buffer")?;
                // Rows are aligned for discrete Macs; the command buffer retains its resources.
                unsafe {
                    for y in 0..height {
                        std::ptr::copy_nonoverlapping(
                            upload.bytes.as_ptr().add(y * width * 4),
                            buffer.contents().as_ptr().cast::<u8>().add(y * pitch),
                            width * 4,
                        );
                    }
                    encoder.copyFromBuffer_sourceOffset_sourceBytesPerRow_sourceBytesPerImage_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                        &buffer, 0, pitch, pitch * height, MTLSize { width, height, depth: 1 },
                        &texture.metal_texture, 0, 0, MTLOrigin {
                            x: upload.bounds.origin.x.0 as usize, y: upload.bounds.origin.y.0 as usize, z: 0,
                        },
                    );
                }
            }
            Ok(())
        })();
        encoder.endEncoding();
        if result.is_ok() {
            uploads.clear();
        }
        result
    }
}

struct MetalAtlasTextures {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    supports_shared_storage: bool,
    monochrome_textures: AtlasTextureList<MetalAtlasTexture>,
    polychrome_textures: AtlasTextureList<MetalAtlasTexture>,
}

impl PlatformAtlas for MetalAtlas {
    fn contains(&self, key: &AtlasKey) -> bool {
        self.0.lock().contains(key)
    }

    fn stream_gpu_staging_bytes(&self, size: Size<DevicePixels>) -> Result<usize> {
        let width = usize::try_from(size.width.0)?;
        let height = usize::try_from(size.height.0)?;
        width
            .checked_mul(4)
            .and_then(|row| row.div_ceil(256).checked_mul(256))
            .and_then(|row| row.checked_mul(height))
            .context("stream upload size overflow")
    }

    fn stream_upload_staging_bytes(&self, size: Size<DevicePixels>) -> Result<usize> {
        let packed = usize::try_from(size.width.0)?
            .checked_mul(usize::try_from(size.height.0)?)
            .and_then(|n| n.checked_mul(4))
            .context("stream upload size overflow")?;
        packed
            .checked_add(self.stream_gpu_staging_bytes(size)?)
            .context("stream upload size overflow")
    }

    fn update_stream(
        &self,
        key: &AtlasKey,
        bounds: Bounds<DevicePixels>,
        bytes: &[u8],
    ) -> Result<()> {
        let lock = self.0.lock();
        let tile = lock
            .tile(key)
            .context("stream texture was retired before upload")?;
        self.2.lock().push(StreamUpload {
            texture: tile.texture_id,
            bounds: Bounds {
                origin: tile.bounds.origin + bounds.origin,
                size: bounds.size,
            },
            bytes: bytes.to_vec(),
        });
        Ok(())
    }

    fn gpu_submission(&self) -> Result<gpui::GpuSubmission> {
        let submission = gpui::GpuSubmission::default();
        self.1.lock().push(submission.clone());
        Ok(submission)
    }

    fn get_or_insert_with<'a>(
        &self,
        key: AtlasKey,
        build: &mut dyn FnMut() -> Result<Option<(Size<DevicePixels>, Cow<'a, [u8]>)>>,
    ) -> Result<Option<AtlasTile>> {
        self.0.lock().get_or_insert_with(key, build)
    }

    fn update(&self, key: &AtlasKey, bounds: Bounds<DevicePixels>, bytes: &[u8]) -> Result<()> {
        let lock = self.0.lock();
        let Some(tile) = lock.tile(key) else {
            return Ok(());
        };
        let upload_bounds = Bounds {
            origin: tile.bounds.origin + bounds.origin,
            size: bounds.size,
        };
        lock.backend
            .texture(tile.texture_id)
            .context("atlas tile refers to a missing texture")?
            .upload(
                upload_bounds.origin,
                &AtlasUpload::new(tile.texture_id.kind, bounds.size, bytes)?,
            )?;
        Ok(())
    }

    fn resource_generation(&self) -> u64 {
        // Metal devices are not recreated by the current backend.
        0
    }

    fn remove(&self, key: &AtlasKey) {
        if let Some(tile) = self.0.lock().tile(key) {
            self.2
                .lock()
                .retain(|upload| upload.texture != tile.texture_id);
        }
        self.0.lock().remove(key);
    }
}

impl Drop for MetalAtlas {
    fn drop(&mut self) {
        for submission in self.1.get_mut().drain(..) {
            submission.complete();
        }
    }
}

impl AtlasBackend for MetalAtlasTextures {
    fn insert(
        &mut self,
        key: &AtlasKey,
        size: Size<DevicePixels>,
        bytes: &[u8],
    ) -> Result<AtlasTile> {
        let kind = key.texture_kind();
        let upload = AtlasUpload::new(kind, size, bytes)?;
        let tile = if matches!(key, AtlasKey::DynamicTexture(_)) {
            self.allocate_dedicated(size, kind)
        } else {
            self.allocate(size, kind)
        }
        .context("failed to allocate")?;
        let texture = self
            .texture(tile.texture_id)
            .context("looking up allocated atlas texture")?;
        texture.upload(tile.bounds.origin, &upload)?;
        Ok(tile)
    }

    fn remove(&mut self, tile: AtlasTile) {
        let id = tile.texture_id;

        let textures = match id.kind {
            AtlasTextureKind::Monochrome => &mut self.monochrome_textures,
            AtlasTextureKind::Polychrome => &mut self.polychrome_textures,
            AtlasTextureKind::Subpixel => unreachable!(),
        };

        let Some(texture_slot) = textures
            .textures
            .iter_mut()
            .find(|texture| texture.as_ref().is_some_and(|v| v.id == id))
        else {
            return;
        };

        if let Some(mut texture) = texture_slot.take() {
            texture.decrement_ref_count();
            if texture.is_unreferenced() {
                textures.free_list.push(id.index as usize);
            } else {
                *texture_slot = Some(texture);
            }
        }
    }
}

impl MetalAtlasTextures {
    fn allocate_dedicated(
        &mut self,
        size: Size<DevicePixels>,
        texture_kind: AtlasTextureKind,
    ) -> Result<AtlasTile> {
        ensure!(
            size.width <= MAX_ATLAS_SIZE.width && size.height <= MAX_ATLAS_SIZE.height,
            "dynamic texture exceeds Metal size limit"
        );

        // Dynamic textures are mutable framebuffers, so keeping them out of the
        // shared atlas avoids dirty uploads contending with immutable entries.
        self.push_texture_with_size(size, texture_kind)?
            .allocate(size)
            .context("allocating dynamic texture")
    }

    fn allocate(
        &mut self,
        size: Size<DevicePixels>,
        texture_kind: AtlasTextureKind,
    ) -> Result<AtlasTile> {
        {
            let textures = match texture_kind {
                AtlasTextureKind::Monochrome => &mut self.monochrome_textures,
                AtlasTextureKind::Polychrome => &mut self.polychrome_textures,
                AtlasTextureKind::Subpixel => unreachable!(),
            };

            if let Some(tile) = textures
                .iter_mut()
                .rev()
                .find_map(|texture| texture.allocate(size))
            {
                return Ok(tile);
            }
        }

        let texture = self.push_texture(size, texture_kind)?;
        texture.allocate(size).context("allocating atlas tile")
    }

    fn push_texture(
        &mut self,
        min_size: Size<DevicePixels>,
        kind: AtlasTextureKind,
    ) -> Result<&mut MetalAtlasTexture> {
        let size = min_size.min(&MAX_ATLAS_SIZE).max(&DEFAULT_ATLAS_SIZE);
        self.push_texture_with_size(size, kind)
    }

    fn push_texture_with_size(
        &mut self,
        size: Size<DevicePixels>,
        kind: AtlasTextureKind,
    ) -> Result<&mut MetalAtlasTexture> {
        let texture_descriptor = MTLTextureDescriptor::new();
        // SAFETY: Both dimensions are clamped to the existing 1024..=16384 atlas size limits.
        unsafe {
            texture_descriptor.setWidth(size.width.0 as usize);
            texture_descriptor.setHeight(size.height.0 as usize);
        }
        let pixel_format;
        let usage;
        match kind {
            AtlasTextureKind::Monochrome => {
                pixel_format = MTLPixelFormat::A8Unorm;
                usage = MTLTextureUsage::ShaderRead;
            }
            AtlasTextureKind::Polychrome => {
                pixel_format = MTLPixelFormat::BGRA8Unorm;
                usage = MTLTextureUsage::ShaderRead;
            }
            AtlasTextureKind::Subpixel => unreachable!(),
        }
        texture_descriptor.setPixelFormat(pixel_format);
        texture_descriptor.setUsage(usage);
        // Shared memory mode can be used only on Apple GPU families
        // https://developer.apple.com/documentation/metal/mtlresourceoptions/storagemodeshared
        texture_descriptor.setStorageMode(if self.supports_shared_storage {
            MTLStorageMode::Shared
        } else {
            MTLStorageMode::Managed
        });
        let metal_texture = self
            .device
            .newTextureWithDescriptor(&texture_descriptor)
            .context("allocating atlas texture")?;

        let texture_list = match kind {
            AtlasTextureKind::Monochrome => &mut self.monochrome_textures,
            AtlasTextureKind::Polychrome => &mut self.polychrome_textures,
            AtlasTextureKind::Subpixel => unreachable!(),
        };

        let index = texture_list.free_list.pop().unwrap_or_else(|| {
            let index = texture_list.textures.len();
            texture_list.textures.push(None);
            index
        });

        let atlas_texture = MetalAtlasTexture {
            id: AtlasTextureId {
                index: index as u32,
                kind,
            },
            allocator: etagere::BucketedAtlasAllocator::new(size_to_etagere(size)),
            metal_texture,
            live_atlas_keys: 0,
        };

        Ok(texture_list.textures[index].insert(atlas_texture))
    }

    fn texture(&self, id: AtlasTextureId) -> Option<&MetalAtlasTexture> {
        let textures = match id.kind {
            AtlasTextureKind::Monochrome => &self.monochrome_textures,
            AtlasTextureKind::Polychrome => &self.polychrome_textures,
            AtlasTextureKind::Subpixel => unreachable!(),
        };
        textures.textures.get(id.index as usize)?.as_ref()
    }
}

struct MetalAtlasTexture {
    id: AtlasTextureId,
    allocator: BucketedAtlasAllocator,
    metal_texture: Retained<ProtocolObject<dyn MTLTexture>>,
    live_atlas_keys: u32,
}

impl MetalAtlasTexture {
    fn allocate(&mut self, size: Size<DevicePixels>) -> Option<AtlasTile> {
        let allocation = self.allocator.allocate(size_to_etagere(size))?;
        let tile = AtlasTile {
            texture_id: self.id,
            tile_id: allocation.id.into(),
            bounds: Bounds {
                origin: point_from_etagere(allocation.rectangle.min),
                size,
            },
            padding: 0,
        };
        self.live_atlas_keys += 1;
        Some(tile)
    }

    fn upload(&self, origin: Point<DevicePixels>, upload: &AtlasUpload<'_>) -> Result<()> {
        ensure!(
            self.id.kind == upload.kind,
            "atlas upload kind does not match texture"
        );
        ensure!(
            origin.x.0 >= 0
                && origin.y.0 >= 0
                && (origin.x.0 as usize)
                    .checked_add(upload.size.width.0 as usize)
                    .is_some_and(|end| end <= self.metal_texture.width())
                && (origin.y.0 as usize)
                    .checked_add(upload.size.height.0 as usize)
                    .is_some_and(|end| end <= self.metal_texture.height()),
            "atlas upload is outside texture bounds"
        );
        let region = MTLRegion {
            origin: MTLOrigin {
                x: origin.x.0 as usize,
                y: origin.y.0 as usize,
                z: 0,
            },
            size: MTLSize {
                width: upload.size.width.0 as usize,
                height: upload.size.height.0 as usize,
                depth: 1,
            },
        };
        // SAFETY: Validated bytes cover every row of the checked region in this CPU-accessible texture.
        // The borrowed bytes remain live for the copy; GPU ordering is a separate caller requirement.
        unsafe {
            self.metal_texture
                .replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                    region,
                    0,
                    NonNull::from(upload.bytes).cast(),
                    upload.bytes_per_row,
                );
        }
        Ok(())
    }

    fn decrement_ref_count(&mut self) {
        self.live_atlas_keys -= 1;
    }

    fn is_unreferenced(&mut self) -> bool {
        self.live_atlas_keys == 0
    }
}

struct AtlasUpload<'a> {
    kind: AtlasTextureKind,
    size: Size<DevicePixels>,
    bytes: &'a [u8],
    bytes_per_row: usize,
}

impl<'a> AtlasUpload<'a> {
    fn new(kind: AtlasTextureKind, size: Size<DevicePixels>, bytes: &'a [u8]) -> Result<Self> {
        ensure!(
            size.width.0 > 0 && size.height.0 > 0,
            "atlas dimensions must be positive"
        );
        let bytes_per_pixel = match kind {
            AtlasTextureKind::Monochrome => 1,
            AtlasTextureKind::Polychrome => 4,
            AtlasTextureKind::Subpixel => unreachable!(),
        };
        let bytes_per_row = (size.width.0 as usize)
            .checked_mul(bytes_per_pixel)
            .context("calculating atlas row byte count")?;
        let byte_count = bytes_per_row
            .checked_mul(size.height.0 as usize)
            .context("calculating atlas byte count")?;
        ensure!(
            bytes.len() >= byte_count,
            "atlas upload byte length is {}, expected at least {byte_count}",
            bytes.len()
        );
        Ok(Self {
            kind,
            size,
            bytes,
            bytes_per_row,
        })
    }
}

fn size_to_etagere(size: Size<DevicePixels>) -> etagere::Size {
    etagere::Size::new(size.width.into(), size.height.into())
}

fn point_from_etagere(value: etagere::Point) -> Point<DevicePixels> {
    Point {
        x: DevicePixels::from(value.x),
        y: DevicePixels::from(value.y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{DynamicTextureId, DynamicTextureParams, PlatformAtlas, point, size};
    use objc2_metal::{MTLCreateSystemDefaultDevice, MTLGPUFamily};
    use std::borrow::Cow;

    fn make_dynamic_texture_key(texture_id: usize) -> AtlasKey {
        AtlasKey::DynamicTexture(DynamicTextureParams {
            texture_id: DynamicTextureId(texture_id),
        })
    }

    #[test]
    fn test_remove_clears_stale_keys_from_tiles_by_key() {
        let Some(atlas) = create_atlas() else {
            return;
        };

        let small = Size {
            width: DevicePixels(64),
            height: DevicePixels(64),
        };

        let key_a = make_image_key(1, 0);
        let key_b = make_image_key(2, 0);
        let key_c = make_image_key(3, 0);

        let tile_a = insert_tile(&atlas, key_a.clone(), small);
        let tile_b = insert_tile(&atlas, key_b.clone(), small);
        let tile_c = insert_tile(&atlas, key_c.clone(), small);

        assert_eq!(tile_a.texture_id, tile_b.texture_id);
        assert_eq!(tile_b.texture_id, tile_c.texture_id);

        // Remove A: texture still has B and C, so it stays.
        // The key for A must be removed from tiles_by_key.
        atlas.remove(&key_a);

        // Remove B: texture still has C.
        atlas.remove(&key_b);

        // Remove C: texture becomes unreferenced and is deleted.
        atlas.remove(&key_c);

        // Re-inserting A must allocate a fresh tile on a new texture,
        // NOT return a stale tile referencing the deleted texture.
        let tile_a2 = insert_tile(&atlas, key_a, small);
        assert_eq!(tile_a2.texture_id, tile_a.texture_id);

        // The texture must actually exist — this would panic before the fix.
        assert!(atlas.metal_texture(tile_a2.texture_id).is_some());
    }

    #[test]
    fn test_metal_texture_is_none_after_last_tile_removed() {
        let Some(atlas) = create_atlas() else {
            return;
        };

        let key = make_image_key(1, 0);
        let tile = insert_tile(
            &atlas,
            key.clone(),
            Size {
                width: DevicePixels(64),
                height: DevicePixels(64),
            },
        );
        assert!(atlas.metal_texture(tile.texture_id).is_some());

        // A scene built before the removal may still carry `tile`; looking its
        // texture up must report the gap instead of panicking.
        atlas.remove(&key);
        assert!(atlas.metal_texture(tile.texture_id).is_none());
    }

    #[test]
    fn test_remove_nonexistent_key_is_noop() {
        let Some(atlas) = create_atlas() else {
            return;
        };
        let key = make_image_key(999, 0);
        atlas.remove(&key);
    }

    #[test]
    fn dynamic_textures_use_exact_sized_dedicated_metal_textures() {
        let Some(atlas) = create_atlas() else {
            return;
        };
        let size = Size {
            width: DevicePixels(64),
            height: DevicePixels(32),
        };

        let first = insert_tile(&atlas, make_dynamic_texture_key(1), size);
        let second = insert_tile(&atlas, make_dynamic_texture_key(2), size);

        assert_ne!(first.texture_id, second.texture_id);
        let first_texture = atlas.metal_texture(first.texture_id).unwrap();
        assert_eq!(first_texture.width(), 64);
        assert_eq!(first_texture.height(), 32);
        assert_eq!(atlas.resource_generation(), 0);
    }

    #[test]
    fn dynamic_texture_updates_upload_only_the_requested_region() {
        let Some(atlas) = create_atlas() else {
            return;
        };
        let size = Size {
            width: DevicePixels(2),
            height: DevicePixels(2),
        };
        let key = make_dynamic_texture_key(3);
        let tile = insert_tile(&atlas, key.clone(), size);
        let dirty_pixel = [1, 2, 3, 4];

        atlas
            .update(
                &key,
                Bounds {
                    origin: Point::new(DevicePixels(1), DevicePixels(1)),
                    size: Size {
                        width: DevicePixels(1),
                        height: DevicePixels(1),
                    },
                },
                &dirty_pixel,
            )
            .expect("partial upload should succeed");

        let texture = atlas.metal_texture(tile.texture_id).unwrap();
        let mut uploaded = [0u8; 16];
        unsafe {
            texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
                NonNull::new(uploaded.as_mut_ptr()).unwrap().cast(),
                8,
                MTLRegion {
                    origin: MTLOrigin { x: 0, y: 0, z: 0 },
                    size: MTLSize {
                        width: 2,
                        height: 2,
                        depth: 1,
                    },
                },
                0,
            );
        }
        assert_eq!(&uploaded[..12], &[0; 12]);
        assert_eq!(&uploaded[12..], &dirty_pixel);
    }

    #[test]
    fn test_invalid_upload_does_not_allocate() {
        let Some(atlas) = create_atlas() else {
            return;
        };
        let mut state = atlas.0.lock();
        for (kind, short_bytes, short_error) in [
            (
                AtlasTextureKind::Monochrome,
                &[][..],
                "atlas upload byte length is 0, expected at least 1",
            ),
            (
                AtlasTextureKind::Polychrome,
                &[0][..],
                "atlas upload byte length is 1, expected at least 4",
            ),
        ] {
            for (width, height, bytes, expected_error) in [
                (1, 1, short_bytes, short_error),
                (0, 1, &[][..], "atlas dimensions must be positive"),
                (1, 0, &[][..], "atlas dimensions must be positive"),
                (i32::MIN, 1, &[][..], "atlas dimensions must be positive"),
                (1, i32::MIN, &[][..], "atlas dimensions must be positive"),
            ] {
                let dimensions = size(DevicePixels(width), DevicePixels(height));
                let key = match kind {
                    AtlasTextureKind::Monochrome => AtlasKey::Svg(gpui::RenderSvgParams {
                        path: "test.svg".into(),
                        size: dimensions,
                    }),
                    AtlasTextureKind::Polychrome => make_image_key(1, 0),
                    AtlasTextureKind::Subpixel => unreachable!(),
                };
                assert_eq!(
                    state
                        .backend
                        .insert(&key, dimensions, bytes)
                        .err()
                        .map(|error| error.to_string()),
                    Some(expected_error.to_owned())
                );
                for textures in [
                    &state.backend.monochrome_textures,
                    &state.backend.polychrome_textures,
                ] {
                    assert_eq!((textures.textures.len(), textures.free_list.len()), (0, 0));
                }
            }
        }
    }

    #[test]
    fn test_texture_upload_preserves_pixel_layout() -> Result<()> {
        let Some(atlas) = create_atlas() else {
            return Ok(());
        };
        let mut state = atlas.0.lock();
        for (kind, bytes_per_pixel) in [
            (AtlasTextureKind::Monochrome, 1),
            (AtlasTextureKind::Polychrome, 4),
        ] {
            let texture = state
                .backend
                .push_texture(size(DevicePixels(8), DevicePixels(8)), kind)?;
            texture.upload(
                Point::default(),
                &AtlasUpload::new(
                    kind,
                    size(DevicePixels(8), DevicePixels(8)),
                    &vec![0; 8 * 8 * bytes_per_pixel],
                )?,
            )?;
            let pixels = (1..=3 * 2 * bytes_per_pixel)
                .map(|value| value as u8)
                .collect::<Vec<_>>();
            texture.upload(
                point(DevicePixels(2), DevicePixels(3)),
                &AtlasUpload::new(kind, size(DevicePixels(3), DevicePixels(2)), &pixels)?,
            )?;
            let mut actual = vec![0; 8 * 8 * bytes_per_pixel];
            unsafe {
                texture
                    .metal_texture
                    .getBytes_bytesPerRow_fromRegion_mipmapLevel(
                        NonNull::from(actual.as_mut_slice()).cast(),
                        8 * bytes_per_pixel,
                        MTLRegion {
                            origin: MTLOrigin { x: 0, y: 0, z: 0 },
                            size: MTLSize {
                                width: 8,
                                height: 8,
                                depth: 1,
                            },
                        },
                        0,
                    );
            }
            let mut expected = vec![0; actual.len()];
            for row in 0..2 {
                let start = ((row + 3) * 8 + 2) * bytes_per_pixel;
                let source = row * 3 * bytes_per_pixel;
                expected[start..start + 3 * bytes_per_pixel]
                    .copy_from_slice(&pixels[source..source + 3 * bytes_per_pixel]);
            }
            assert_eq!(actual, expected);
        }
        Ok(())
    }

    fn create_atlas() -> Option<MetalAtlas> {
        let device = MTLCreateSystemDefaultDevice()?;
        let supports_shared_storage =
            cfg!(target_os = "ios") || device.supportsFamily(MTLGPUFamily::Apple1);
        Some(MetalAtlas::new(device, supports_shared_storage))
    }

    fn make_image_key(image_id: usize, frame_index: usize) -> AtlasKey {
        AtlasKey::Image(gpui::RenderImageParams {
            image_id: gpui::ImageId(image_id),
            frame_index,
        })
    }

    fn insert_tile(atlas: &MetalAtlas, key: AtlasKey, size: Size<DevicePixels>) -> AtlasTile {
        atlas
            .get_or_insert_with(key, &mut || {
                let byte_count = (size.width.0 as usize) * (size.height.0 as usize) * 4;
                Ok(Some((size, Cow::Owned(vec![0u8; byte_count]))))
            })
            .expect("inserting atlas tile")
            .expect("retrieving inserted atlas tile")
    }
}
