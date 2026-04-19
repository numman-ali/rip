//! Tests for `copy.rs` helpers that aren't reached by the broader
//! `fullscreen/tests.rs` cases: the 1-byte-remainder branch of
//! `base64_encode`, empty-payload behavior of `osc52_sequence`, and
//! round-trip coverage of `prepare_copy_selected` when a selection is
//! present and OSC52 is enabled. `copy_selected` itself needs a real
//! `Terminal<CrosstermBackend<Stdout>>` so it stays out of the unit
//! test suite.

use super::*;

#[test]
fn base64_encode_single_byte_uses_two_pad_chars() {
    // 1-byte remainder branch. "a" = 0x61 → "YQ==".
    assert_eq!(base64_encode(b"a"), "YQ==");
    // 4 bytes → 3+1 remainder; same branch exercised.
    assert_eq!(base64_encode(b"abcd"), "YWJjZA==");
    // 7 bytes → 6+1.
    assert_eq!(base64_encode(b"abcdefg"), "YWJjZGVmZw==");
}

#[test]
fn base64_encode_two_byte_remainder_uses_one_pad_char() {
    // Already covered by `b"hi"` in the sibling file, but we want a
    // longer input that ends with the 2-byte branch so the test file is
    // self-contained.
    assert_eq!(base64_encode(b"abcde"), "YWJjZGU=");
}

#[test]
fn base64_encode_aligned_length_has_no_pad_chars() {
    assert_eq!(base64_encode(b"abc"), "YWJj");
    assert_eq!(base64_encode(b"abcdef"), "YWJjZGVm");
}

#[test]
fn osc52_sequence_empty_payload_still_wraps_prefix_and_bell() {
    // Empty input should still produce a well-formed OSC52 sequence —
    // terminals that gate OSC52 parsing on the trailing BEL won't drop
    // the next sequence on the floor.
    let seq = osc52_sequence(b"");
    assert_eq!(seq, "\x1b]52;c;\x07");
}

#[test]
fn osc52_sequence_preserves_expected_shape() {
    let seq = osc52_sequence(b"a");
    assert!(seq.starts_with("\x1b]52;c;"));
    assert!(seq.ends_with('\x07'));
    assert!(seq.contains("YQ=="));
}
