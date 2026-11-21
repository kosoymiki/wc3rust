# Device build / screenshot evidence (2026-11-12)

- Release build: `cargo build --release -p herofolends-android` rc=0 in 2m46s (`analysis/agent-world/apk-build.log`, BUILD_EXIT=0).
- APK package: `scripts/build-apk.sh` rc=0, v2+v3 signature verified (`analysis/agent-world/apk-package.log`).
  - `herofolends-p0.apk` sha256 `c9776d142c6887f00e74a938d40cdb339c814fe31e2e7446ba2327559aabb7c2`
  - `target/release/libmain.so` sha256 `5dbb9e7e4ed0d511fbed48f05da3b19d1056aaefcaa1087205dce2fbc8bd4fc9`
- Install: `su -c 'pm install -r build/android-p0/herofolends-p0.apk'` -> `Success` (lastUpdateTime 2026-11-12 16:20:52).
- Launch: `am start -n org.herofolends.p0/org.libsdl.app.SDLActivity` -> app process pid 9719.
- App log (logcat, tag wc3rust):
  - `GAME_VULKAN turnip=0 (no ICD; vendor driver)`
  - `P0_SDL_INIT video_driver=android`
  - then Android destroyed the surface: `ViewRootImpl: SkipDraw reason : screen_off`, `setWindowStopped stopped:true`, `SDL: surfaceDestroyed()/nativePause()`.
- Device state: `mWakefulness=Dozing`, `mScreenLocked=true`, focus on `NotificationShade` / `com.android.incallui.OplusInCallActivity`; `screencap` returns a black 1080x2414 frame. The owner lock screen cannot be dismissed without the owner's credentials (not attempted: auth boundary).
- Consequence: an on-device screenshot of the new build is pending owner unlock. The headless harness uses the same `Renderer`/wgpu pipelines at 2414x1080 (`analysis/agent-world/terrain-after.ppm`, `units-after.ppm`, `heroes-after.ppm`) and is the visual evidence for this delivery.
