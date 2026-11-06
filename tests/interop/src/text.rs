//! Byte searches shared by the sink and the queue readers.

/// The position of the first `needle` in `haystack`.
pub(crate) fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    let last_start = haystack.len() - needle.len();
    let mut start = 0;
    while start <= last_start {
        if &haystack[start..start + needle.len()] == needle {
            return Some(start);
        }
        start += 1;
    }
    None
}

/// `needle` appears in `haystack`.
pub(crate) fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    find(haystack, needle).is_some()
}

/// The position of the first `byte` in `haystack`.
pub(crate) fn find_byte(haystack: &[u8], byte: u8) -> Option<usize> {
    let mut index = 0;
    while index < haystack.len() {
        if haystack[index] == byte {
            return Some(index);
        }
        index += 1;
    }
    None
}

/// The lines of `bytes`, split at each LF, without the LF. A CR before the
/// LF stays in the line.
pub(crate) fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut rest = bytes;
    loop {
        match find_byte(rest, b'\n') {
            Some(end) => {
                lines.push(&rest[..end]);
                rest = &rest[end + 1..];
            }
            None => {
                lines.push(rest);
                return lines;
            }
        }
    }
}

/// The lines of `bytes`, each with its LF.
pub(crate) fn lines_with_ends(bytes: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        match find_byte(rest, b'\n') {
            Some(end) => {
                lines.push(&rest[..end + 1]);
                rest = &rest[end + 1..];
            }
            None => {
                lines.push(rest);
                rest = &[];
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_returns_the_first_match() {
        assert_eq!(find(b"abcabc", b"bc"), Some(1));
        assert_eq!(find(b"abc", b"abc"), Some(0));
        assert_eq!(find(b"abc", b"x"), None);
        assert_eq!(find(b"ab", b"abc"), None);
        assert_eq!(find(b"abc", b""), None);
    }

    #[test]
    fn find_byte_returns_the_first_match() {
        assert_eq!(find_byte(b"a:b:c", b':'), Some(1));
        assert_eq!(find_byte(b"abc", b':'), None);
        assert_eq!(find_byte(b"", b':'), None);
    }

    #[test]
    fn lines_split_at_each_lf() {
        let expected: Vec<&[u8]> = vec![b"one\r", b"two", b""];
        assert_eq!(lines(b"one\r\ntwo\n"), expected);
        let single: Vec<&[u8]> = vec![b"one"];
        assert_eq!(lines(b"one"), single);
    }

    #[test]
    fn lines_with_ends_keep_each_lf() {
        let expected: Vec<&[u8]> = vec![b"one\r\n", b"two\n", b"three"];
        assert_eq!(lines_with_ends(b"one\r\ntwo\nthree"), expected);
        let empty: Vec<&[u8]> = Vec::new();
        assert_eq!(lines_with_ends(b""), empty);
    }
}
