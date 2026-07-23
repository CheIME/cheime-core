use super::*;

// ── backspace ──────────────────────────────────────────────────────────

#[test]
fn backspace_at_start_noop() {
    let mut doc = Document {
        text: "abc".into(),
        cursor: 0,
    };
    let removed = doc.backspace();
    assert!(!removed, "backspace at start must report no mutation");
    assert_eq!(doc.text(), "abc");
    assert_eq!(doc.cursor(), 0);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn backspace_ascii() {
    let mut doc = Document {
        text: "abc".into(),
        cursor: 3,
    };
    let removed = doc.backspace();
    assert!(removed, "backspace on ASCII must report mutation");
    assert_eq!(doc.text(), "ab");
    assert_eq!(doc.cursor(), 2);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn backspace_two_byte_scalar() {
    let mut doc = Document {
        text: "xé".into(),
        cursor: 3,
    };
    let removed = doc.backspace();
    assert!(removed, "backspace on 2-byte scalar must report mutation");
    assert_eq!(doc.text(), "x");
    assert_eq!(doc.cursor(), 1);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn backspace_three_byte_scalar() {
    let mut doc = Document {
        text: "x你".into(),
        cursor: 4,
    };
    let removed = doc.backspace();
    assert!(removed, "backspace on 3-byte scalar must report mutation");
    assert_eq!(doc.text(), "x");
    assert_eq!(doc.cursor(), 1);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn backspace_four_byte_scalar() {
    let mut doc = Document {
        text: "x😀".into(),
        cursor: 5,
    };
    let removed = doc.backspace();
    assert!(removed, "backspace on 4-byte scalar must report mutation");
    assert_eq!(doc.text(), "x");
    assert_eq!(doc.cursor(), 1);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn backspace_interior_mixed_unicode() {
    // "aé你😀": offsets 0,1,3,6,10
    let mut doc = Document {
        text: "aé你😀".into(),
        cursor: 6,
    };
    let removed1 = doc.backspace(); // deletes '你'(bytes 3..6), cursor→3
    assert!(removed1, "first backspace must report mutation");
    assert_eq!(doc.text(), "aé😀");
    assert_eq!(doc.cursor(), 3);
    assert!(doc.text().is_char_boundary(doc.cursor()));
    let removed2 = doc.backspace(); // deletes 'é'(bytes 1..3), cursor→1
    assert!(removed2, "second backspace must report mutation");
    assert_eq!(doc.text(), "a😀");
    assert_eq!(doc.cursor(), 1);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

// ── delete ─────────────────────────────────────────────────────────────

#[test]
fn delete_at_end_noop() {
    let mut doc = Document {
        text: "abc".into(),
        cursor: 3,
    };
    let removed = doc.delete();
    assert!(!removed, "delete at end must report no mutation");
    assert_eq!(doc.text(), "abc");
    assert_eq!(doc.cursor(), 3);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn delete_ascii() {
    let mut doc = Document {
        text: "abc".into(),
        cursor: 0,
    };
    let removed = doc.delete();
    assert!(removed, "delete on ASCII must report mutation");
    assert_eq!(doc.text(), "bc");
    assert_eq!(doc.cursor(), 0);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn delete_two_byte_scalar() {
    let mut doc = Document {
        text: "éx".into(),
        cursor: 0,
    };
    let removed = doc.delete();
    assert!(removed, "delete on 2-byte scalar must report mutation");
    assert_eq!(doc.text(), "x");
    assert_eq!(doc.cursor(), 0);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn delete_three_byte_scalar() {
    let mut doc = Document {
        text: "你x".into(),
        cursor: 0,
    };
    let removed = doc.delete();
    assert!(removed, "delete on 3-byte scalar must report mutation");
    assert_eq!(doc.text(), "x");
    assert_eq!(doc.cursor(), 0);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn delete_four_byte_scalar() {
    let mut doc = Document {
        text: "😀x".into(),
        cursor: 0,
    };
    let removed = doc.delete();
    assert!(removed, "delete on 4-byte scalar must report mutation");
    assert_eq!(doc.text(), "x");
    assert_eq!(doc.cursor(), 0);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn delete_interior_mixed_unicode() {
    // "aé你😀": offsets 0,1,3,6,10
    let mut doc = Document {
        text: "aé你😀".into(),
        cursor: 3,
    };
    let removed1 = doc.delete(); // deletes '你'(bytes 3..6), cursor stays 3
    assert!(removed1, "first delete must report mutation");
    assert_eq!(doc.text(), "aé😀");
    assert_eq!(doc.cursor(), 3);
    assert!(doc.text().is_char_boundary(doc.cursor()));
    let removed2 = doc.delete(); // deletes '😀'(bytes 3..7), cursor stays 3
    assert!(removed2, "second delete must report mutation");
    assert_eq!(doc.text(), "aé");
    assert_eq!(doc.cursor(), 3);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}
