# Local compatibility patch

Upstream xattr 0.2.3, pulled in by GPUI's font dependency, references `libc::ENOATTR` on Linux. Recent libc releases removed this alias. On Linux and Android we use `libc::ENODATA`, the equivalent error constant; all other platforms keep the upstream constant. No other functional changes are intended. Original MIT and Apache license files are retained.
