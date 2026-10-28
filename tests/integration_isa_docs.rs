// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Guards the documented ISA table against drift from [`Opcode`].
//!
//! The README table and the decoder must describe the same instruction set: same number of
//! opcodes, same 5-trit encodings, and no stale counts left behind by an older revision.

use setun_doom::cpu::Opcode;
use setun_doom::trit::Trit;
use setun_doom::word::i64_to_trits;

/// Renders the 5-trit field of `value` in the README order (most significant trit first).
fn balanced_field(value: i64) -> String {
    i64_to_trits::<5>(value)
        .iter()
        .rev()
        .map(|trit| match trit {
            Trit::Positive => '+',
            Trit::Zero => '0',
            Trit::Negative => '-',
        })
        .collect()
}

/// Parses `(opcode, trit field)` pairs out of the README ISA table.
fn documented_opcodes(readme: &str) -> Vec<(i64, String)> {
    let mut rows = Vec::new();
    let mut in_table = false;
    for line in readme.lines() {
        if line.starts_with("## 45-Opcode Ternary ISA Table") {
            in_table = true;
            continue;
        }
        if in_table && line.starts_with("---") {
            break;
        }
        if !in_table || !line.starts_with("| `") {
            continue;
        }
        let cells: Vec<&str> = line.split('|').collect();
        assert!(cells.len() >= 4, "malformed ISA table row: {line}");
        let opcode = cells[1].trim().trim_matches('`').trim_start_matches('+');
        let value: i64 = opcode
            .parse()
            .unwrap_or_else(|_| panic!("unparsable opcode {opcode:?} in row: {line}"));
        let field = cells[3].trim().trim_matches('`').to_owned();
        rows.push((value, field));
    }
    rows
}

#[test]
fn readme_isa_table_matches_the_decoder() {
    let readme = include_str!("../README.md");
    let rows = documented_opcodes(readme);
    assert!(!rows.is_empty(), "ISA table section not found in README");

    let decodable: Vec<i64> = (-121..=121)
        .filter(|value| Opcode::from_trit_field(*value).is_some())
        .collect();

    assert_eq!(
        rows.len(),
        decodable.len(),
        "README documents {} opcodes, the decoder implements {}",
        rows.len(),
        decodable.len()
    );

    for (value, field) in &rows {
        let opcode = Opcode::from_trit_field(*value)
            .unwrap_or_else(|| panic!("README lists opcode {value} which the decoder rejects"));
        assert_eq!(
            *field,
            balanced_field(*value),
            "trit field for {opcode:?} ({value}) is {field}, expected {}",
            balanced_field(*value)
        );
    }

    for value in decodable {
        assert!(
            rows.iter().any(|(documented, _)| *documented == value),
            "opcode {value} ({:?}) is implemented but missing from the README table",
            Opcode::from_trit_field(value)
        );
    }
}

#[test]
fn documentation_has_no_stale_opcode_counts() {
    let readme = include_str!("../README.md");
    let agents = include_str!("../AGENTS.md");

    assert!(readme.contains("implements 45 authentic opcodes"));
    assert!(readme.contains("## 45-Opcode Ternary ISA Table"));
    assert!(agents.contains("45-opcode Setun/REBEL-6 CPU pipeline"));

    for stale in ["36-Opcode", "36 authentic opcodes", "35-opcode"] {
        assert!(
            !readme.contains(stale),
            "README still carries the stale phrase {stale:?}"
        );
        assert!(
            !agents.contains(stale),
            "AGENTS.md still carries the stale phrase {stale:?}"
        );
    }
}
