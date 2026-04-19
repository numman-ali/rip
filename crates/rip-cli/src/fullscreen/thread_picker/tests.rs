//! Unit tests for the pure helpers in `thread_picker.rs`:
//! `build_entries`, `short_thread_label`, and `relative_age_chip`.

use super::*;

fn meta(thread_id: &str, created_at_ms: u64) -> ThreadMetaResponse {
    ThreadMetaResponse {
        thread_id: thread_id.to_string(),
        created_at_ms,
        title: None,
        archived: false,
    }
}

#[test]
fn short_label_returns_id_when_short_enough() {
    assert_eq!(short_thread_label("thread-abc"), "thread-abc");
    let exactly_twenty = "thread_1234567890abc";
    assert_eq!(exactly_twenty.chars().count(), 20);
    assert_eq!(short_thread_label(exactly_twenty), exactly_twenty);
}

#[test]
fn short_label_truncates_long_ids_to_ellipsis_plus_last_twelve() {
    let long_id = "thread_0123456789abcdef_0123456789ab";
    assert_eq!(long_id.chars().count(), 36);
    let label = short_thread_label(long_id);
    assert_eq!(label, "…0123456789ab");
    assert!(label.starts_with('…'));
    let after_ellipsis: String = label.chars().skip(1).collect();
    assert_eq!(after_ellipsis.chars().count(), 12);
}

#[test]
fn short_label_handles_multibyte_without_panicking() {
    let mixed = "thread_abc_日本語_tail_end_12345";
    let label = short_thread_label(mixed);
    assert!(label.starts_with('…'));
}

#[test]
fn relative_age_chip_covers_all_boundaries() {
    let minute = 60_000u64;
    let hour = 60 * minute;
    let day = 24 * hour;
    assert_eq!(relative_age_chip(0, 0), "now");
    assert_eq!(relative_age_chip(30_000, 0), "now");
    assert_eq!(relative_age_chip(minute, 0), "1m");
    assert_eq!(relative_age_chip(59 * minute, 0), "59m");
    assert_eq!(relative_age_chip(hour, 0), "1h");
    assert_eq!(relative_age_chip(23 * hour + 59 * minute, 0), "23h");
    assert_eq!(relative_age_chip(day, 0), "1d");
    assert_eq!(relative_age_chip(10 * day, 0), "10d");
}

#[test]
fn relative_age_chip_saturates_when_created_after_now() {
    assert_eq!(relative_age_chip(1_000, 60_000), "now");
}

#[test]
fn build_entries_pins_current_thread_first() {
    let threads = vec![
        meta("t-old", 1_000),
        meta("t-current", 500),
        meta("t-new", 2_000),
    ];
    let entries = build_entries(threads, Some("t-current"), 10_000);
    assert_eq!(
        entries
            .iter()
            .map(|e| e.thread_id.as_str())
            .collect::<Vec<_>>(),
        vec!["t-current", "t-new", "t-old"],
    );
    assert_eq!(
        entries[0].chips.first().map(String::as_str),
        Some("current")
    );
    assert!(entries[1]
        .chips
        .iter()
        .all(|chip| chip.as_str() != "current"));
}

#[test]
fn build_entries_sorts_by_newest_when_no_current() {
    let threads = vec![meta("t1", 1_000), meta("t2", 3_000), meta("t3", 2_000)];
    let entries = build_entries(threads, None, 10_000);
    assert_eq!(
        entries
            .iter()
            .map(|e| e.thread_id.as_str())
            .collect::<Vec<_>>(),
        vec!["t2", "t3", "t1"],
    );
    assert!(entries
        .iter()
        .all(|entry| entry.chips.iter().all(|chip| chip.as_str() != "current")));
}

#[test]
fn build_entries_uses_title_when_present_and_short_label_otherwise() {
    let mut with_title = meta("t1", 1_000);
    with_title.title = Some("Launch plan".to_string());
    let long = meta("thread_0123456789abcdef_0123456789ab", 2_000);
    // `long` is newer, so it comes first.
    let entries = build_entries(vec![with_title, long], None, 10_000);
    assert_eq!(entries[0].title, "…0123456789ab");
    assert_eq!(entries[1].title, "Launch plan");
}

#[test]
fn build_entries_adds_archived_chip_when_set() {
    let mut archived = meta("t-arch", 1_000);
    archived.archived = true;
    let entries = build_entries(vec![archived], None, 10_000);
    assert!(entries[0].chips.iter().any(|chip| chip == "archived"));
}

#[test]
fn build_entries_includes_placeholder_chips_for_unsupported_capabilities() {
    let entries = build_entries(vec![meta("t1", 1_000)], None, 10_000);
    let chips = &entries[0].chips;
    assert!(chips.iter().any(|chip| chip.starts_with("age ")));
    assert!(chips.iter().any(|chip| chip == "size —"));
    assert!(chips.iter().any(|chip| chip == "actors —"));
}

#[test]
fn build_entries_age_chip_reflects_elapsed_time() {
    let now = 10u64 * 24 * 60 * 60 * 1000;
    let created = now - 3 * 60 * 60 * 1000;
    let entries = build_entries(vec![meta("t1", created)], None, now);
    assert!(entries[0].chips.iter().any(|chip| chip == "age 3h"));
}
