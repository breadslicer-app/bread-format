/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! What `bread info` says about who made a file and what its leaves are for:
//! the creator's credit, the roles (SPEC.md 11) and the metadata (SPEC.md 12).

use std::fmt::Write as _;

use bread_format::{BreadFile, MetadataEntry, ROLE_RAFT, ROLE_SUPPORTS};

/// The metadata keys of the credit, in the order `bread info` shows them,
/// before anything else.
const CREDIT: [&str; 4] = ["title", "designer", "license", "source"];

/// Bytes of a program's own value shown, as hex, before the rest is cut.
const SHOWN_BYTES: usize = 16;

/// Who made the file, as the metadata about the whole file says: its title,
/// designer, license and source, those it has. Empty when it has none.
pub(crate) fn credit(file: &BreadFile) -> Vec<String> {
    CREDIT
        .iter()
        .filter_map(|&key| {
            let text = file
                .metadata
                .iter()
                .find(|entry| entry.node.is_none() && entry.key == key)?
                .as_text()?;
            Some(format!("{:<10}{}", format!("{key}:"), printable(text)))
        })
        .collect()
}

/// The leaves that are not models, each with what it shows: those the role
/// chunk names, and the support leaves of the supports entries, which show
/// supports whatever the role chunk says. Empty when every leaf is a model.
pub(crate) fn roles(file: &BreadFile) -> Vec<String> {
    let mut leaves: Vec<(usize, String)> = file
        .roles
        .iter()
        .map(|entry| {
            let role = match entry.role {
                ROLE_SUPPORTS => "supports".to_owned(),
                ROLE_RAFT => "a raft".to_owned(),
                other => format!("a model (role {other} is reserved)"),
            };
            (entry.leaf, role)
        })
        .collect();
    for entry in &file.supports {
        if !leaves.iter().any(|(leaf, _)| *leaf == entry.support) {
            leaves.push((entry.support, "supports, for its supports entry".into()));
        }
    }
    if leaves.is_empty() {
        return Vec::new();
    }
    leaves.sort_by_key(|(leaf, _)| *leaf);
    let mut lines = vec!["roles (leaves that are not models):".to_owned()];
    for (leaf, role) in leaves {
        lines.push(format!("  {}: {role}", node(file, leaf)));
    }
    lines
}

/// Every metadata entry a reader keeps: those about the whole file first,
/// then those about each node, in node order. Empty when there are none.
pub(crate) fn metadata(file: &BreadFile) -> Vec<String> {
    if file.metadata.is_empty() {
        return Vec::new();
    }
    let mut subjects: Vec<Option<usize>> = file.metadata.iter().map(|e| e.node).collect();
    subjects.sort_unstable();
    subjects.dedup();
    let mut lines = vec!["metadata:".to_owned()];
    for subject in subjects {
        lines.push(match subject {
            None => "  about the file:".to_owned(),
            Some(index) => format!("  about {}:", node(file, index)),
        });
        for entry in file.metadata.iter().filter(|e| e.node == subject) {
            lines.push(format!("    {}: {}", entry.key, value(entry)));
        }
    }
    lines
}

/// A node as `bread info` names it: its number, and its name when it has one.
fn node(file: &BreadFile, index: usize) -> String {
    match file
        .scene
        .nodes
        .get(index)
        .and_then(|n| n.meta.as_ref())
        .and_then(|m| m.name.as_deref())
    {
        Some(name) => format!("node {index} ({})", printable(name)),
        None => format!("node {index}"),
    }
}

/// A metadata value as text: a registered text key's text, a UUID in its
/// usual form, or a program's own bytes, counted and shown as hex.
fn value(entry: &MetadataEntry) -> String {
    if let Some(uuid) = entry.as_uuid() {
        return uuid_text(uuid);
    }
    if let Some(text) = entry.as_text() {
        return printable(text);
    }
    let bytes = &entry.value;
    let mut out = format!(
        "{} byte{}",
        bytes.len(),
        if bytes.len() == 1 { "" } else { "s" }
    );
    if !bytes.is_empty() {
        out.push(':');
        for byte in bytes.iter().take(SHOWN_BYTES) {
            let _ = write!(out, " {byte:02x}");
        }
        if bytes.len() > SHOWN_BYTES {
            out.push_str(" …");
        }
    }
    out
}

/// A UUID's 16 bytes in their usual text form: lowercase hex, 8-4-4-4-12.
fn uuid_text(uuid: [u8; 16]) -> String {
    let hex: String = uuid.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

/// Text from a file, safe to print on a terminal: control characters, which
/// could move the cursor or change colours, and the characters that turn text
/// around, which could make it read as something else, are written as
/// escapes, such as `\n` and `\u{202e}`.
pub(crate) fn printable(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() || turns_text(c) {
            out.extend(c.escape_default());
        } else {
            out.push(c);
        }
    }
    out
}

/// Whether `c` changes the direction of the text around it.
fn turns_text(c: char) -> bool {
    matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

#[cfg(test)]
mod tests {
    use bread_format::{
        BreadFile, Meta, MetadataEntry, Node, NodeKind, RoleEntry, Scene, SupportEntry,
    };

    use super::{credit, metadata, printable, roles};

    fn leaf(name: Option<&str>, mesh: usize) -> Node {
        Node {
            meta: name.map(|n| Meta {
                name: Some(n.into()),
                comment: None,
            }),
            transform: None,
            kind: NodeKind::Leaf { mesh },
        }
    }

    fn file() -> BreadFile {
        let scene = Scene {
            meta: None,
            nodes: vec![leaf(Some("base"), 0), leaf(None, 1), leaf(Some("plate"), 2)],
            roots: vec![0, 1, 2],
            meshes: vec![Default::default(); 3],
            mesh_resources: Vec::new(),
            subtrees: Vec::new(),
        };
        BreadFile {
            scene,
            ..BreadFile::default()
        }
    }

    #[test]
    fn the_credit_comes_in_its_order_and_only_about_the_file() {
        let mut f = file();
        f.metadata = vec![
            MetadataEntry::text(None, "source", "https://example.com/box"),
            MetadataEntry::text(Some(0), "designer", "Someone Else"),
            MetadataEntry::text(None, "license", "CC-BY-4.0"),
            MetadataEntry::text(None, "designer", "A. Designer"),
            MetadataEntry::text(None, "description", "Not part of the credit."),
        ];
        assert_eq!(
            credit(&f),
            [
                "designer: A. Designer",
                "license:  CC-BY-4.0",
                "source:   https://example.com/box",
            ]
        );
        assert!(credit(&file()).is_empty());
    }

    #[test]
    fn roles_name_each_leaf_and_count_supports_entries() {
        let mut f = file();
        f.roles = vec![
            RoleEntry { leaf: 2, role: 2 },
            RoleEntry { leaf: 0, role: 9 },
        ];
        f.supports = vec![SupportEntry {
            model: 0,
            support: 1,
            generator: "df".into(),
            version: 1,
            document: Vec::new(),
        }];
        assert_eq!(
            roles(&f),
            [
                "roles (leaves that are not models):",
                "  node 0 (base): a model (role 9 is reserved)",
                "  node 1: supports, for its supports entry",
                "  node 2 (plate): a raft",
            ]
        );
        assert!(roles(&file()).is_empty());
    }

    #[test]
    fn metadata_comes_by_subject_with_each_kind_of_value() {
        let mut f = file();
        f.metadata = vec![
            MetadataEntry::text(Some(2), "title", "Plate"),
            MetadataEntry::text(None, "title", "Box"),
            MetadataEntry::uuid(Some(0), [0x1b; 16]),
            MetadataEntry {
                node: Some(0),
                key: "example.note".into(),
                value: (0..20).collect(),
            },
        ];
        assert_eq!(
            metadata(&f),
            [
                "metadata:",
                "  about the file:",
                "    title: Box",
                "  about node 0 (base):",
                "    uuid: 1b1b1b1b-1b1b-1b1b-1b1b-1b1b1b1b1b1b",
                "    example.note: 20 bytes: 00 01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f …",
                "  about node 2 (plate):",
                "    title: Plate",
            ]
        );
        assert!(metadata(&file()).is_empty());
    }

    #[test]
    fn text_from_a_file_cannot_drive_the_terminal() {
        assert_eq!(printable("plain, ñandú"), "plain, ñandú");
        assert_eq!(printable("a\u{1b}[31mred\n"), "a\\u{1b}[31mred\\n");
        assert_eq!(printable("evil\u{202e}txt.exe"), "evil\\u{202e}txt.exe");
    }
}
