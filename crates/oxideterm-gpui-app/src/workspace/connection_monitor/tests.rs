use super::*;

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
