//! Shared scanners for `$NAME`-style placeholder grammars used by the
//! regex convergence probe ([`crate::pattern`]) and the structural
//! mode's pattern preprocess + template parser ([`crate::structural`]).
//!
//! Keeping identifier and brace-find logic in one module stops the
//! three byte walkers from drifting: any future tweak to "what counts
//! as a metavar name" lands here once instead of in three places.
//!
//! All scanners take the byte index of the leading `$` and return the
//! inner name range plus the index of the first byte past the
//! placeholder, so callers can resume their walk.

/// Scan a `$NAME` placeholder. NAME is `[A-Za-z_][A-Za-z0-9_]*`.
/// `bytes[start]` must be `b'$'`. Returns `(name_start, name_end, after)`
/// or `None` if the position doesn't start a valid placeholder.
pub(crate) fn scan_meta_name(bytes: &[u8], start: usize) -> Option<(usize, usize, usize)> {
    debug_assert!(bytes.get(start) == Some(&b'$'));
    let name_start = start + 1;
    if name_start >= bytes.len() || !is_ident_start(bytes[name_start]) {
        return None;
    }
    let name_end = scan_ident_continue(bytes, name_start + 1);
    Some((name_start, name_end, name_end))
}

/// Scan a `$$$NAME` placeholder. NAME is `[A-Za-z_][A-Za-z0-9_]*`.
/// `bytes[start]` must be `b'$'`. Returns `(name_start, name_end, after)`
/// or `None` if the three leading dollars or the identifier aren't there.
pub(crate) fn scan_ellipsis_name(bytes: &[u8], start: usize) -> Option<(usize, usize, usize)> {
    debug_assert!(bytes.get(start) == Some(&b'$'));
    if start + 3 >= bytes.len() || bytes[start + 1] != b'$' || bytes[start + 2] != b'$' {
        return None;
    }
    let name_start = start + 3;
    if !is_ident_start(bytes[name_start]) {
        return None;
    }
    let name_end = scan_ident_continue(bytes, name_start + 1);
    Some((name_start, name_end, name_end))
}

/// Scan a `${NAME}` placeholder. The input slice `input.as_bytes()[start..start+2]`
/// must be `b"${"`. Returns `(name_start, name_end, after)` (the inner
/// name range and the byte after the closing `}`). `None` if the `}` is
/// missing.
pub(crate) fn scan_braced_name(input: &str, start: usize) -> Option<(usize, usize, usize)> {
    debug_assert!(input.as_bytes().get(start) == Some(&b'$'));
    debug_assert!(input.as_bytes().get(start + 1) == Some(&b'{'));
    let name_start = start + 2;
    let close_offset = input.get(name_start..)?.find('}')?;
    let name_end = name_start + close_offset;
    Some((name_start, name_end, name_end + 1))
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn scan_ident_continue(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    i
}
