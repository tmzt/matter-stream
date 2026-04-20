//! Convert JSON values to TKV entries.
//!
//! Maps `serde_json::Value` to a flat `Vec<TkvFixedEntry>` suitable for
//! arena storage and VM access via TKV ops.
//!
//! Requires the `json` feature.

use crate::tkv_key::{TkvKey, TkvType, TkvFixedEntry, StrRefDisc};

/// Convert a JSON value into a sorted vec of TKV entries.
///
/// String values are added to `string_table` and referenced by index.
/// Objects map each key to a segment (assigned incrementally 1..7).
/// Arrays use `TkvKey::array()` with element indices (max 63).
///
/// Returns entries sorted by `TkvKey::sort_key()` for binary search.
pub fn json_to_tkv_entries(
    value: &serde_json::Value,
    string_table: &mut Vec<String>,
) -> Vec<TkvFixedEntry> {
    let mut entries = Vec::new();
    let mut field_map = FieldMap::new();
    convert_value(value, &[], &mut entries, string_table, &mut field_map);
    entries.sort_by_key(|e| e.sort_key());
    entries
}

/// Track field name → segment assignment per depth level.
/// Each object at a given path gets its own namespace of segments 1..7.
struct FieldMap {
    /// (path_prefix, field_name) → segment_id
    assignments: Vec<(Vec<u8>, String, u8)>,
}

impl FieldMap {
    fn new() -> Self {
        Self { assignments: Vec::new() }
    }

    /// Get or assign a segment for a field name at the given path depth.
    fn segment_for(&mut self, path: &[u8], field_name: &str) -> Option<u8> {
        // Check existing assignment
        for (p, name, seg) in &self.assignments {
            if p == path && name == field_name {
                return Some(*seg);
            }
        }
        // Assign next available segment (1..7, 0 is reserved)
        let used: u8 = self.assignments.iter()
            .filter(|(p, _, _)| p == path)
            .count() as u8;
        let seg = used + 1;
        if seg > 7 { return None; } // max 7 fields per object level
        self.assignments.push((path.to_vec(), field_name.to_string(), seg));
        Some(seg)
    }
}

fn intern_string(string_table: &mut Vec<String>, s: &str) -> u32 {
    // Check for existing entry
    if let Some(idx) = string_table.iter().position(|existing| existing == s) {
        return idx as u32;
    }
    let idx = string_table.len() as u32;
    string_table.push(s.to_string());
    idx
}

fn convert_value(
    value: &serde_json::Value,
    path: &[u8],
    entries: &mut Vec<TkvFixedEntry>,
    string_table: &mut Vec<String>,
    field_map: &mut FieldMap,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                let seg = match field_map.segment_for(path, key) {
                    Some(s) => s,
                    None => continue, // skip if > 7 fields
                };
                let key_str_idx = intern_string(string_table, key);

                let mut child_path = path.to_vec();
                child_path.push(seg);

                match val {
                    serde_json::Value::String(s) => {
                        let str_idx = intern_string(string_table, s);
                        let tkv_key = TkvKey::new(&child_path, TkvType::String);
                        entries.push(TkvFixedEntry::string(
                            tkv_key, str_idx,
                            StrRefDisc::StringTable as u8, key_str_idx as u16,
                        ));
                    }
                    serde_json::Value::Number(n) => {
                        let int_val = n.as_u64().unwrap_or_else(||
                            n.as_i64().map(|i| i as u64).unwrap_or(0)
                        );
                        let tkv_key = TkvKey::new(&child_path, TkvType::Integer);
                        entries.push(TkvFixedEntry::integer(
                            tkv_key, int_val,
                            StrRefDisc::StringTable as u8, key_str_idx as u16,
                        ));
                    }
                    serde_json::Value::Bool(b) => {
                        let tkv_key = TkvKey::new(&child_path, TkvType::Boolean);
                        entries.push(TkvFixedEntry::boolean(
                            tkv_key, *b,
                            StrRefDisc::StringTable as u8, key_str_idx as u16,
                        ));
                    }
                    serde_json::Value::Null => {
                        let tkv_key = TkvKey::new(&child_path, TkvType::Null);
                        entries.push(TkvFixedEntry::null(
                            tkv_key,
                            StrRefDisc::StringTable as u8, key_str_idx as u16,
                        ));
                    }
                    serde_json::Value::Array(arr) => {
                        // Each array element gets an array key with index
                        for (i, elem) in arr.iter().enumerate() {
                            if i > 63 { break; } // TKV max 64 array elements
                            convert_array_element(
                                elem, &child_path, i as u8,
                                entries, string_table, field_map,
                                StrRefDisc::StringTable as u8, key_str_idx as u16,
                            );
                        }
                    }
                    serde_json::Value::Object(_) => {
                        // Recurse into nested object
                        convert_value(val, &child_path, entries, string_table, field_map);
                    }
                }
            }
        }
        serde_json::Value::Array(arr) => {
            // Top-level array: use segment 1 as the array path
            let array_path = if path.is_empty() { vec![1u8] } else { path.to_vec() };
            let key_str_idx = intern_string(string_table, "items");
            for (i, elem) in arr.iter().enumerate() {
                if i > 63 { break; }
                convert_array_element(
                    elem, &array_path, i as u8,
                    entries, string_table, field_map,
                    StrRefDisc::StringTable as u8, key_str_idx as u16,
                );
            }
        }
        // Scalar at top level — unusual but handle it
        serde_json::Value::String(s) => {
            let str_idx = intern_string(string_table, s);
            let tkv_key = TkvKey::new(&[1], TkvType::String);
            entries.push(TkvFixedEntry::string(tkv_key, str_idx, 0, 0));
        }
        serde_json::Value::Number(n) => {
            let int_val = n.as_u64().unwrap_or(0);
            let tkv_key = TkvKey::new(&[1], TkvType::Integer);
            entries.push(TkvFixedEntry::integer(tkv_key, int_val, 0, 0));
        }
        serde_json::Value::Bool(b) => {
            let tkv_key = TkvKey::new(&[1], TkvType::Boolean);
            entries.push(TkvFixedEntry::boolean(tkv_key, *b, 0, 0));
        }
        serde_json::Value::Null => {}
    }
}

fn convert_array_element(
    elem: &serde_json::Value,
    array_path: &[u8],
    index: u8,
    entries: &mut Vec<TkvFixedEntry>,
    string_table: &mut Vec<String>,
    field_map: &mut FieldMap,
    key_name_disc: u8,
    key_name_idx: u16,
) {
    if array_path.len() > 6 { return; } // TkvKey::array max 6 path segments

    match elem {
        serde_json::Value::String(s) => {
            let str_idx = intern_string(string_table, s);
            let tkv_key = TkvKey::array(array_path, index, TkvType::String);
            entries.push(TkvFixedEntry::string(
                tkv_key, str_idx, key_name_disc, key_name_idx,
            ));
        }
        serde_json::Value::Number(n) => {
            let int_val = n.as_u64().unwrap_or(0);
            let tkv_key = TkvKey::array(array_path, index, TkvType::Integer);
            entries.push(TkvFixedEntry::integer(
                tkv_key, int_val, key_name_disc, key_name_idx,
            ));
        }
        serde_json::Value::Bool(b) => {
            let tkv_key = TkvKey::array(array_path, index, TkvType::Boolean);
            entries.push(TkvFixedEntry::boolean(
                tkv_key, *b, key_name_disc, key_name_idx,
            ));
        }
        serde_json::Value::Null => {
            let tkv_key = TkvKey::array(array_path, index, TkvType::Null);
            entries.push(TkvFixedEntry::null(
                tkv_key, key_name_disc, key_name_idx,
            ));
        }
        serde_json::Value::Object(map) => {
            // Nested object in array — each field becomes a child of the array element.
            // We represent this as array entries where the key_str points to the field name.
            // For now, flatten: store each field as its own array entry with field name as key.
            for (field_name, field_val) in map {
                let field_str_idx = intern_string(string_table, field_name);
                match field_val {
                    serde_json::Value::String(s) => {
                        let str_idx = intern_string(string_table, s);
                        let tkv_key = TkvKey::array(array_path, index, TkvType::String);
                        entries.push(TkvFixedEntry::string(
                            tkv_key, str_idx,
                            StrRefDisc::StringTable as u8, field_str_idx as u16,
                        ));
                    }
                    serde_json::Value::Number(n) => {
                        let int_val = n.as_u64().unwrap_or(0);
                        let tkv_key = TkvKey::array(array_path, index, TkvType::Integer);
                        entries.push(TkvFixedEntry::integer(
                            tkv_key, int_val,
                            StrRefDisc::StringTable as u8, field_str_idx as u16,
                        ));
                    }
                    serde_json::Value::Bool(b) => {
                        let tkv_key = TkvKey::array(array_path, index, TkvType::Boolean);
                        entries.push(TkvFixedEntry::boolean(
                            tkv_key, *b,
                            StrRefDisc::StringTable as u8, field_str_idx as u16,
                        ));
                    }
                    _ => {} // skip nested arrays/objects within array objects for now
                }
            }
        }
        serde_json::Value::Array(_) => {
            // Nested arrays not supported in TKV array encoding
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_object() {
        let json = serde_json::json!({
            "name": "Alice",
            "age": 30,
            "active": true
        });
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        assert_eq!(entries.len(), 3);
        // Strings should be interned
        assert!(st.contains(&"name".to_string()));
        assert!(st.contains(&"Alice".to_string()));
        assert!(st.contains(&"age".to_string()));
        assert!(st.contains(&"active".to_string()));
    }

    #[test]
    fn string_array() {
        let json = serde_json::json!(["hello", "world", "foo"]);
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        assert_eq!(entries.len(), 3);
        assert!(st.contains(&"hello".to_string()));
        assert!(st.contains(&"world".to_string()));
        assert!(st.contains(&"foo".to_string()));
        // All should be array keys
        for e in &entries {
            assert!(TkvKey(e.key_path).is_array());
        }
    }

    #[test]
    fn object_with_array() {
        let json = serde_json::json!({
            "labels": ["inbox", "starred"],
            "subject": "Hello"
        });
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        // 2 array elements + 1 string field = 3 entries
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn nested_object() {
        let json = serde_json::json!({
            "user": {
                "name": "Bob",
                "email": "bob@example.com"
            }
        });
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        assert_eq!(entries.len(), 2); // name + email (user is structural, not an entry)
        assert!(st.contains(&"Bob".to_string()));
        assert!(st.contains(&"bob@example.com".to_string()));
    }

    #[test]
    fn array_of_objects() {
        let json = serde_json::json!([
            {"subject": "Hello", "from": "alice@test.com"},
            {"subject": "Meeting", "from": "bob@test.com"}
        ]);
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        // 2 objects × 2 fields = 4 entries
        assert_eq!(entries.len(), 4);
    }

    #[test]
    fn max_array_index() {
        let arr: Vec<serde_json::Value> = (0..70).map(|i| serde_json::json!(i)).collect();
        let json = serde_json::Value::Array(arr);
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        // Capped at 64 (indices 0-63)
        assert_eq!(entries.len(), 64);
    }

    #[test]
    fn entries_are_sorted() {
        let json = serde_json::json!({
            "z_field": "last",
            "a_field": "first",
            "m_field": "middle"
        });
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        for w in entries.windows(2) {
            assert!(w[0].sort_key() <= w[1].sort_key());
        }
    }

    #[test]
    fn empty_object() {
        let json = serde_json::json!({});
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);
        assert!(entries.is_empty());
    }

    #[test]
    fn null_value() {
        let json = serde_json::json!(null);
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);
        assert!(entries.is_empty());
    }

    // ── Gmail search result (realistic cog MCP output) ──────────────

    fn gmail_search_response() -> serde_json::Value {
        serde_json::json!([
            {
                "id": "18f1a2b3c4d5e6f7",
                "threadId": "18f1a2b3c4d5e6f7",
                "labelIds": ["INBOX", "UNREAD"],
                "snippet": "Hi, please find the invoice attached.",
                "internalDate": "1713456000000",
                "sizeEstimate": 15234,
                "payload": {
                    "mimeType": "multipart/mixed",
                    "headers": [
                        {"name": "From", "value": "alice@example.com"},
                        {"name": "Subject", "value": "Invoice #1234"},
                        {"name": "Date", "value": "Thu, 18 Apr 2024 10:00:00 -0400"}
                    ]
                }
            },
            {
                "id": "18f2b3c4d5e6f7a8",
                "threadId": "18f2b3c4d5e6f7a8",
                "labelIds": ["INBOX"],
                "snippet": "Meeting confirmed for tomorrow at 3pm.",
                "internalDate": "1713542400000",
                "sizeEstimate": 8192
            },
            {
                "id": "18f3c4d5e6f7a8b9",
                "threadId": "18f3c4d5e6f7a8b9",
                "labelIds": ["INBOX", "STARRED"],
                "snippet": "Q1 report is ready for review.",
                "internalDate": "1713628800000",
                "sizeEstimate": 42000
            }
        ])
    }

    #[test]
    fn gmail_search_converts() {
        let json = gmail_search_response();
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        // Should produce entries for all 3 messages
        assert!(!entries.is_empty());

        // All top-level entries should be array keys (array of messages)
        for e in &entries {
            let k = TkvKey(e.key_path);
            assert!(k.is_array(), "gmail entries should be array-keyed: {:?}", k);
        }

        // Check string interning captured key fields
        assert!(st.contains(&"id".to_string()));
        assert!(st.contains(&"snippet".to_string()));
        assert!(st.contains(&"18f1a2b3c4d5e6f7".to_string()));
        assert!(st.contains(&"Hi, please find the invoice attached.".to_string()));
        assert!(st.contains(&"Meeting confirmed for tomorrow at 3pm.".to_string()));
    }

    #[test]
    fn gmail_search_field_lookup() {
        let json = gmail_search_response();
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        // Find all entries for message index 0 by array_index
        let msg0: Vec<_> = entries.iter().filter(|e| {
            let k = TkvKey(e.key_path);
            k.is_array() && k.array_index() == 0
        }).collect();

        // Message 0 has: id, threadId, snippet, internalDate, sizeEstimate
        // (labelIds is an array of strings — nested arrays in array objects are skipped)
        // (payload is a nested object in array object — also skipped for now)
        assert!(msg0.len() >= 4, "msg0 should have at least 4 scalar fields, got {}", msg0.len());

        // Verify we can find the "id" field by key_str_idx
        let id_str_idx = st.iter().position(|s| s == "id").unwrap() as u16;
        let id_entry = msg0.iter().find(|e| e.key_str_idx == id_str_idx);
        assert!(id_entry.is_some(), "should find 'id' field in msg0");

        // Verify the id value is the right string
        let id_e = id_entry.unwrap();
        assert_eq!(id_e.value_type, TkvType::String as u8);
        // value[0] = StrRefDisc::StringTable, value[1..5] = string index
        let val_idx = u32::from_le_bytes([id_e.value[1], id_e.value[2], id_e.value[3], id_e.value[4]]);
        assert_eq!(&st[val_idx as usize], "18f1a2b3c4d5e6f7");
    }

    #[test]
    fn gmail_search_three_messages() {
        let json = gmail_search_response();
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        // Count distinct array indices
        let mut indices: Vec<u8> = entries.iter()
            .map(|e| TkvKey(e.key_path).array_index())
            .collect();
        indices.sort();
        indices.dedup();
        assert_eq!(indices, vec![0, 1, 2], "should have 3 message indices");
    }

    // ── Drive file list (realistic cog MCP output) ──────────────────

    fn drive_list_response() -> serde_json::Value {
        serde_json::json!([
            {
                "id": "1abc2def3ghi4jkl",
                "name": "Q1 Budget 2024.xlsx",
                "mimeType": "application/vnd.google-apps.spreadsheet",
                "starred": false,
                "trashed": false,
                "createdTime": "2024-01-15T09:30:00.000Z",
                "modifiedTime": "2024-04-10T14:22:00.000Z",
                "size": "245760",
                "webViewLink": "https://docs.google.com/spreadsheets/d/1abc2def3ghi4jkl/edit"
            },
            {
                "id": "2bcd3efg4hij5klm",
                "name": "Project Proposal.docx",
                "mimeType": "application/vnd.google-apps.document",
                "starred": true,
                "trashed": false,
                "createdTime": "2024-03-01T11:00:00.000Z",
                "modifiedTime": "2024-04-18T16:45:00.000Z",
                "webViewLink": "https://docs.google.com/document/d/2bcd3efg4hij5klm/edit"
            },
            {
                "id": "3cde4fgh5ijk6lmn",
                "name": "team-photo.jpg",
                "mimeType": "image/jpeg",
                "starred": false,
                "trashed": false,
                "createdTime": "2024-04-05T08:15:00.000Z",
                "modifiedTime": "2024-04-05T08:15:00.000Z",
                "size": "3145728"
            }
        ])
    }

    #[test]
    fn drive_list_converts() {
        let json = drive_list_response();
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        assert!(!entries.is_empty());

        // Check key fields were interned
        assert!(st.contains(&"name".to_string()));
        assert!(st.contains(&"mimeType".to_string()));
        assert!(st.contains(&"Q1 Budget 2024.xlsx".to_string()));
        assert!(st.contains(&"Project Proposal.docx".to_string()));
        assert!(st.contains(&"team-photo.jpg".to_string()));
    }

    #[test]
    fn drive_list_field_access() {
        let json = drive_list_response();
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        // Find entries for file index 1 (Project Proposal)
        let file1: Vec<_> = entries.iter().filter(|e| {
            let k = TkvKey(e.key_path);
            k.is_array() && k.array_index() == 1
        }).collect();

        // Should have name, mimeType, starred, trashed, createdTime, modifiedTime, webViewLink, id
        assert!(file1.len() >= 5, "file1 should have multiple fields, got {}", file1.len());

        // Find the "starred" field — should be boolean true for file 1
        let starred_idx = st.iter().position(|s| s == "starred").unwrap() as u16;
        let starred = file1.iter().find(|e| e.key_str_idx == starred_idx);
        assert!(starred.is_some());
        let s = starred.unwrap();
        assert_eq!(s.value_type, TkvType::Boolean as u8);
        assert_eq!(s.value[0], 1, "file1.starred should be true");
    }

    #[test]
    fn drive_list_boolean_values() {
        let json = drive_list_response();
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        let trashed_idx = st.iter().position(|s| s == "trashed").unwrap() as u16;

        // All files have trashed=false
        let trashed_entries: Vec<_> = entries.iter()
            .filter(|e| e.key_str_idx == trashed_idx && e.value_type == TkvType::Boolean as u8)
            .collect();

        assert_eq!(trashed_entries.len(), 3);
        for e in &trashed_entries {
            assert_eq!(e.value[0], 0, "all files should have trashed=false");
        }
    }

    #[test]
    fn drive_list_three_files() {
        let json = drive_list_response();
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        let mut indices: Vec<u8> = entries.iter()
            .map(|e| TkvKey(e.key_path).array_index())
            .collect();
        indices.sort();
        indices.dedup();
        assert_eq!(indices, vec![0, 1, 2]);
    }

    // ── Calendar events (realistic cog MCP output) ──────────────────

    fn calendar_events_response() -> serde_json::Value {
        serde_json::json!([
            {
                "id": "evt_abc123",
                "summary": "Team Standup",
                "status": "confirmed",
                "start": "2024-04-19T09:00:00-04:00",
                "end": "2024-04-19T09:15:00-04:00",
                "location": "Room 42",
                "organizer": "manager@example.com"
            },
            {
                "id": "evt_def456",
                "summary": "1:1 with Alice",
                "status": "confirmed",
                "start": "2024-04-19T14:00:00-04:00",
                "end": "2024-04-19T14:30:00-04:00"
            }
        ])
    }

    #[test]
    fn calendar_events_converts() {
        let json = calendar_events_response();
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        assert!(!entries.is_empty());
        assert!(st.contains(&"summary".to_string()));
        assert!(st.contains(&"Team Standup".to_string()));
        assert!(st.contains(&"1:1 with Alice".to_string()));
    }

    #[test]
    fn calendar_event_field_access() {
        let json = calendar_events_response();
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        // Event 0 should have a location field
        let evt0: Vec<_> = entries.iter().filter(|e| {
            let k = TkvKey(e.key_path);
            k.is_array() && k.array_index() == 0
        }).collect();

        let loc_idx = st.iter().position(|s| s == "location").unwrap() as u16;
        let loc_entry = evt0.iter().find(|e| e.key_str_idx == loc_idx);
        assert!(loc_entry.is_some());

        let loc = loc_entry.unwrap();
        let val_idx = u32::from_le_bytes([loc.value[1], loc.value[2], loc.value[3], loc.value[4]]);
        assert_eq!(&st[val_idx as usize], "Room 42");

        // Event 1 should NOT have a location field
        let evt1: Vec<_> = entries.iter().filter(|e| {
            let k = TkvKey(e.key_path);
            k.is_array() && k.array_index() == 1
        }).collect();
        let loc_in_evt1 = evt1.iter().find(|e| e.key_str_idx == loc_idx);
        assert!(loc_in_evt1.is_none(), "event 1 should not have location");
    }

    // ── Roundtrip: verify string values are recoverable ─────────────

    #[test]
    fn string_value_roundtrip() {
        let json = serde_json::json!({
            "from": "alice@example.com",
            "subject": "Invoice #1234"
        });
        let mut st = Vec::new();
        let entries = json_to_tkv_entries(&json, &mut st);

        for e in &entries {
            if e.value_type == TkvType::String as u8 {
                let disc = e.value[0];
                assert_eq!(disc, StrRefDisc::StringTable as u8);
                let idx = u32::from_le_bytes([e.value[1], e.value[2], e.value[3], e.value[4]]);
                let val = &st[idx as usize];
                // key_str_idx tells us the field name
                let key_name = &st[e.key_str_idx as usize];
                match key_name.as_str() {
                    "from" => assert_eq!(val, "alice@example.com"),
                    "subject" => assert_eq!(val, "Invoice #1234"),
                    _ => panic!("unexpected field: {}", key_name),
                }
            }
        }
    }
}
