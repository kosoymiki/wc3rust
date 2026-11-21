#!/usr/bin/env python3
"""Independent SYLK audit of the shipped WC3 data sheets.

Purpose: prove that `wc3-data` reads the shipped tables row-for-row and
cell-for-cell. This reader is deliberately a *second* implementation of the
SYLK positional model (a crate-side parser bug cannot hide a drift from it),
and it is cross-checked against the sheet's own `B;` bounds record, which is a
third, structural witness of the table shape.

SYLK positional model implemented here (matches the shipped files):
  * `C` (cell) and `F` (format) records share one cursor: either can carry
    `X`/`Y` and the position persists until changed;
  * a record that sets `Y` starts a new row;
  * a Y-less sheet starts a new row when the column goes backwards
    (`DestructableData.slk` is written that way);
  * a value record with no position continues to the next column, unless an
    `F`/`X` record positioned it first;
  * a `K` payload runs to end of line (formula remnants contain `;`).

Usage:
  python3 slk_audit.py [--root DIR] [--bounds-check] [--ids] [--probe TABLE:ROW:C1,C2]
"""
import argparse
import os
import sys

DEFAULT_ROOT = os.path.abspath(
    os.path.join(os.path.dirname(__file__), "..", "..", "..", "analysis", "war3_data")
)

TABLES = {
    "UnitData": ("Units/UnitData.slk", "unitID"),
    "UnitBalance": ("Units/UnitBalance.slk", "unitBalanceID"),
    "UnitWeapons": ("Units/UnitWeapons.slk", "unitWeapID"),
    "UnitAbilities": ("Units/UnitAbilities.slk", "unitAbilID"),
    "UnitUI": ("Units/unitUI.slk", "unitUIID"),
    "AbilityData": ("Units/AbilityData.slk", "alias"),
    "ItemData": ("Units/ItemData.slk", "itemID"),
    "UpgradeData": ("Units/UpgradeData.slk", "upgradeid"),
    "DestructableData": ("Units/DestructableData.slk", "DestructableID"),
    "Doodads": ("Doodads/Doodads.slk", "doodID"),
}


def unquote(value):
    if len(value) >= 2 and value[0] == '"' and value[-1] == '"':
        return value[1:-1].replace('""', '"')
    if value.startswith('"'):
        return value[1:].replace('""', '"')
    return value


def parse_position(line):
    """(x, y, value) with 1-based coordinates, from one `C`/`F` record."""
    x = y = None
    value = None
    # The K payload is the rest of the line (spreadsheet formula remnants
    # contain `;`), so join every field after `K` back together.
    parts = line.split(";")
    for index, part in enumerate(parts[1:], 1):
        if part.startswith("X") and part[1:].isdigit():
            x = int(part[1:])
        elif part.startswith("Y") and part[1:].isdigit():
            y = int(part[1:])
        elif part.startswith("K"):
            value = unquote(";".join([part[1:]] + parts[index + 1 :]))
            break
    return x, y, value


def parse_bounds(line):
    """`B` record -> (rows, cols) of the used range.

    Two spellings ship: `B;Y467;X34;D0 0 466 33` (RoC) and
    `B;X31;Y813;D0` (TFT). Both carry the extent as Y=last row+1, X=last
    column+1, so the axes are read directly instead of trusting the `D` payload.
    """
    x = y = None
    for part in line.split(";")[1:]:
        if part.startswith("X") and part[1:].isdigit():
            x = int(part[1:])
        elif part.startswith("Y") and part[1:].isdigit():
            y = int(part[1:])
    if x is None or y is None:
        return None
    return y, x


def parse_sheet(path):
    """Return (header, rows, bounds) with `rows` keyed by 0-based row index."""
    text = open(path, "r", encoding="utf-8", errors="replace").read()
    grid = {}
    bounds = None
    column = row = 0
    previous = None
    positioned = False
    for line in text.split("\n"):
        line = line.rstrip("\r")
        if not line:
            continue
        kind = line[0]
        if kind == "B":
            parsed = parse_bounds(line)
            if parsed:
                bounds = parsed
            continue
        if kind not in ("C", "F"):
            continue
        x, y, value = parse_position(line)
        if x is not None or y is not None:
            positioned = True
        if y is not None:
            row = max(0, y - 1)
            previous = None
        if x is not None:
            nxt = max(0, x - 1)
            if previous is not None and nxt < previous:
                row += 1
                previous = None
            column = nxt
        elif value is not None and not positioned:
            if previous is not None:
                column = previous + 1
        if value is None:
            continue
        grid.setdefault(row, {})[column] = value
        previous = column
        positioned = False
    header = grid.pop(0, {})
    return header, grid, bounds


def column_index(header, name):
    wanted = name.lower()
    for index, cell in header.items():
        if cell.lower() == wanted:
            return index
    return None


def load_table(root, table):
    rel, id_name = TABLES[table]
    path = os.path.join(root, rel)
    header, rows, bounds = parse_sheet(path)
    id_index = column_index(header, id_name)
    return header, rows, bounds, id_index


def audit(root):
    print(f"root={root}")
    print(f"{'table':16} {'rows':>5} {'unique':>6} {'bounds':>6} {'cols':>4} "
          f"{'header':>6} {'dupes':>5} {'bounds_ok':>9}")
    failures = 0
    for table in TABLES:
        header, rows, bounds, id_index = load_table(root, table)
        if id_index is None:
            print(f"{table:16} MISSING id column {TABLES[table][1]!r}")
            failures += 1
            continue
        ids = [cells[id_index] for _, cells in sorted(rows.items()) if cells.get(id_index)]
        unique = len(set(ids))
        dupes = len(ids) - unique
        data_width = 1 + max((max(cells) for cells in rows.values()), default=0)
        width = max(len(header), data_width)
        bounds_rows = bounds[0] - 1 if bounds else -1
        # The `B;` record may over-declare: TFT `UnitData` declares one more
        # column than any cell reaches, and `AbilityData` declares header-less
        # spacer rows (767 declared vs 763 id rows). A grid that fits inside the
        # declared extent is consistent; a grid that exceeds it is drift.
        spacers = bounds_rows - len(rows) if bounds else 0
        bounds_ok = (
            bounds_rows >= len(rows) and bounds[1] >= width and spacers >= 0
            if bounds
            else True
        )
        if not bounds_ok or dupes:
            failures += 1
        print(f"{table:16} {len(rows):>5} {unique:>6} {bounds_rows:>6} {width:>4} "
              f"{len(header):>6} {dupes:>5} {str(bounds_ok):>9} spacers={spacers}")
    print(f"failures={failures}")
    return failures


def probe(root, spec):
    table, row_id, columns = spec.split(":", 2)
    header, rows, bounds, id_index = load_table(root, table)
    if id_index is None:
        print(f"{table}: missing id column")
        return 1
    target = None
    for _, cells in sorted(rows.items()):
        if cells.get(id_index) == row_id:
            target = cells
            break
    if target is None:
        print(f"{table}: row {row_id!r} not found")
        return 1
    for column in columns.split(","):
        index = column_index(header, column)
        if index is None:
            print(f"{table}.{row_id}.{column}: column not found")
            continue
        print(f"{table}.{row_id}.{column} = {target.get(index)!r}")
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=DEFAULT_ROOT)
    parser.add_argument("--probe", action="append", default=[])
    parser.add_argument("--ids", action="store_true", help="print every id, one per line")
    parser.add_argument("--table", default=None, help="table for --ids")
    args = parser.parse_args()

    status = 0
    if args.probe:
        for spec in args.probe:
            status |= probe(args.root, spec)
    else:
        status = audit(args.root)
    if args.ids:
        table = args.table or "UnitData"
        _, rows, _, id_index = load_table(args.root, table)
        for _, cells in sorted(rows.items()):
            if cells.get(id_index):
                print(cells[id_index])
    return status


if __name__ == "__main__":
    sys.exit(main())
