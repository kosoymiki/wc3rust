#!/usr/bin/env python3
"""Full-table differential audit: typed database (SQLite dump) vs shipped SLKs.

`dump_gamedata sqlite <db>` materialises what `wc3-data` typed from the sheets.
This script reads the same sheets with its own SYLK parser (`slk_audit.py`) and
compares every mapped field of every row — not just spot values — so a mapping
swap (e.g. `HP` vs `realHP`) cannot hide behind matching row counts.

Usage:
  python3 slk_diff.py --db /path/to/gamedata.db --root analysis/war3_data
"""
import argparse
import os
import sqlite3
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import slk_audit as audit

ABSENT = {"", "-", "_"}


def num(cell):
    """atof-style leading number, matching the crate's lossy reads."""
    if cell is None:
        return None
    text = cell.strip()
    if text in ABSENT:
        return None
    start = 0
    if text[0] in "+-":
        start = 1
    end = start
    seen_digit = False
    while end < len(text):
        ch = text[end]
        if ch.isdigit():
            seen_digit = True
            end += 1
        elif ch == "." and end + 1 < len(text) and text[end + 1].isdigit():
            end += 1
        else:
            break
    if not seen_digit:
        return None
    value = float(text[:end])
    return int(value) if value.is_integer() else value


def flag(cell):
    cell = num(cell)
    return None if cell is None else int(cell != 0)


def table_rows(root, table):
    header, rows, _, id_index = audit.load_table(root, table)
    out = {}
    for _, cells in sorted(rows.items()):
        row_id = cells.get(id_index)
        if row_id:
            out[row_id] = cells
    return header, out


def column(header, name):
    for index, value in header.items():
        if value.lower() == name.lower():
            return index
    return None


def compare(db, root, table, mapped, normalize):
    """Compare `mapped`: {sql_column: (sheet, sheet_column, kind)}.

    Returns (rows, mismatches, samples).
    """
    connection = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    sql_rows = {row[0]: row for row in connection.execute(f"SELECT * FROM {table}")}
    sql_columns = [row[1] for row in connection.execute(f"PRAGMA table_info({table})")]
    sheets = {}
    rows_by_id = {}
    for sheet_name in {spec[0] for spec in mapped.values()}:
        header, rows = table_rows(root, sheet_name)
        sheets[sheet_name] = header
        rows_by_id[sheet_name] = rows

    mismatches = 0
    samples = []
    checked = 0
    for row_id in sorted(set(rows_by_id[next(iter(rows_by_id))]) & set(sql_rows)):
        checked += 1
        sql_row = sql_rows[row_id]
        for sql_column, (sheet_name, sheet_column, kind) in mapped.items():
            index = column(sheets[sheet_name], sheet_column)
            cells = rows_by_id[sheet_name].get(row_id, {})
            expected = cells.get(index) if index is not None else None
            if kind == "num":
                expected = num(expected)
            elif kind == "flag":
                expected = flag(expected)
            elif kind == "text":
                text = expected.strip() if expected is not None else None
                expected = None if text in ABSENT or text is None else text
            actual = sql_row[sql_columns.index(sql_column)]
            actual = normalize(actual)
            if actual != expected:
                mismatches += 1
                if len(samples) < 12:
                    samples.append(
                        f"{row_id}.{sql_column}: typed={actual!r} slk={expected!r} "
                        f"[{sheet_name}.{sheet_column}]"
                    )
    connection.close()
    return checked, mismatches, samples


def text_value(value):
    return value


def num_value(value):
    if value is None:
        return None
    if isinstance(value, float) and value.is_integer():
        return int(value)
    return value


UNIT_MAP = {
    "race": ("UnitData", "race", "text"),
    "hp": ("UnitBalance", "HP", "num"),
    "mana": ("UnitBalance", "manaN", "num"),
    "level": ("UnitBalance", "level", "num"),
    "gold": ("UnitBalance", "goldcost", "num"),
    "lumber": ("UnitBalance", "lumbercost", "num"),
    "food_used": ("UnitBalance", "fused", "num"),
    "food_provided": ("UnitBalance", "fmade", "num"),
    "speed": ("UnitBalance", "spd", "num"),
    "armor": ("UnitBalance", "def", "num"),
}

ITEM_MAP_ROC = {
    "class": ("ItemData", "itemClass", "text"),
    "level": ("ItemData", "Level", "num"),
    "gold": ("ItemData", "goldcost", "num"),
    "lumber": ("ItemData", "lumbercost", "num"),
    "charges": ("ItemData", "uses", "num"),
    "stock_max": ("ItemData", "stockMax", "num"),
    "stock_regen": ("ItemData", "stockRegen", "num"),
    "stock_start": ("ItemData", "stockStart", "num"),
    "sellable": ("ItemData", "sellable", "flag"),
}

ITEM_MAP_TFT = dict(ITEM_MAP_ROC)
ITEM_MAP_TFT["class"] = ("ItemData", "class", "text")

UPGRADE_MAP = {
    "max_level": ("UpgradeData", "maxlevel", "num"),
    "gold_base": ("UpgradeData", "goldbase", "num"),
    "gold_per_level": ("UpgradeData", "goldmod", "num"),
    "lumber_base": ("UpgradeData", "lumberbase", "num"),
    "lumber_per_level": ("UpgradeData", "lumbermod", "num"),
    "time_base": ("UpgradeData", "timebase", "num"),
    "time_per_level": ("UpgradeData", "timemod", "num"),
}

ABILITY_MAP = {
    "hero": ("AbilityData", "hero", "flag"),
    "item": ("AbilityData", "item", "flag"),
    "use_in_editor": ("AbilityData", "useInEditor", "flag"),
    "levels": ("AbilityData", "levels", "num"),
}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", required=True)
    parser.add_argument("--root", required=True)
    args = parser.parse_args()

    root = os.path.abspath(args.root)
    tft = "war3x_data" in root
    plan = [
        ("units", UNIT_MAP, num_value),
        ("items", ITEM_MAP_TFT if tft else ITEM_MAP_ROC, text_value),
        ("upgrades", UPGRADE_MAP, num_value),
        ("abilities", ABILITY_MAP, num_value),
    ]
    total_mismatches = 0
    print(f"db={args.db}\nroot={root}")
    for table, mapped, normalize in plan:
        checked, mismatches, samples = compare(args.db, root, table, mapped, normalize)
        total_mismatches += mismatches
        print(f"{table:10} rows={checked:4} fields={len(mapped):2} mismatches={mismatches}")
        for sample in samples:
            print(f"    {sample}")
    print(f"total_mismatches={total_mismatches}")
    return 1 if total_mismatches else 0


if __name__ == "__main__":
    sys.exit(main())
