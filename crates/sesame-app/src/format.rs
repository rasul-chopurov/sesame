pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} B") } else { format!("{v:.1} {}", UNITS[i]) }
}

pub fn duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86400, secs % 86400 / 3600, secs % 3600 / 60);
    match (d, h) {
        (0, 0) => format!("{m}m"),
        (0, _) => format!("{h}h {m}m"),
        _ => format!("{d}d {h}h"),
    }
}

pub fn percent(part: u64, whole: u64) -> u64 {
    if whole == 0 { 0 } else { part * 100 / whole }
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats() {
        assert_eq!(super::bytes(3_880_000 * 1024), "3.7 GB");
        assert_eq!(super::bytes(512), "512 B");
        assert_eq!(super::duration(183245), "2d 2h");
        assert_eq!(super::duration(3700), "1h 1m");
        assert_eq!(super::percent(13, 38), 34);
    }
}
