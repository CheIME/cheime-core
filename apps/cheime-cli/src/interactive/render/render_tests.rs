use super::{display_width, truncate_columns};
use unicode_width::UnicodeWidthStr;

#[test]
fn display_width_counts_empty_ascii_and_spaces() {
    // Given
    let empty = "";
    let ascii = "hello";
    let spaces = "   ";

    // When / Then
    assert_eq!(display_width(empty), 0);
    assert_eq!(display_width(ascii), 5);
    assert_eq!(display_width(spaces), 3);
}

#[test]
fn display_width_counts_cjk_fullwidth_punctuation_and_emoji() {
    // Given
    let cjk = "你好";
    let punctuation = "，。";
    let emoji = "😀";

    // When / Then
    assert_eq!(display_width(cjk), 4);
    assert_eq!(display_width(punctuation), 4);
    assert_eq!(display_width(emoji), 2);
    assert_eq!(display_width("👩\u{200D}🔬"), 2);
}

#[test]
fn display_width_treats_combining_marks_and_zwj_as_zero_width() {
    // Given
    let combining = "e\u{301}";
    let zwj = "\u{200D}";

    // When / Then
    assert_eq!(display_width(combining), 1);
    assert_eq!(display_width(zwj), 0);
}

#[test]
fn truncate_columns_with_zero_maximum_keeps_leading_zero_width_scalars() {
    // Given
    let text = "\u{200D}\u{301}a";

    // When
    let truncated = truncate_columns(text, 0);

    // Then
    assert_eq!(truncated, "\u{200D}\u{301}");
}

#[test]
fn truncate_columns_refuses_half_of_a_width_two_scalar() {
    // Given
    let text = "你a";

    // When
    let truncated = truncate_columns(text, 1);

    // Then
    assert_eq!(truncated, "");
}

#[test]
fn truncate_columns_returns_exact_width_boundaries() {
    // Given
    let text = "a你b";

    // When / Then
    assert_eq!(truncate_columns(text, 1), "a");
    assert_eq!(truncate_columns(text, 3), "a你");
    assert_eq!(truncate_columns(text, 4), text);
}

#[test]
fn truncate_columns_preserves_combining_marks_before_an_excluded_scalar() {
    // Given
    let text = "a\u{301}你";

    // When
    let truncated = truncate_columns(text, 1);

    // Then
    assert_eq!(truncated, "a\u{301}");
}

#[test]
fn truncate_columns_handles_mixed_text() {
    // Given
    let text = "a你😀b";

    // When
    let truncated = truncate_columns(text, 3);

    // Then
    assert_eq!(truncated, "a你");
}

#[test]
fn truncate_columns_keeps_an_emoji_zwj_sequence_that_fits() {
    // Given
    let text = "👩\u{200D}🔬a";

    // When
    let truncated = truncate_columns(text, 2);

    // Then
    assert_eq!(truncated, "👩\u{200D}🔬");
}

#[test]
fn truncate_columns_excludes_width_increasing_vs16_when_limit_is_one() {
    // Given
    let text = "☀\u{FE0F}";

    // When
    let truncated = truncate_columns(text, 1);

    // Then
    assert_eq!(truncated, "☀");
    assert!(display_width(&truncated) <= 1);
}

#[test]
fn truncate_columns_handles_vs15_using_current_unicode_width_semantics() {
    // Given
    let text = "😀\u{FE0E}";

    // When
    let truncated = truncate_columns(text, 1);

    // Then
    let expected = match text.width() {
        1 => text,
        2 => "",
        width => panic!("unexpected unicode-width result: {width}"),
    };
    assert_eq!(truncated, expected);
    assert!(display_width(&truncated) <= 1);
}

#[test]
fn truncate_columns_does_not_preserve_a_newline_at_zero_columns() {
    // Given
    let text = "\na";

    // When
    let truncated = truncate_columns(text, 0);

    // Then
    assert_eq!(truncated, "");
    assert!(display_width(&truncated) == 0);
}

#[test]
fn truncate_columns_leaves_input_within_limit_unchanged() {
    // Given
    let text = "a你😀";

    // When
    let truncated = truncate_columns(text, 5);

    // Then
    assert_eq!(truncated, text);
}

#[test]
fn truncate_columns_returns_valid_utf8_at_a_multibyte_boundary() {
    // Given
    let text = "a你好";

    // When
    let truncated = truncate_columns(text, 3);

    // Then
    assert_eq!(truncated, "a你");
    assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
}
