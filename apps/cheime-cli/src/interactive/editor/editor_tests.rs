use super::*;

#[test]
fn empty_new() {
    let doc = Document::new();
    assert_eq!(doc.text(), "");
    assert_eq!(doc.cursor(), 0);
}

#[test]
fn insert_ascii() {
    let mut doc = Document::new();
    doc.insert("hello");
    assert_eq!(doc.text(), "hello");
    assert_eq!(doc.cursor(), 5);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn insert_unicode() {
    let mut doc = Document::new();
    doc.insert("你好");
    assert_eq!(doc.text(), "你好");
    assert_eq!(doc.cursor(), 6); // 2 × 3 UTF-8 bytes
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn insert_mixed_unicode() {
    let mut doc = Document::new();
    doc.insert("a你b好");
    assert_eq!(doc.text(), "a你b好");
    // 1 + 3 + 1 + 3 = 8
    assert_eq!(doc.cursor(), 8);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn insert_middle() {
    let mut doc = Document {
        text: "你a好".to_string(),
        cursor: 4, // after "你a"
    };
    doc.insert("bc");
    assert_eq!(doc.text(), "你abc好");
    // 4 + 2 = 6
    assert_eq!(doc.cursor(), 6);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn insert_before_chinese() {
    let mut doc = Document {
        text: "你好".to_string(),
        cursor: 0,
    };
    doc.insert("前");
    assert_eq!(doc.text(), "前你好");
    assert_eq!(doc.cursor(), 3); // 1 × 3 UTF-8 bytes
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn insert_after_chinese() {
    let mut doc = Document {
        text: "你好".to_string(),
        cursor: 6, // end of "你好"
    };
    doc.insert("后");
    assert_eq!(doc.text(), "你好后");
    assert_eq!(doc.cursor(), 9); // 6 + 3
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn insert_empty_string_is_noop() {
    let mut doc = Document::new();
    doc.insert("");
    assert_eq!(doc.text(), "");
    assert_eq!(doc.cursor(), 0);
}

#[test]
fn insert_empty_into_middle_does_not_move_cursor() {
    let mut doc = Document {
        text: "abc".to_string(),
        cursor: 1,
    };
    doc.insert("");
    assert_eq!(doc.text(), "abc");
    assert_eq!(doc.cursor(), 1);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn insert_cursor_always_char_boundary() {
    let mut doc = Document::new();
    doc.insert("a你b");
    assert!(doc.text().is_char_boundary(doc.cursor()));
    doc.insert("好c");
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn move_left_through_mixed_unicode() {
    let mut doc = Document {
        text: "a你b好".to_string(),
        cursor: 8,
    };
    doc.move_left();
    assert_eq!(doc.cursor(), 5);
    assert!(doc.text().is_char_boundary(doc.cursor()));
    doc.move_left();
    assert_eq!(doc.cursor(), 4);
    assert!(doc.text().is_char_boundary(doc.cursor()));
    doc.move_left();
    assert_eq!(doc.cursor(), 1);
    assert!(doc.text().is_char_boundary(doc.cursor()));
    doc.move_left();
    assert_eq!(doc.cursor(), 0);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn move_left_at_start_is_noop() {
    let mut doc = Document {
        text: "a你b好".to_string(),
        cursor: 0,
    };
    doc.move_left();
    assert_eq!(doc.cursor(), 0);
}

#[test]
fn move_right_through_mixed_unicode() {
    let mut doc = Document {
        text: "a你b好".to_string(),
        cursor: 0,
    };
    doc.move_right();
    assert_eq!(doc.cursor(), 1);
    assert!(doc.text().is_char_boundary(doc.cursor()));
    doc.move_right();
    assert_eq!(doc.cursor(), 4);
    assert!(doc.text().is_char_boundary(doc.cursor()));
    doc.move_right();
    assert_eq!(doc.cursor(), 5);
    assert!(doc.text().is_char_boundary(doc.cursor()));
    doc.move_right();
    assert_eq!(doc.cursor(), 8);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn move_right_at_end_is_noop() {
    let mut doc = Document {
        text: "a你b好".to_string(),
        cursor: 8,
    };
    doc.move_right();
    assert_eq!(doc.cursor(), 8);
}

#[test]
fn move_home_from_interior() {
    let mut doc = Document {
        text: "a你b好".to_string(),
        cursor: 5,
    };
    doc.move_home();
    assert_eq!(doc.cursor(), 0);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn move_end_from_interior() {
    let mut doc = Document {
        text: "a你b好".to_string(),
        cursor: 1,
    };
    doc.move_end();
    assert_eq!(doc.cursor(), 8);
    assert!(doc.text().is_char_boundary(doc.cursor()));
}

#[test]
fn move_home_from_home_is_noop() {
    let mut doc = Document {
        text: "a你b好".to_string(),
        cursor: 0,
    };
    doc.move_home();
    assert_eq!(doc.cursor(), 0);
}

#[test]
fn move_end_from_end_is_noop() {
    let mut doc = Document {
        text: "a你b好".to_string(),
        cursor: 8,
    };
    doc.move_end();
    assert_eq!(doc.cursor(), 8);
}

#[test]
fn move_left_right_through_four_byte_widths() {
    // String "aé你😀" contains one scalar at each UTF-8 byte width:
    //   'a'  = 1 byte,  'é'  = 2 bytes,  '你' = 3 bytes,  '😀' = 4 bytes
    // Correct byte offsets: 0 (start), 1 (after 'a'), 3 (after 'é'),
    // 6 (after '你'), 10 (after '😀').
    let mut doc = Document {
        text: "aé你😀".to_string(),
        cursor: 0,
    };
    assert!(doc.text().is_char_boundary(doc.cursor()));

    // ---- rightward ----
    doc.move_right();
    assert_eq!(doc.cursor(), 1);
    assert!(doc.text().is_char_boundary(doc.cursor()));
    doc.move_right();
    assert_eq!(doc.cursor(), 3);
    assert!(doc.text().is_char_boundary(doc.cursor()));
    doc.move_right();
    assert_eq!(doc.cursor(), 6);
    assert!(doc.text().is_char_boundary(doc.cursor()));
    doc.move_right();
    assert_eq!(doc.cursor(), 10);
    assert!(doc.text().is_char_boundary(doc.cursor()));

    // noop at end
    doc.move_right();
    assert_eq!(doc.cursor(), 10);

    // ---- leftward ----
    doc.move_left();
    assert_eq!(doc.cursor(), 6);
    assert!(doc.text().is_char_boundary(doc.cursor()));
    doc.move_left();
    assert_eq!(doc.cursor(), 3);
    assert!(doc.text().is_char_boundary(doc.cursor()));
    doc.move_left();
    assert_eq!(doc.cursor(), 1);
    assert!(doc.text().is_char_boundary(doc.cursor()));
    doc.move_left();
    assert_eq!(doc.cursor(), 0);
    assert!(doc.text().is_char_boundary(doc.cursor()));

    // noop at start
    doc.move_left();
    assert_eq!(doc.cursor(), 0);
}
