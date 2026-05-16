#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparkline_points_match_tauri_polyline_mapping() {
        let points =
            sparkline_polyline_points(&[Some(10.0), None, Some(20.0), Some(5.0)], 100.0, 28.0);

        assert_eq!(points.len(), 3);
        assert_point_close(points[0], (0.0, 14.7));
        assert_point_close(points[1], (50.0, 2.8));
        assert_point_close(points[2], (100.0, 20.65));
    }

    fn assert_point_close(actual: (f32, f32), expected: (f32, f32)) {
        assert!((actual.0 - expected.0).abs() < 0.001);
        assert!((actual.1 - expected.1).abs() < 0.001);
    }
}
