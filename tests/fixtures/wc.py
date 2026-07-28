"""Reference wc implementation for golden task 1.

Deterministic library: count words, lines, characters, and bytes in a string.
This is the baseline that the Rust port must match byte-for-byte.
"""


def wc(text: str) -> dict[str, int]:
    """Count words, lines, characters, and bytes in a string.

    Args:
        text: Input string (any encoding).

    Returns:
        Dict with keys: words, lines, chars, bytes.
    """
    if not text:
        return {"words": 0, "lines": 0, "chars": 0, "bytes": 0}

    lines = text.count("\n")
    # Count trailing content after last newline as a line
    if text and not text.endswith("\n"):
        lines += 1
    elif not text.strip():
        # Edge case: only whitespace/newlines, treat as counted lines
        lines = text.count("\n")

    words = len(text.split())
    chars = len(text)
    b = text.encode("utf-8")

    return {
        "words": words,
        "lines": lines,
        "chars": chars,
        "bytes": len(b),
    }
