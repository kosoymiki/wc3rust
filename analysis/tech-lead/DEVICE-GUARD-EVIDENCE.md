# Device guard — evidence (2026-11-18, ledger 560)

Policy: `analysis/tech-lead/DEVICE-POLICY.md`. Both walk scripts now fail closed
**before** any `su`/`am`/`input`/`logcat` call.

## Implementation

- `analysis/tech-lead/device_menu_walk.py`: `_require_device_ok()` checks
  `H2_DEVICE_OK == "1"` at import (skipped only for `-h/--help`) and again inside
  `sh()`.
- `analysis/tech-lead/device_battle_walk.py`: same, plus inside `sh_bytes()`.
- Failure prints exactly
  `device access is operator-gated (analysis/tech-lead/DEVICE-POLICY.md)` to stderr
  and exits **2**.

## Observed

```
$ env -u H2_DEVICE_OK python3 analysis/tech-lead/device_menu_walk.py
device access is operator-gated (analysis/tech-lead/DEVICE-POLICY.md)
menu_rc=2

$ env -u H2_DEVICE_OK python3 analysis/tech-lead/device_battle_walk.py
device access is operator-gated (analysis/tech-lead/DEVICE-POLICY.md)
battle_rc=2

$ env -u H2_DEVICE_OK python3 analysis/tech-lead/device_menu_walk.py --help >/dev/null
menu_help_rc=0

$ env -u H2_DEVICE_OK python3 analysis/tech-lead/device_battle_walk.py --help >/dev/null
battle_help_rc=0

$ H2_DEVICE_OK=0 python3 analysis/tech-lead/device_menu_walk.py
menu_env0_rc=2

$ H2_DEVICE_OK=1 python3 analysis/tech-lead/device_menu_walk.py --help >/dev/null
menu_help_env1_rc=0

$ python3 -m py_compile analysis/tech-lead/device_menu_walk.py analysis/tech-lead/device_battle_walk.py
py_compile_rc=0
```

No `am`/`input`/`pm`/`screencap`/`logcat -c` command was executed during this
verification; the gate returns before `sh()` on every path above.
