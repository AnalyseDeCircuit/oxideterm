//! Manual profiling of the production expansion loop, without exposing benchmark APIs.

use std::{hint::black_box, sync::Arc, time::Duration};

use criterion::{BenchmarkId, Criterion, Throughput};

use super::Scene;
use crate::window::{PreparedGlyphBatch, PreparedMonochromeGlyph, PreparedPolychromeGlyph};
use crate::{
    AtlasTextureId, AtlasTextureKind, AtlasTile, Bounds, ContentMask, DevicePixels, Hsla, Point,
    ScaledPixels, TileId, point, size,
};

fn fixture(kind: AtlasTextureKind, count: usize, half_clipped: bool) -> Arc<PreparedGlyphBatch> {
    let mut batch = PreparedGlyphBatch::default();
    for index in 0..count {
        let x = if half_clipped && index % 2 == 1 {
            4096.0
        } else {
            (index % 128) as f32 * 12.0
        };
        let bounds = Bounds::new(
            point(ScaledPixels(x), ScaledPixels((index / 128) as f32 * 20.0)),
            size(ScaledPixels(10.0), ScaledPixels(18.0)),
        );
        let tile = AtlasTile {
            texture_id: AtlasTextureId { index: 0, kind },
            tile_id: TileId(index as u32 + 1),
            padding: 0,
            bounds: Bounds::new(Point::default(), size(DevicePixels(10), DevicePixels(18))),
        };
        let glyph = PreparedMonochromeGlyph {
            bounds,
            color: Hsla {
                h: 0.6,
                s: 0.4,
                l: 0.7,
                a: 0.8,
            }
            .into(),
            tile,
        };
        match kind {
            AtlasTextureKind::Monochrome => batch.monochrome.push(glyph),
            AtlasTextureKind::Subpixel => batch.subpixel.push(glyph),
            AtlasTextureKind::Polychrome => batch
                .polychrome
                .push(PreparedPolychromeGlyph { bounds, tile }),
        }
    }
    Arc::new(batch)
}

// An ignored test gives this profiler access to the real private batch types;
// it is not a correctness test and is never part of the normal CI test workload.
#[test]
#[ignore = "manual Criterion profiling; run with --release --features bench --ignored --nocapture"]
fn prepared_glyph_batch_expansion() {
    let mut criterion = Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(1))
        .without_plots();
    let mut group = criterion.benchmark_group("prepared_glyph_batch_expansion");
    let origin = point(ScaledPixels(3.25), ScaledPixels(8.5));
    let mask = ContentMask {
        bounds: Bounds::new(
            Point::default(),
            size(ScaledPixels(2048.0), ScaledPixels(1024.0)),
        ),
    };
    for (name, kind) in [
        ("monochrome", AtlasTextureKind::Monochrome),
        ("subpixel", AtlasTextureKind::Subpixel),
        ("polychrome", AtlasTextureKind::Polychrome),
    ] {
        for count in [64, 4096] {
            for half_clipped in [false, true] {
                let batch = fixture(kind, count, half_clipped);
                let mut scene = Scene::default();
                // Terminal text already has a row layer. Keep its bounds tree stable
                // and retain vector capacity so allocator growth is outside measurement.
                scene.push_layer(mask.bounds);
                scene.insert_prepared_glyph_batch(batch.clone(), origin, mask, 0.65);
                let tiles = match kind {
                    AtlasTextureKind::Monochrome => scene
                        .monochrome_sprites
                        .iter()
                        .map(|s| s.tile.tile_id)
                        .collect::<Vec<_>>(),
                    AtlasTextureKind::Subpixel => scene
                        .subpixel_sprites
                        .iter()
                        .map(|s| s.tile.tile_id)
                        .collect(),
                    AtlasTextureKind::Polychrome => scene
                        .polychrome_sprites
                        .iter()
                        .map(|s| s.tile.tile_id)
                        .collect(),
                };
                let expected = (0..count)
                    .filter(|index| !half_clipped || index % 2 == 0)
                    .map(|index| TileId(index as u32 + 1))
                    .collect::<Vec<_>>();
                assert_eq!(tiles, expected, "fixture visibility and order: {name}");
                let visibility = if half_clipped {
                    "half_clipped"
                } else {
                    "visible"
                };
                group.throughput(Throughput::Elements(count as u64));
                group.bench_function(
                    BenchmarkId::new(format!("{name}/{visibility}"), count),
                    |b| {
                        b.iter(|| {
                            scene.monochrome_sprites.clear();
                            scene.subpixel_sprites.clear();
                            scene.polychrome_sprites.clear();
                            scene.paint_operations.clear();
                            scene.insert_prepared_glyph_batch(
                                black_box(batch.clone()),
                                black_box(origin),
                                black_box(mask),
                                black_box(0.65),
                            );
                            black_box((
                                &scene.monochrome_sprites,
                                &scene.subpixel_sprites,
                                &scene.polychrome_sprites,
                            ));
                        });
                    },
                );
            }
        }
    }
    group.finish();
    criterion.final_summary();
}
