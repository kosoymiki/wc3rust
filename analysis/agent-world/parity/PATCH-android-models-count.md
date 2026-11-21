# Patch note: apps/android-native GAME_READY models count (outside r2-world owner)

Current: `apps/android-native/src/lib.rs:820` logs `models={}` as `assets.models.len()` (legacy 4).
Needed: report per-definition/registry model count so assets_models_min 4->542 is observable at runtime.
Not applied (outside OWNER_PATHS); deliver to android/assets integration owner.
