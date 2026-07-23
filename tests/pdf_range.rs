use paper_codex::pdf_range::{parse_single_range, ByteRange};

#[test]
fn parses_closed_open_and_suffix_ranges() {
    assert_eq!(
        parse_single_range("bytes=0-99", 1000).unwrap(),
        Some(ByteRange { start: 0, end: 99 })
    );
    assert_eq!(
        parse_single_range("bytes=900-", 1000).unwrap(),
        Some(ByteRange {
            start: 900,
            end: 999
        })
    );
    assert_eq!(
        parse_single_range("bytes=-100", 1000).unwrap(),
        Some(ByteRange {
            start: 900,
            end: 999
        })
    );
}

#[test]
fn rejects_unsatisfiable_or_multiple_ranges() {
    assert!(parse_single_range("bytes=1000-1200", 1000).is_err());
    assert!(parse_single_range("bytes=0-2,5-7", 1000).is_err());
    assert!(parse_single_range("items=0-2", 1000).is_err());
    assert!(parse_single_range("bytes=-0", 1000).is_err());
}
