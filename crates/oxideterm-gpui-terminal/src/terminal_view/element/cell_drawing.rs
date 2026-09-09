// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

//! Font-independent cell graphics. Coordinates are quantized in device pixels
//! before conversion back to GPUI pixels, including shared edges between cells.

use std::fmt::Write;

use gpui::{Bounds, Pixels, Point, point, px, size};

pub(super) enum CellDrawing {
    Fill(Bounds<Pixels>, f32),
    Line(Point<Pixels>, Point<Pixels>, Pixels),
    Curve([Point<Pixels>; 4], Pixels),
}

pub(super) fn is_cell_drawing(ch: char) -> bool {
    ('\u{2500}'..='\u{259f}').contains(&ch)
}

struct CellGeometry {
    edges: [f32; 4],
    center: [f32; 2],
    stroke: f32,
    scale: f32,
}

impl CellGeometry {
    #[cfg(test)]
    fn new(bounds: Bounds<Pixels>, scale: f32) -> Self {
        let stroke = (bounds.size.width.as_f32() * scale / 8.0).round().max(1.0);
        Self::with_stroke(bounds, scale, stroke)
    }

    fn with_stroke(bounds: Bounds<Pixels>, scale: f32, stroke: f32) -> Self {
        let edges = [bounds.left(), bounds.top(), bounds.right(), bounds.bottom()]
            .map(|value| (value.as_f32() * scale).round());
        let center = [0, 1]
            .map(|axis| ((edges[axis] + edges[axis + 2] - stroke) / 2.0).round() + stroke / 2.0);
        Self {
            edges,
            center,
            stroke,
            scale,
        }
    }

    fn point(&self, x: f32, y: f32) -> Point<Pixels> {
        point(px(x / self.scale), px(y / self.scale))
    }

    fn rectangle(&self, coordinates: [f32; 4], alpha: f32, emit: &mut impl FnMut(CellDrawing)) {
        let [x0, y0, x1, y1] = std::array::from_fn(|index| {
            let axis = index % 2;
            coordinates[index]
                .round()
                .clamp(self.edges[axis], self.edges[axis + 2])
        });
        if x1 > x0 && y1 > y0 {
            emit(CellDrawing::Fill(
                Bounds::from_corners(self.point(x0, y0), self.point(x1, y1)),
                alpha,
            ));
        }
    }

    fn fraction(&self, fractions: [f32; 4], alpha: f32, emit: &mut impl FnMut(CellDrawing)) {
        let coordinates = std::array::from_fn(|index| {
            let axis = index % 2;
            self.edges[axis] + (self.edges[axis + 2] - self.edges[axis]) * fractions[index]
        });
        self.rectangle(coordinates, alpha, emit);
    }

    fn line(&self, a: [f32; 2], b: [f32; 2], emit: &mut impl FnMut(CellDrawing)) {
        if a != b {
            emit(CellDrawing::Line(
                self.point(a[0], a[1]),
                self.point(b[0], b[1]),
                px(self.stroke / self.scale),
            ));
        }
    }

    fn connections(&self, weights: [u8; 4], emit: &mut impl FnMut(CellDrawing)) {
        // Each arm ends at the crossing arm's rail. Double junctions leave
        // their central channels open instead of painting a solid cross.
        for direction in 0..4 {
            let weight = weights[direction];
            if weight == 0 {
                continue;
            }
            let axis = direction / 2;
            let cross_axis = 1 - axis;
            let sign = if direction % 2 == 0 { -1.0 } else { 1.0 };
            let crossing = [weights[cross_axis * 2], weights[cross_axis * 2 + 1]];
            let lanes: &[i8] = if weight == 3 { &[-1, 1] } else { &[0] };
            let thickness = self.stroke * if weight == 2 { 2.0 } else { 1.0 };
            for &lane in lanes {
                let junction = if weight == 3 && crossing.contains(&3) {
                    let side = usize::from(lane > 0);
                    if crossing[side] == 3 {
                        self.stroke
                    } else if weights[direction ^ 1] == 0 {
                        -self.stroke
                    } else {
                        0.0
                    }
                } else if crossing.contains(&3) {
                    if crossing == [3, 3] {
                        self.stroke
                    } else {
                        -self.stroke
                    }
                } else {
                    -self.stroke * f32::from(crossing[0].max(crossing[1])) / 2.0
                };
                let rail_overlap = if crossing.contains(&3) {
                    self.stroke / 2.0
                } else {
                    0.0
                };
                let inner = self.center[axis] + sign * (junction - rail_overlap);
                let outer = self.edges[axis + if sign > 0.0 { 2 } else { 0 }];
                let across = self.center[cross_axis] + f32::from(lane) * self.stroke;
                let mut rectangle = [0.0; 4];
                rectangle[axis] = inner.min(outer);
                rectangle[axis + 2] = inner.max(outer);
                rectangle[cross_axis] = across - thickness / 2.0;
                rectangle[cross_axis + 2] = across + thickness / 2.0;
                self.rectangle(rectangle, 1.0, emit);
            }
        }
    }
}

#[cfg(test)]
pub(super) fn draw_cell(
    ch: char,
    bounds: Bounds<Pixels>,
    scale: f32,
    emit: impl FnMut(CellDrawing),
) {
    draw_geometry(ch, CellGeometry::new(bounds, scale), emit);
}

fn draw_geometry(ch: char, geometry: CellGeometry, mut emit: impl FnMut(CellDrawing)) {
    let code = ch as u32;
    match code {
        0x2580 => geometry.fraction([0.0, 0.0, 1.0, 0.5], 1.0, &mut emit),
        0x2581..=0x2588 => geometry.fraction(
            [0.0, 1.0 - (code - 0x2580) as f32 / 8.0, 1.0, 1.0],
            1.0,
            &mut emit,
        ),
        0x2589..=0x258f => geometry.fraction(
            [0.0, 0.0, (0x2590 - code) as f32 / 8.0, 1.0],
            1.0,
            &mut emit,
        ),
        0x2590 => geometry.fraction([0.5, 0.0, 1.0, 1.0], 1.0, &mut emit),
        0x2591..=0x2593 => geometry.fraction(
            [0.0, 0.0, 1.0, 1.0],
            (code - 0x2590) as f32 / 4.0,
            &mut emit,
        ),
        0x2594 => geometry.fraction([0.0, 0.0, 1.0, 0.125], 1.0, &mut emit),
        0x2595 => geometry.fraction([0.875, 0.0, 1.0, 1.0], 1.0, &mut emit),
        0x2596..=0x259f => {
            let quadrants = [4u8, 8, 1, 13, 9, 7, 11, 2, 6, 14][(code - 0x2596) as usize];
            for quadrant in 0..4 {
                if quadrants & (1 << quadrant) != 0 {
                    let x = (quadrant % 2) as f32 * 0.5;
                    let y = (quadrant / 2) as f32 * 0.5;
                    geometry.fraction([x, y, x + 0.5, y + 0.5], 1.0, &mut emit);
                }
            }
        }
        0x2504..=0x250b | 0x254c..=0x254f => {
            let base = if code < 0x254c { 0x2504 } else { 0x254c };
            let axis = ((code - base) / 2 % 2) as usize;
            let count = if base == 0x254c {
                2
            } else if code < 0x2508 {
                3
            } else {
                4
            };
            let thickness = geometry.stroke * if code.is_multiple_of(2) { 1.0 } else { 2.0 };
            let length = geometry.edges[axis + 2] - geometry.edges[axis];
            for dash in 0..count {
                let mut rectangle = [0.0; 4];
                rectangle[axis] =
                    geometry.edges[axis] + length * (2 * dash) as f32 / (2 * count - 1) as f32;
                rectangle[axis + 2] =
                    geometry.edges[axis] + length * (2 * dash + 1) as f32 / (2 * count - 1) as f32;
                rectangle[1 - axis] = geometry.center[1 - axis] - thickness / 2.0;
                rectangle[3 - axis] = geometry.center[1 - axis] + thickness / 2.0;
                geometry.rectangle(rectangle, 1.0, &mut emit);
            }
        }
        0x256d..=0x2570 => {
            let [sx, sy] =
                [[1.0, 1.0], [-1.0, 1.0], [-1.0, -1.0], [1.0, -1.0]][(code - 0x256d) as usize];
            let [cx, cy] = geometry.center;
            let radius = ((geometry.edges[2] - geometry.edges[0])
                .min(geometry.edges[3] - geometry.edges[1])
                - geometry.stroke)
                / 2.0;
            let start = [cx, cy + sy * radius];
            let end = [cx + sx * radius, cy];
            let tangent = radius * (1.0 - 0.552_284_8);
            geometry.line(
                [cx, geometry.edges[if sy > 0.0 { 3 } else { 1 }]],
                start,
                &mut emit,
            );
            emit(CellDrawing::Curve(
                [
                    geometry.point(start[0], start[1]),
                    geometry.point(cx, cy + sy * tangent),
                    geometry.point(cx + sx * tangent, cy),
                    geometry.point(end[0], end[1]),
                ],
                px(geometry.stroke / geometry.scale),
            ));
            geometry.line(
                end,
                [geometry.edges[if sx > 0.0 { 2 } else { 0 }], cy],
                &mut emit,
            );
        }
        0x2571..=0x2573 => {
            let [left, top, right, bottom] = geometry.edges;
            if code != 0x2572 {
                geometry.line([left, bottom], [right, top], &mut emit);
            }
            if code != 0x2571 {
                geometry.line([left, top], [right, bottom], &mut emit);
            }
        }
        0x2500..=0x257f => {
            let packed = BOX_CONNECTIONS[(code - 0x2500) as usize];
            geometry.connections(
                std::array::from_fn(|direction| (packed >> (direction * 2)) & 3),
                &mut emit,
            );
        }
        _ => {}
    }
}

/// SVG masks share GPUI's atlas with icons; colors are applied only at paint time.
pub(super) fn cell_drawing_svg(ch: char, width: u32, height: u32, stroke: u32) -> String {
    let mut svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}' viewBox='0 0 {width} {height}'>"
    );
    let bounds = Bounds::new(
        point(px(0.0), px(0.0)),
        size(px(width as f32), px(height as f32)),
    );
    draw_geometry(
        ch,
        CellGeometry::with_stroke(bounds, 1.0, stroke as f32),
        |drawing| match drawing {
            CellDrawing::Fill(b, alpha) => {
                write!(
                    svg,
                    "<rect x='{}' y='{}' width='{}' height='{}' fill='white' opacity='{alpha}'/>",
                    b.left().as_f32(),
                    b.top().as_f32(),
                    b.size.width.as_f32(),
                    b.size.height.as_f32()
                )
                .unwrap();
            }
            CellDrawing::Line(a, b, width) => {
                write!(
                    svg,
                    "<path d='M {} {} L {} {}' stroke='white' stroke-width='{}' fill='none'/>",
                    a.x.as_f32(),
                    a.y.as_f32(),
                    b.x.as_f32(),
                    b.y.as_f32(),
                    width.as_f32()
                )
                .unwrap();
            }
            CellDrawing::Curve([a, b, c, d], width) => {
                write!(svg, "<path d='M {} {} C {} {}, {} {}, {} {}' stroke='white' stroke-width='{}' fill='none'/>", a.x.as_f32(), a.y.as_f32(), b.x.as_f32(), b.y.as_f32(), c.x.as_f32(), c.y.as_f32(), d.x.as_f32(), d.y.as_f32(), width.as_f32()).unwrap();
            }
        },
    );
    svg.push_str("</svg>");
    svg
}

// Unicode connection semantics: left, right, up, down occupy two bits each.
// 0 = absent, 1 = light/single, 2 = heavy, 3 = double. Special shapes use the branches above.
const BOX_CONNECTIONS: [u8; 128] = [
    0x05, 0x0a, 0x50, 0xa0, 0x00, 0x00, 0x00, 0x00, // U+2500
    0x00, 0x00, 0x00, 0x00, 0x44, 0x48, 0x84, 0x88, // U+2508
    0x41, 0x42, 0x81, 0x82, 0x14, 0x18, 0x24, 0x28, // U+2510
    0x11, 0x12, 0x21, 0x22, 0x54, 0x58, 0x64, 0x94, // U+2518
    0xa4, 0x68, 0x98, 0xa8, 0x51, 0x52, 0x61, 0x91, // U+2520
    0xa1, 0x62, 0x92, 0xa2, 0x45, 0x46, 0x49, 0x4a, // U+2528
    0x85, 0x86, 0x89, 0x8a, 0x15, 0x16, 0x19, 0x1a, // U+2530
    0x25, 0x26, 0x29, 0x2a, 0x55, 0x56, 0x59, 0x5a, // U+2538
    0x65, 0x95, 0xa5, 0x66, 0x69, 0x96, 0x99, 0x6a, // U+2540
    0x9a, 0xa6, 0xa9, 0xaa, 0x00, 0x00, 0x00, 0x00, // U+2548
    0x0f, 0xf0, 0x4c, 0xc4, 0xcc, 0x43, 0xc1, 0xc3, // U+2550
    0x1c, 0x34, 0x3c, 0x13, 0x31, 0x33, 0x5c, 0xf4, // U+2558
    0xfc, 0x53, 0xf1, 0xf3, 0x4f, 0xc5, 0xcf, 0x1f, // U+2560
    0x35, 0x3f, 0x5f, 0xf5, 0xff, 0x00, 0x00, 0x00, // U+2568
    0x00, 0x00, 0x00, 0x00, 0x01, 0x10, 0x04, 0x40, // U+2570
    0x02, 0x20, 0x08, 0x80, 0x09, 0x90, 0x06, 0x60, // U+2578
];

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    fn shapes(ch: char, scale: f32) -> Vec<CellDrawing> {
        let bounds = Bounds::new(point(px(2.25), px(3.5)), size(px(9.5), px(21.0)));
        let mut result = Vec::new();
        draw_cell(ch, bounds, scale, |shape| result.push(shape));
        result
    }

    #[test]
    fn every_box_and_block_character_produces_geometry() {
        for code in 0x2500..=0x259f {
            let ch = char::from_u32(code).unwrap();
            for scale in [1.0, 1.25, 1.5, 2.0] {
                assert!(!shapes(ch, scale).is_empty(), "U+{code:04X} at {scale}");
            }
        }
        assert!(!is_cell_drawing('界'));
        assert!(!is_cell_drawing('A'));
    }

    #[test]
    fn adjacent_horizontal_cells_share_pixel_aligned_edges() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let mut fills = Vec::new();
            for column in 0..2 {
                let bounds = Bounds::new(
                    point(px(2.25 + column as f32 * 9.5), px(3.5)),
                    size(px(9.5), px(21.0)),
                );
                draw_cell('─', bounds, scale, |shape| {
                    if let CellDrawing::Fill(bounds, _) = shape {
                        fills.push(bounds);
                    }
                });
            }
            let right = fills[..2].iter().map(|b| b.right()).max().unwrap();
            let left = fills[2..].iter().map(|b| b.left()).min().unwrap();
            assert_eq!(left, right);
            for bounds in fills {
                for value in [bounds.left(), bounds.top(), bounds.right(), bounds.bottom()] {
                    let device = value.as_f32() * scale;
                    assert!((device - device.round()).abs() < 0.0001);
                }
            }
        }
    }

    #[test]
    fn light_corners_do_not_extend_past_the_joining_stroke() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(16.0), px(24.0)));
        let geometry = CellGeometry::new(bounds, 1.0);
        let mut minimum = [f32::MAX; 2];
        draw_cell('┌', bounds, 1.0, |shape| {
            if let CellDrawing::Fill(rectangle, _) = shape {
                minimum[0] = minimum[0].min(rectangle.left().as_f32());
                minimum[1] = minimum[1].min(rectangle.top().as_f32());
            }
        });
        assert_eq!(
            minimum,
            geometry.center.map(|value| value - geometry.stroke / 2.0)
        );
    }

    #[test]
    fn double_intersections_leave_the_center_open() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(12.0), px(20.0)));
        let center = CellGeometry::new(bounds, 1.0).center;
        let center = point(px(center[0]), px(center[1]));
        for ch in ['╬', '╔', '╗', '╚', '╝'] {
            draw_cell(ch, bounds, 1.0, |shape| {
                if let CellDrawing::Fill(rectangle, _) = shape {
                    assert!(
                        !rectangle.contains(&center),
                        "{ch} closes the double-line channel"
                    );
                }
            });
        }
    }

    #[test]
    fn lower_blocks_and_shades_keep_their_fraction() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(16.0), px(24.0)));
        for eighths in 1..=8 {
            let ch = char::from_u32(0x2580 + eighths).unwrap();
            draw_cell(ch, bounds, 1.0, |shape| {
                let CellDrawing::Fill(rectangle, alpha) = shape else {
                    panic!("block must fill")
                };
                assert_eq!(rectangle.size.height, px(eighths as f32 * 3.0));
                assert_eq!(rectangle.bottom(), bounds.bottom());
                assert_eq!(alpha, 1.0);
            });
        }
        for (ch, expected) in [('░', 0.25), ('▒', 0.5), ('▓', 0.75)] {
            draw_cell(ch, bounds, 1.0, |shape| {
                let CellDrawing::Fill(rectangle, alpha) = shape else {
                    panic!("shade must fill")
                };
                assert_eq!(rectangle, bounds);
                assert_eq!(alpha, expected);
            });
        }
    }
}
