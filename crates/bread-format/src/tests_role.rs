/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::role::{
    DecodeRolesError, EncodeRolesError, ROLE_RAFT, ROLE_SUPPORTS, RoleEntry, decode_roles,
    encode_roles,
};
use crate::tests_scene::{scene, u32s};

fn role(leaf: usize, role: u8) -> RoleEntry {
    RoleEntry { leaf, role }
}

#[test]
fn entries_come_back_identical() {
    let scene = scene();
    for entries in [
        vec![],
        vec![role(2, ROLE_SUPPORTS)],
        vec![
            role(1, ROLE_RAFT),
            role(2, ROLE_SUPPORTS),
            role(4, ROLE_SUPPORTS),
        ],
    ] {
        let payload = encode_roles(&entries, &scene).unwrap();
        assert_eq!(decode_roles(&payload, &scene).unwrap(), entries);
    }
}

#[test]
fn each_entry_takes_five_bytes() {
    let mut expected = u32s(&[2, 1]);
    expected.push(ROLE_RAFT);
    expected.extend(u32s(&[4]));
    expected.push(ROLE_SUPPORTS);
    let entries = [role(1, ROLE_RAFT), role(4, ROLE_SUPPORTS)];
    assert_eq!(encode_roles(&entries, &scene()).unwrap(), expected);
}

#[test]
fn reserved_roles_are_read_and_carried_but_not_written() {
    let scene = scene();
    for reserved in [0, 3, 255] {
        let entry = role(1, reserved);
        assert!(!entry.is_known());
        assert_eq!(
            encode_roles(&[entry], &scene),
            Err(EncodeRolesError::ReservedRole)
        );
        let mut payload = encode_roles(&[role(1, ROLE_RAFT)], &scene).unwrap();
        payload[8] = reserved;
        assert_eq!(decode_roles(&payload, &scene).unwrap(), [entry]);
    }
}

#[test]
fn an_entry_must_name_a_leaf() {
    let scene = scene();
    // Node 3 is a group, node 7 a reference, and there is no node 9.
    for leaf in [3, 7, 9] {
        assert_eq!(
            encode_roles(&[role(leaf, ROLE_RAFT)], &scene),
            Err(EncodeRolesError::NotALeaf),
            "node {leaf}"
        );
        let mut payload = encode_roles(&[role(0, ROLE_RAFT)], &scene).unwrap();
        payload[4] = leaf as u8;
        assert_eq!(
            decode_roles(&payload, &scene),
            Err(DecodeRolesError::NotALeaf),
            "node {leaf}"
        );
    }
}

#[test]
fn two_entries_may_not_name_the_same_leaf() {
    let scene = scene();
    let twice = [role(1, ROLE_RAFT), role(1, ROLE_SUPPORTS)];
    assert_eq!(
        encode_roles(&twice, &scene),
        Err(EncodeRolesError::SameLeaf)
    );
    let mut payload = encode_roles(&twice[..1], &scene).unwrap();
    payload[..4].copy_from_slice(&2u32.to_le_bytes());
    payload.extend([1, 0, 0, 0, ROLE_SUPPORTS]);
    assert_eq!(
        decode_roles(&payload, &scene),
        Err(DecodeRolesError::SameLeaf)
    );
}

#[test]
fn a_cut_short_or_padded_payload_is_malformed() {
    let scene = scene();
    let payload = encode_roles(&[role(1, ROLE_RAFT), role(2, ROLE_SUPPORTS)], &scene).unwrap();
    for cut in 0..payload.len() {
        assert_eq!(
            decode_roles(&payload[..cut], &scene),
            Err(DecodeRolesError::Malformed),
            "cut at {cut}"
        );
    }
    let mut padded = payload;
    padded.push(0);
    assert_eq!(
        decode_roles(&padded, &scene),
        Err(DecodeRolesError::Malformed)
    );
}

#[test]
fn a_huge_count_is_malformed_before_any_allocation() {
    assert_eq!(
        decode_roles(&u32s(&[u32::MAX]), &scene()),
        Err(DecodeRolesError::Malformed)
    );
}
