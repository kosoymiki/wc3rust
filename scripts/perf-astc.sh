#!/data/data/com.termux/files/usr/bin/bash
# ASTC asset-pipeline step: encode source images with `astcenc` and report the
# exact bytes + PSNR against the source.
#
#   scripts/perf-astc.sh [image ...]     # default: the repo bench parity frame
#
# The renderer never encodes at runtime; it consumes `.astc` files through
# `wc3_render::astc` + `TextureStore::upload_sources` (see
# crates/wc3-render/tests/astc.rs for the on-device decode proof).
#
# astcenc is built once into $ASTC_DIR (aarch64 NEON). Clang 21 rejects the
# upstream `-ffp-model=precise` + `-ffp-contract=off` combination under -Werror,
# so the clone is patched to raise the clang version guard for that flag.
set -u
cd "$(dirname "$0")/.." || exit 1

ASTC_DIR="${ASTC_DIR:-/data/data/com.termux/files/usr/tmp/opencode/astc}"
SRC_DIR="$ASTC_DIR/astc-encoder"
BIN="$SRC_DIR/build-neon/Source/astcenc-neon"
OUT_DIR="analysis/agent-perf/astc"
mkdir -p "$OUT_DIR"

build_astcenc() {
    [ -x "$BIN" ] && return 0
    mkdir -p "$ASTC_DIR"
    if [ ! -d "$SRC_DIR" ]; then
        echo "perf-astc: cloning astc-encoder 4.8.0"
        git clone --depth 1 --branch 4.8.0 \
            https://github.com/ARM-software/astc-encoder.git "$SRC_DIR" || return 1
    fi
    python3 - "$SRC_DIR/Source/cmake_core.cmake" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1])
s = p.read_text()
s = s.replace(
    '-ffp-model=precise', '-ffp-model=precise')  # keep for non-clang
s = s.replace(
    '$<AND:${is_clang},$<VERSION_GREATER_EQUAL:$<CXX_COMPILER_VERSION>,10.0.0>>:-ffp-model=precise>',
    '$<AND:${is_clang},$<VERSION_GREATER_EQUAL:$<CXX_COMPILER_VERSION>,999.0.0>>:-ffp-model=precise>',
)
p.write_text(s)
PY
    cmake -B "$SRC_DIR/build-neon" -G Ninja -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_CXX_COMPILER=clang++ -DASTCENC_ISA_AVX2=OFF -DASTCENC_ISA_AVX512=OFF \
        -DASTCENC_ISA_SSE2=OFF -DASTCENC_ISA_NEON=ON -DASTCENC_UNITTEST=OFF \
        > "$ASTC_DIR/cmake.log" 2>&1 || return 1
    cmake --build "$SRC_DIR/build-neon" -j2 > "$ASTC_DIR/build.log" 2>&1 || return 1
    [ -x "$BIN" ]
}

if ! build_astcenc; then
    echo "perf-astc: astcenc build failed (see $ASTC_DIR/*.log)"
    exit 2
fi
echo "perf-astc: astcenc $("$BIN" -v | head -1)"

# Default input: the rendered Human04 frame from the bench (real game content).
if [ "$#" -eq 0 ]; then
    set -- analysis/agent-perf/bench-results/parity.baseline.ppm
fi

# PSNR between a PPM/TGA/PNG source and a decoded TGA, via python (no PIL).
psnr_py='
import sys, struct, math

def read_tga(path):
    with open(path, "rb") as handle:
        header = handle.read(18)
        id_length = header[0]
        image_type = header[2]
        width = struct.unpack("<H", header[12:14])[0]
        height = struct.unpack("<H", header[14:16])[0]
        bpp = header[16]
        desc = header[17]
        handle.read(id_length)
        raw = bytearray(handle.read())
    if bpp not in (24, 32):
        raise SystemExit(f"unsupported TGA depth {bpp}")
    step = bpp // 8
    pixels = bytearray(width * height * step)
    if image_type == 2:
        pixels[:] = raw[:len(pixels)]
    elif image_type == 10:
        src = 0
        dst = 0
        total = width * height
        while dst < total:
            packet = raw[src]
            src += 1
            count = (packet & 0x7F) + 1
            if packet & 0x80:
                pixel = raw[src:src + step]
                src += step
                for _ in range(count):
                    pixels[dst * step:(dst + 1) * step] = pixel
                    dst += 1
            else:
                for _ in range(count):
                    pixels[dst * step:(dst + 1) * step] = raw[src:src + step]
                    src += step
                    dst += 1
    else:
        raise SystemExit(f"unsupported TGA type {image_type}")
    out = bytearray(width * height * 3)
    flip = not (desc & 0x20)
    for y in range(height):
        row = (height - 1 - y) if flip else y
        for x in range(width):
            i = (row * width + x) * step
            out[(y * width + x) * 3:(y * width + x) * 3 + 3] = pixels[i:i + 3]
    return width, height, out

def read_ppm(path):
    with open(path, "rb") as handle:
        assert handle.readline().strip() == b"P6"
        line = handle.readline().split()
        while line and line[0].startswith(b"#"):
            line = handle.readline().split()
        width, height = int(line[0]), int(line[1])
        handle.readline()
        return width, height, bytearray(handle.read())

def read_any(path):
    with open(path, "rb") as handle:
        magic = handle.read(2)
    if magic == b"P6":
        return read_ppm(path)
    return read_tga(path)

w1, h1, a = read_any(sys.argv[1])
w2, h2, b = read_any(sys.argv[2])
assert (w1, h1) == (w2, h2), (w1, h1, w2, h2)
n = len(a)
mse = sum((a[i] - b[i]) ** 2 for i in range(n)) / n
psnr = float("inf") if mse == 0 else 10 * math.log10(255 * 255 / mse)
print(f"{psnr:.2f} {math.sqrt(mse):.2f}")
'

total_rgba=0
total_astc=0
for src_img in "$@"; do
    img="$src_img"
    case "$img" in
        *.ppm|*.pnm)
            tga_src="$OUT_DIR/$(basename "${img%.*}").tga"
            python3 "$OUT_DIR/../ppm_to_tga.py" "$img" "$tga_src"
            img="$tga_src"
            ;;
    esac
    base=$(basename "$img"); stem="${base%.*}"
    for block in 6x6 8x8; do
        enc="$OUT_DIR/$stem-$block.astc"
        dec="$OUT_DIR/$stem-$block.tga"
        if ! "$BIN" -cl "$img" "$enc" "$block" -medium > "$OUT_DIR/$stem-$block.encode.log" 2>&1; then
            echo "perf-astc: encode failed for $img $block"
            continue
        fi
        "$BIN" -dl "$enc" "$dec" > "$OUT_DIR/$stem-$block.decode.log" 2>&1
        read -r psnr rmse < <(python3 -c "$psnr_py" "$img" "$dec")
        bytes=$(stat -c %s "$enc")
        # source bytes for the same image (RGB or RGBA)
        src=$(stat -c %s "$img")
        bpp=$(python3 - "$block" <<'PY'
import sys
b=sys.argv[1]
print({'6x6': '3.56', '8x8': '2.00'}[b])
PY
)
        echo "ASTC image=$base block=$block file_bytes=$bytes source_bytes=$src psnr_db=$psnr rmse=$rmse bpp=$bpp"
        total_astc=$((total_astc + bytes))
        total_rgba=$((total_rgba + src))
    done
done
echo "ASTC total encoded_bytes=$total_astc source_bytes=$total_rgba"
