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

    #[test]
    fn host_tools_connection_row_only_enables_switching_when_possible() {
        assert!(!monitor_connection_can_switch(&connection_options(0)));
        assert!(!monitor_connection_can_switch(&connection_options(1)));
        assert!(monitor_connection_can_switch(&connection_options(2)));
    }

    #[test]
    fn host_process_table_merges_user_column_until_sidebar_is_wide_enough() {
        assert!(!host_process_table_uses_separate_user_column(
            HOST_PROCESS_SEPARATE_USER_COLUMN_MIN_WIDTH - 1.0
        ));
        assert!(host_process_table_uses_separate_user_column(
            HOST_PROCESS_SEPARATE_USER_COLUMN_MIN_WIDTH
        ));
    }

    #[test]
    fn connection_monitor_surface_bg_is_transparent_when_background_is_active() {
        let theme_bg = 0x112233;
        assert_eq!(
            connection_monitor_surface_bg(theme_bg, true),
            rgba(0x00000000)
        );
        assert_eq!(
            connection_monitor_surface_bg(theme_bg, false),
            rgb(theme_bg)
        );
    }

    fn connection_options(count: usize) -> Vec<MonitorConnectionOption> {
        (0..count)
            .map(|index| MonitorConnectionOption {
                connection_id: format!("conn-{index}"),
                host: format!("host-{index}"),
                port: 22,
                username: "user".to_string(),
            })
            .collect()
    }

    fn assert_point_close(actual: (f32, f32), expected: (f32, f32)) {
        assert!((actual.0 - expected.0).abs() < 0.001);
        assert!((actual.1 - expected.1).abs() < 0.001);
    }
}
