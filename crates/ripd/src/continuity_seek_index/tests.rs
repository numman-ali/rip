use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use rip_kernel::{Event, EventKind};
use tempfile::tempdir;
use uuid::Uuid;

use super::*;

fn continuity_event(continuity_id: &str, seq: u64, id: &str, kind: EventKind) -> Event {
    Event {
        id: id.to_string(),
        session_id: continuity_id.to_string(),
        timestamp_ms: 0,
        seq,
        kind,
    }
}

fn message_event(continuity_id: &str, seq: u64, id: &str) -> Event {
    continuity_event(
        continuity_id,
        seq,
        id,
        EventKind::ContinuityMessageAppended {
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
            content: format!("message {seq}"),
        },
    )
}

fn run_event(continuity_id: &str, seq: u64) -> Event {
    continuity_event(
        continuity_id,
        seq,
        &format!("run-{seq}"),
        EventKind::ContinuityRunEnded {
            run_session_id: format!("run-{seq}"),
            message_id: format!("m{seq}"),
            reason: "done".to_string(),
            actor_id: None,
            origin: None,
        },
    )
}

fn write_events_jsonl(path: &Path, events: &[Event]) -> Vec<u64> {
    let mut out = String::new();
    let mut offsets = Vec::new();
    let mut offset = 0u64;
    for event in events {
        offsets.push(offset);
        let line = serde_json::to_string(event).expect("json");
        out.push_str(&line);
        out.push('\n');
        offset = offset.saturating_add(line.len() as u64 + 1);
    }
    fs::write(path, out).expect("write");
    offsets
}

#[test]
fn load_seq_index_and_best_offset_cover_validation_paths() {
    let dir = tempdir().expect("tmp");
    let path = dir.path().join("seek.jsonl");

    assert!(load_seq_index_v1(&path).expect("missing").is_none());

    let entry0 = SeqSeekIndexEntryV1::new(0, 0);
    let entry1 = SeqSeekIndexEntryV1::new(256, 100);
    fs::write(
        &path,
        format!(
            "{}\n\n{}\n",
            serde_json::to_string(&entry0).expect("json"),
            serde_json::to_string(&entry1).expect("json"),
        ),
    )
    .expect("write");
    let entries = load_seq_index_v1(&path).expect("load").expect("entries");
    assert_eq!(entries.len(), 2);
    assert_eq!(best_offset_for_seq(&entries, 0), 0);
    assert_eq!(best_offset_for_seq(&entries, 42), 0);
    assert_eq!(best_offset_for_seq(&entries, 256), 100);
    assert_eq!(best_offset_for_seq(&entries, 999), 100);

    fs::write(&path, "\n\n").expect("write");
    let err = load_seq_index_v1(&path).expect_err("empty");
    assert!(err.to_string().contains("seek index is empty"));

    let mut bad_version = SeqSeekIndexEntryV1::new(0, 0);
    bad_version.version = 2;
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string(&bad_version).expect("json")),
    )
    .expect("write");
    let err = load_seq_index_v1(&path).expect_err("version");
    assert!(err.to_string().contains("version mismatch"));

    fs::write(
        &path,
        format!(
            "{}\n{}\n",
            serde_json::to_string(&SeqSeekIndexEntryV1::new(10, 20)).expect("json"),
            serde_json::to_string(&SeqSeekIndexEntryV1::new(9, 21)).expect("json"),
        ),
    )
    .expect("write");
    let err = load_seq_index_v1(&path).expect_err("seq mismatch");
    assert!(err.to_string().contains("seq is not monotonic"));

    fs::write(
        &path,
        format!(
            "{}\n{}\n",
            serde_json::to_string(&SeqSeekIndexEntryV1::new(10, 20)).expect("json"),
            serde_json::to_string(&SeqSeekIndexEntryV1::new(11, 19)).expect("json"),
        ),
    )
    .expect("write");
    let err = load_seq_index_v1(&path).expect_err("offset mismatch");
    assert!(err.to_string().contains("offset is not monotonic"));
}

#[test]
fn validate_seq_index_against_sidecar_detects_mismatches() {
    let dir = tempdir().expect("tmp");
    let path = dir.path().join("events.jsonl");
    let continuity_id = "c1";
    let events = vec![
        message_event(continuity_id, 0, "00000000-0000-0000-0000-000000000001"),
        message_event(continuity_id, 256, "00000000-0000-0000-0000-000000000002"),
    ];
    let offsets = write_events_jsonl(&path, &events);

    let entries = vec![
        SeqSeekIndexEntryV1::new(0, offsets[0]),
        SeqSeekIndexEntryV1::new(256, offsets[1]),
    ];
    validate_seq_index_against_sidecar(&entries, &path, continuity_id).expect("valid");

    let err = validate_seq_index_against_sidecar(&[], &path, continuity_id).expect_err("empty");
    assert!(err.to_string().contains("seek index empty"));

    let bad_offset = vec![SeqSeekIndexEntryV1::new(
        256,
        fs::metadata(&path).expect("meta").len(),
    )];
    let err = validate_seq_index_against_sidecar(&bad_offset, &path, continuity_id)
        .expect_err("past end");
    assert!(err.to_string().contains("past end"));

    let mismatched = vec![SeqSeekIndexEntryV1::new(255, offsets[1])];
    let err = validate_seq_index_against_sidecar(&mismatched, &path, continuity_id)
        .expect_err("mismatch");
    assert!(err.to_string().contains("does not match"));
}

#[test]
fn rebuild_seq_index_from_sidecar_handles_valid_and_invalid_streams() {
    let dir = tempdir().expect("tmp");
    let sidecar = dir.path().join("events.jsonl");
    let index = dir.path().join("seek.jsonl");
    let continuity_id = "c1";

    let events = (0..=260)
        .map(|seq| message_event(continuity_id, seq, &Uuid::new_v4().to_string()))
        .collect::<Vec<_>>();
    write_events_jsonl(&sidecar, &events);
    rebuild_seq_index_from_sidecar_v1(&sidecar, &index).expect("rebuild");
    let entries = load_seq_index_v1(&index).expect("load").expect("entries");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].seq, 0);
    assert_eq!(entries[1].seq, 256);

    write_events_jsonl(
        &sidecar,
        &[
            message_event(continuity_id, 0, &Uuid::new_v4().to_string()),
            message_event(continuity_id, 2, &Uuid::new_v4().to_string()),
        ],
    );
    let err = rebuild_seq_index_from_sidecar_v1(&sidecar, &index).expect_err("gap");
    assert!(err.to_string().contains("seq mismatch"));

    let session = Event {
        id: "s0".to_string(),
        session_id: "run-1".to_string(),
        timestamp_ms: 0,
        seq: 0,
        kind: EventKind::SessionStarted {
            input: "hi".to_string(),
        },
    };
    write_events_jsonl(&sidecar, &[session]);
    let err = rebuild_seq_index_from_sidecar_v1(&sidecar, &index).expect_err("stream kind");
    assert!(err.to_string().contains("non-continuity"));
}

#[test]
fn message_index_round_trips_and_recovers_from_corruption() {
    let dir = tempdir().expect("tmp");
    let sidecar = dir.path().join("events.jsonl");
    let index = dir.path().join("messages.bin");
    let continuity_id = "c1";
    let id1 = "00000000-0000-0000-0000-000000000001";
    let id2 = "00000000-0000-0000-0000-000000000002";

    let events = vec![
        message_event(continuity_id, 0, id1),
        run_event(continuity_id, 1),
        message_event(continuity_id, 2, id2),
    ];
    let offsets = write_events_jsonl(&sidecar, &events);
    rebuild_message_index_from_sidecar_v1(&sidecar, &index).expect("rebuild");

    assert_eq!(
        lookup_message_v1(&index, id1).expect("lookup"),
        Some((0, offsets[0]))
    );
    assert_eq!(
        lookup_message_v1(&index, id2).expect("lookup"),
        Some((2, offsets[2]))
    );
    assert_eq!(
        lookup_message_v1(&index, "not-a-uuid").expect("invalid"),
        None
    );
    assert_eq!(
        lookup_message_v1(&dir.path().join("missing.bin"), id1).expect("missing"),
        None
    );

    insert_message_best_effort_v1(&index, &sidecar, id1, 5, 123);
    assert_eq!(
        lookup_message_v1(&index, id1).expect("lookup"),
        Some((5, 123))
    );

    fs::write(&index, b"corrupt").expect("corrupt");
    insert_message_best_effort_v1(&index, &sidecar, id2, 2, offsets[2]);
    assert_eq!(
        lookup_message_v1(&index, id2).expect("lookup"),
        Some((2, offsets[2]))
    );

    insert_message_best_effort_v1(&index, &sidecar, "not-a-uuid", 1, 1);
    assert_eq!(
        lookup_message_v1(&index, "not-a-uuid").expect("lookup"),
        None
    );
}

#[test]
fn message_index_helpers_cover_growth_collisions_and_blank_sidecars() {
    let dir = tempdir().expect("tmp");
    let continuity_id = "c1";

    assert_eq!(next_power_of_two_u64(0), 1);
    assert_eq!(next_power_of_two_u64(17), 32);

    let index = dir.path().join("collisions.bin");
    create_empty_msg_index(&index, 2).expect("create");

    let mut same_bucket: Vec<Uuid> = Vec::new();
    for raw in 1..128u128 {
        let uuid = Uuid::from_u128(raw);
        if hash_uuid_v1(uuid.as_bytes()) & 1 == 0 {
            same_bucket.push(uuid);
            if same_bucket.len() == 3 {
                break;
            }
        }
    }
    assert_eq!(same_bucket.len(), 3, "expected colliding uuids");

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&index)
        .expect("open");
    let mut header = read_msg_index_header(&mut file).expect("header");
    let key1 = same_bucket[0].into_bytes();
    let key2 = same_bucket[1].into_bytes();
    let key3 = same_bucket[2].into_bytes();

    msg_index_insert_v1(&mut file, &mut header, &key1, 1, 11).expect("insert 1");
    msg_index_insert_v1(&mut file, &mut header, &key2, 2, 22).expect("insert 2");
    msg_index_insert_v1(&mut file, &mut header, &key1, 3, 33).expect("update 1");
    write_msg_index_header(&mut file, header).expect("persist header");
    drop(file);

    assert_eq!(
        lookup_message_v1(&index, &same_bucket[0].to_string()).expect("lookup"),
        Some((3, 33))
    );
    assert_eq!(
        lookup_message_v1(&index, &same_bucket[1].to_string()).expect("lookup"),
        Some((2, 22))
    );

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&index)
        .expect("open");
    let mut header = read_msg_index_header(&mut file).expect("header");
    let err = msg_index_insert_v1(&mut file, &mut header, &key3, 4, 44).expect_err("full");
    assert!(err.to_string().contains("message index is full"));

    write_msg_index_header(
        &mut file,
        MsgIndexHeader {
            capacity: 3,
            len: 0,
        },
    )
    .expect("bad header");
    drop(file);
    let err = lookup_message_v1(&index, &same_bucket[0].to_string()).expect_err("pow2");
    assert!(err.to_string().contains("power of two"));

    let sidecar = dir.path().join("events.jsonl");
    let valid_id = "00000000-0000-0000-0000-0000000000aa";
    let valid_line =
        serde_json::to_string(&message_event(continuity_id, 0, valid_id)).expect("json");
    let run_line = serde_json::to_string(&run_event(continuity_id, 1)).expect("json");
    let invalid_line =
        serde_json::to_string(&message_event(continuity_id, 2, "not-a-uuid")).expect("json");
    fs::write(
        &sidecar,
        format!("\n{valid_line}\n{run_line}\n{invalid_line}\n"),
    )
    .expect("write sidecar");

    let rebuilt = dir.path().join("rebuilt.bin");
    rebuild_message_index_from_sidecar_v1(&sidecar, &rebuilt).expect("rebuild");
    assert_eq!(
        lookup_message_v1(&rebuilt, valid_id).expect("lookup"),
        Some((0, 1))
    );
    assert_eq!(
        lookup_message_v1(&rebuilt, "not-a-uuid").expect("lookup"),
        None
    );

    let grow_index = dir.path().join("grow.bin");
    create_empty_msg_index(&grow_index, 8).expect("grow create");
    let mut grow_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&grow_index)
        .expect("open grow");
    write_msg_index_header(
        &mut grow_file,
        MsgIndexHeader {
            capacity: 8,
            len: 6,
        },
    )
    .expect("write grow header");
    drop(grow_file);
    insert_message_best_effort_v1(&grow_index, &sidecar, valid_id, 0, 1);
    assert_eq!(
        lookup_message_v1(&grow_index, valid_id).expect("lookup"),
        Some((0, 1))
    );
}

#[test]
fn sidecar_index_builder_handles_empty_and_invalid_message_ids() {
    let dir = tempdir().expect("tmp");
    let continuity_id = "c-empty";

    SidecarIndexBuilderV1::new()
        .write_best_effort(dir.path(), continuity_id)
        .expect("empty write");
    assert!(
        !seq_index_path(dir.path(), continuity_id).exists(),
        "empty builder should not write indexes"
    );

    let mut builder = SidecarIndexBuilderV1::new();
    builder.observe_event(&message_event(continuity_id, 0, "not-a-uuid"), 0);
    builder
        .write_best_effort(dir.path(), "invalid-id")
        .expect("write");
    let seek_entries = load_seq_index_v1(&seq_index_path(dir.path(), "invalid-id"))
        .expect("load")
        .expect("entries");
    assert_eq!(seek_entries.len(), 1);
    assert!(
        lookup_message_v1(&message_index_path(dir.path(), "invalid-id"), "not-a-uuid")
            .expect("lookup")
            .is_none()
    );
}

#[test]
fn sidecar_index_builder_writes_indexes_for_observed_events() {
    let dir = tempdir().expect("tmp");
    let continuity_id = "c1";
    let id1 = Uuid::new_v4().to_string();
    let id2 = Uuid::new_v4().to_string();

    let mut builder = SidecarIndexBuilderV1::new();
    builder.observe_event(&message_event(continuity_id, 0, &id1), 0);
    builder.observe_event(&run_event(continuity_id, 1), 100);
    builder.observe_event(&message_event(continuity_id, 2, &id2), 200);
    builder.observe_event(
        &Event {
            id: "s0".to_string(),
            session_id: "run-1".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind: EventKind::SessionStarted {
                input: "ignored".to_string(),
            },
        },
        300,
    );
    builder
        .write_best_effort(dir.path(), continuity_id)
        .expect("write");

    let seek_entries = load_seq_index_v1(&seq_index_path(dir.path(), continuity_id))
        .expect("load")
        .expect("entries");
    assert_eq!(seek_entries.len(), 1);
    assert_eq!(seek_entries[0].seq, 0);

    let msg_path = message_index_path(dir.path(), continuity_id);
    assert_eq!(
        lookup_message_v1(&msg_path, &id1).expect("lookup"),
        Some((0, 0))
    );
    assert_eq!(
        lookup_message_v1(&msg_path, &id2).expect("lookup"),
        Some((2, 200))
    );
}

#[test]
fn helper_paths_and_rebuilds_cover_remaining_message_index_edges() {
    let dir = tempdir().expect("tmp");
    let continuity_id = "c-helper";
    let entry = SeqSeekIndexEntryV1::new(0, 0);

    let blocked_parent = dir.path().join("blocked-parent");
    fs::write(&blocked_parent, b"nope").expect("write parent file");
    append_seq_index_entry_best_effort(&blocked_parent.join("seek.jsonl"), &entry);
    assert!(!blocked_parent.join("seek.jsonl").exists());

    let dir_target = dir.path().join("seek-dir");
    fs::create_dir_all(&dir_target).expect("dir target");
    append_seq_index_entry_best_effort(&dir_target, &entry);
    assert!(dir_target.is_dir());

    let nested_dir = dir.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("nested dir");
    let nested_sidecar = nested_dir.join("events.jsonl");
    let event0 = serde_json::to_string(&message_event(
        continuity_id,
        0,
        &Uuid::new_v4().to_string(),
    ))
    .expect("json");
    let event1 = serde_json::to_string(&message_event(
        continuity_id,
        1,
        &Uuid::new_v4().to_string(),
    ))
    .expect("json");
    fs::write(&nested_sidecar, format!("\n{event0}\n\r\n{event1}\n")).expect("write sidecar");
    let nested_index = nested_dir.join("out").join("seek.jsonl");
    rebuild_seq_index_from_sidecar_v1(&nested_sidecar, &nested_index).expect("rebuild");
    let rebuilt_entries = load_seq_index_v1(&nested_index)
        .expect("load")
        .expect("entries");
    assert_eq!(rebuilt_entries.len(), 1);

    let bad_magic = dir.path().join("bad-magic.bin");
    create_empty_msg_index(&bad_magic, 4).expect("create magic");
    let mut bad_magic_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&bad_magic)
        .expect("open magic");
    bad_magic_file.seek(SeekFrom::Start(0)).expect("seek");
    bad_magic_file.write_all(b"BADMAGIC").expect("write magic");
    drop(bad_magic_file);
    let err = lookup_message_v1(&bad_magic, &Uuid::new_v4().to_string()).expect_err("magic");
    assert!(err.to_string().contains("magic mismatch"));

    let bad_version = dir.path().join("bad-version.bin");
    create_empty_msg_index(&bad_version, 4).expect("create version");
    let mut bad_version_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&bad_version)
        .expect("open version");
    bad_version_file.seek(SeekFrom::Start(8)).expect("seek");
    bad_version_file
        .write_all(&2u32.to_le_bytes())
        .expect("write version");
    drop(bad_version_file);
    let err = lookup_message_v1(&bad_version, &Uuid::new_v4().to_string()).expect_err("version");
    assert!(err.to_string().contains("version mismatch"));

    let zero_capacity = dir.path().join("zero-capacity.bin");
    create_empty_msg_index(&zero_capacity, 4).expect("create zero");
    let mut zero_capacity_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&zero_capacity)
        .expect("open zero");
    write_msg_index_header(
        &mut zero_capacity_file,
        MsgIndexHeader {
            capacity: 0,
            len: 0,
        },
    )
    .expect("write zero cap");
    drop(zero_capacity_file);
    let err = lookup_message_v1(&zero_capacity, &Uuid::new_v4().to_string()).expect_err("zero cap");
    assert!(err.to_string().contains("capacity is zero"));

    let bad_insert = dir.path().join("bad-insert.bin");
    create_empty_msg_index(&bad_insert, 4).expect("create bad insert");
    let mut bad_insert_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&bad_insert)
        .expect("open bad insert");
    let mut bad_header = MsgIndexHeader {
        capacity: 3,
        len: 0,
    };
    let bad_key = Uuid::new_v4().into_bytes();
    let err = msg_index_insert_v1(&mut bad_insert_file, &mut bad_header, &bad_key, 1, 10)
        .expect_err("invalid capacity");
    assert!(err.to_string().contains("power of two"));

    let rebuild_sidecar = dir.path().join("rebuild-events.jsonl");
    let rebuild_id = "00000000-0000-0000-0000-0000000000aa";
    let rebuild_events = vec![
        message_event(continuity_id, 0, rebuild_id),
        run_event(continuity_id, 1),
    ];
    let rebuild_offsets = write_events_jsonl(&rebuild_sidecar, &rebuild_events);
    let rebuild_index = dir.path().join("rebuild-index.bin");
    create_empty_msg_index(&rebuild_index, 4).expect("create rebuild index");
    let mut rebuild_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&rebuild_index)
        .expect("open rebuild index");
    write_msg_index_header(
        &mut rebuild_file,
        MsgIndexHeader {
            capacity: 3,
            len: 0,
        },
    )
    .expect("bad cap header");
    drop(rebuild_file);
    insert_message_best_effort_v1(
        &rebuild_index,
        &rebuild_sidecar,
        rebuild_id,
        0,
        rebuild_offsets[0],
    );
    assert_eq!(
        lookup_message_v1(&rebuild_index, rebuild_id).expect("rebuilt lookup"),
        Some((0, rebuild_offsets[0]))
    );

    let full_sidecar = dir.path().join("full-events.jsonl");
    let mut same_bucket: Vec<Uuid> = Vec::new();
    for raw in 1..128u128 {
        let uuid = Uuid::from_u128(raw);
        if hash_uuid_v1(uuid.as_bytes()) & 1 == 0 {
            same_bucket.push(uuid);
            if same_bucket.len() == 3 {
                break;
            }
        }
    }
    assert_eq!(same_bucket.len(), 3, "expected colliding uuids");
    let full_events = vec![
        message_event(continuity_id, 0, &same_bucket[0].to_string()),
        message_event(continuity_id, 1, &same_bucket[1].to_string()),
        message_event(continuity_id, 2, &same_bucket[2].to_string()),
    ];
    let full_offsets = write_events_jsonl(&full_sidecar, &full_events);
    let full_index = dir.path().join("full-index.bin");
    create_empty_msg_index(&full_index, 2).expect("create full index");
    let mut full_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&full_index)
        .expect("open full index");
    let mut full_header = read_msg_index_header(&mut full_file).expect("header");
    msg_index_insert_v1(
        &mut full_file,
        &mut full_header,
        &same_bucket[0].into_bytes(),
        0,
        full_offsets[0],
    )
    .expect("insert first");
    msg_index_insert_v1(
        &mut full_file,
        &mut full_header,
        &same_bucket[1].into_bytes(),
        1,
        full_offsets[1],
    )
    .expect("insert second");
    write_msg_index_header(&mut full_file, full_header).expect("persist full header");
    drop(full_file);

    insert_message_best_effort_v1(
        &full_index,
        &full_sidecar,
        &same_bucket[2].to_string(),
        2,
        full_offsets[2],
    );
    assert_eq!(
        lookup_message_v1(&full_index, &same_bucket[2].to_string()).expect("full rebuild"),
        Some((2, full_offsets[2]))
    );
}
