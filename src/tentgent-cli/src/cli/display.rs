pub(crate) fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    const TIB: f64 = GIB * 1024.0;

    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let bytes = bytes as f64;
    if bytes >= TIB {
        format!("{:.1} TiB", bytes / TIB)
    } else if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else {
        format!("{:.1} KiB", bytes / KIB)
    }
}

pub(crate) fn format_optional_bytes(bytes: Option<u64>) -> String {
    bytes.map(format_bytes).unwrap_or_else(|| "-".to_string())
}

pub(crate) fn format_size_transition(left: Option<u64>, right: Option<u64>) -> String {
    match (left, right) {
        (Some(left), Some(right)) => format!("{} -> {}", format_bytes(left), format_bytes(right)),
        (Some(left), None) => format!("{} -> -", format_bytes(left)),
        (None, Some(right)) => format!("- -> {}", format_bytes(right)),
        (None, None) => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bytes_with_binary_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(1024_u64 * 1024), "1.0 MiB");
        assert_eq!(format_bytes(2_469_606_195), "2.3 GiB");
    }

    #[test]
    fn formats_optional_bytes() {
        assert_eq!(format_optional_bytes(Some(2048)), "2.0 KiB");
        assert_eq!(format_optional_bytes(None), "-");
    }

    #[test]
    fn formats_size_transitions() {
        assert_eq!(
            format_size_transition(Some(1024), Some(2048)),
            "1.0 KiB -> 2.0 KiB"
        );
        assert_eq!(format_size_transition(None, Some(4096)), "- -> 4.0 KiB");
        assert_eq!(format_size_transition(Some(4096), None), "4.0 KiB -> -");
        assert_eq!(format_size_transition(None, None), "-");
    }
}
