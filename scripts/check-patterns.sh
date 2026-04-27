#!/bin/sh
set -e

failures=0

check() {
    name="$1"
    shift
    if "$@"; then
        echo "  PASS  $name"
    else
        echo "  FAIL  $name"
        failures=$((failures + 1))
    fi
}

echo "=== Pattern Checks ==="
echo ""

# 1. Clippy
echo "--- clippy ---"
check "cargo clippy -D warnings -D clippy::unwrap_used -D clippy::expect_used" \
    cargo clippy -- -D warnings -D clippy::unwrap_used -D clippy::expect_used

echo ""

# 2. No bare unwrap() in non-test source
echo "--- unwrap check ---"
unwrap_count=$(find src/ -name '*.rs' -not -path 'src/mpd/mock.rs' -exec grep -Hn '\.unwrap()' {} + | \
    grep -v '#\[cfg\(test\)\]' | \
    grep -v '^\s*//' | \
    grep -c '\.unwrap()' || true)
if [ "$unwrap_count" -eq 0 ]; then
    echo "  PASS  No bare unwrap() in non-test source"
else
    echo "  FAIL  Found $unwrap_count bare unwrap() calls in non-test source:"
    find src/ -name '*.rs' -not -path 'src/mpd/mock.rs' -exec grep -Hn '\.unwrap()' {} + | \
        grep -v '#\[cfg\(test\)\]' | \
        grep -v '^\s*//'
    failures=$((failures + 1))
fi

echo ""

# 3. No GTK imports in presenters
echo "--- GTK import in presenters ---"
gtk_in_presenters=$(find src/presenters/ -name '*.rs' -exec grep -HnE 'use (gtk4|gdk4|gdk_pixbuf)' {} + || true)
if [ -z "$gtk_in_presenters" ]; then
    echo "  PASS  No GTK imports in src/presenters/"
else
    echo "  FAIL  GTK imports found in src/presenters/:"
    echo "$gtk_in_presenters"
    failures=$((failures + 1))
fi

echo ""

# 4. Module import rules
echo "--- module import rules ---"

mpd_imports_ui=$(find src/mpd/ -name '*.rs' -exec grep -Hn 'use crate::ui' {} + || true)
if [ -z "$mpd_imports_ui" ]; then
    echo "  PASS  mpd/ does not import ui/"
else
    echo "  FAIL  mpd/ imports ui/:"
    echo "$mpd_imports_ui"
    failures=$((failures + 1))
fi

presenters_imports_mpd=$(find src/presenters/ -name '*.rs' -exec grep -Hn 'use crate::mpd' {} + || true)
if [ -z "$presenters_imports_mpd" ]; then
    echo "  PASS  presenters/ does not import mpd/ directly"
else
    echo "  FAIL  presenters/ imports mpd/ directly:"
    echo "$presenters_imports_mpd"
    failures=$((failures + 1))
fi

echo ""

# 5. Tests
echo "--- tests ---"
check "cargo test" cargo test

echo ""

# Summary
if [ "$failures" -eq 0 ]; then
    echo "=== All checks passed ==="
    exit 0
else
    echo "=== $failures check(s) failed ==="
    exit 1
fi
