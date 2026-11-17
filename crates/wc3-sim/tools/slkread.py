"""Minimal SYLK (.slk) reader for the War3 extraction.

War3 ships its unit tables as SYLK: a header row (``Y1``) of column names, then
one row per unit.  Each line is ``C;<Y row>;<X col>;K<value>`` where ``Y`` may
be omitted on continuation lines (the row state carries over), the value may be
quoted (``K"text"``) or numeric (``K160``), and a trailing ``;<FORMULA>`` may
follow.  Rows are emitted in file order with their first-column key under
``"_id"``.
"""

from __future__ import annotations


def _parse_cell(line):
    """Return ``(row, col, value)`` for one SYLK cell line, or ``None``."""
    if not line.startswith("C;"):
        return None
    row = None
    col = None
    value = None
    for token in line[2:].split(";"):
        if token.startswith("Y") and token[1:].isdigit():
            row = int(token[1:])
        elif token.startswith("X") and token[1:].isdigit():
            col = int(token[1:])
        elif token.startswith("K"):
            raw = token[1:]
            if raw.startswith('"'):
                end = raw.rfind('"')
                value = raw[1:end] if end > 0 else raw[1:]
            else:
                value = raw
        elif token[:1] in {"E", "F", "P"}:
            break  # formula/formatting tail: nothing more to read
    return row, col, value


def read_slk_rows(path):
    """Return ``(header, rows)`` for a SYLK file.

    ``header`` maps a 1-based column index to its name; ``rows`` is a list of
    ``{column_name: raw_value}`` dicts in file order, each carrying the
    first-column key under ``"_id"``.
    """
    header: dict[int, str] = {}
    rows: dict[int, dict[int, str]] = {}
    current_row = 0
    current_col = 0
    with open(path, encoding="latin-1") as handle:
        for line in handle:
            parsed = _parse_cell(line.rstrip("\r\n"))
            if parsed is None:
                continue
            row, col, value = parsed
            if row is not None:
                current_row = row
            if col is not None:
                current_col = col
            if value is None:
                continue
            if current_row == 1:
                header[current_col] = value
            else:
                rows.setdefault(current_row, {})[current_col] = value
    out = []
    for row in sorted(rows):
        body = rows[row]
        unit_id = body.get(1)
        if unit_id is None:
            continue
        record = {"_id": unit_id}
        for col, value in body.items():
            name = header.get(col)
            if name is None or col == 1:
                continue
            record[name] = value
        out.append(record)
    return header, out


def index_by_id(records):
    """Index ``read_slk_rows`` output by its ``_id`` column."""
    return {rec["_id"]: rec for rec in records}


_PLACEHOLDERS = {"", "-", "_", " - ", "?", "N/A"}


def as_int(value, default=0):
    """Best-effort integer parse of a raw SYLK cell."""
    text = "" if value is None else str(value).strip()
    if text in _PLACEHOLDERS:
        return default
    try:
        return int(float(text))
    except ValueError:
        return default


def as_float(value, default=0.0):
    """Best-effort float parse of a raw SYLK cell."""
    text = "" if value is None else str(value).strip()
    if text in _PLACEHOLDERS:
        return default
    try:
        return float(text)
    except ValueError:
        return default


def as_list(value):
    """Split a comma-separated SYLK cell (``targs1``/``splashTargs1``)."""
    if value is None:
        return []
    return [item.strip() for item in str(value).split(",") if item.strip() not in {"", "-", "_"}]
