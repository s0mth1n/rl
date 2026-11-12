#!/usr/bin/env bash
# Mode origin: entering a sub-view from the main list shows a back arrow in the
# input prefix; clicking it restores the combined query. A sub-view entered by
# typing its prefix shows no arrow, and clicking the prefix zone does nothing.
P="$here/target/release/examples/vptr"
set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
type_query() { "$Z" hide; "$Z" show; sleep .15; printf 't %s\ns 0.25\n' "$1" | send_test_keys; }
shot() {
    sleep .3
    grim "$OUT/$1.png"
    python3 "$here/tests/ocr.py" "$OUT/$1.png" "$OUT/$1-ocr.png"
    env -u LD_LIBRARY_PATH tesseract "$OUT/$1-ocr.png" "$OUT/$1" --psm 6 2>/dev/null
}
check_text() { grep -qi "$2" "$OUT/$1.txt" || { echo "FAIL $1 missing $2"; cat "$OUT/$1.txt"; exit 1; }; }
PANEL_LEFT=$(( (1280 - 600) / 2 ))
BACK_X=$(( PANEL_LEFT + 27 ))

# Enter Clipboard History from the main list: the arrow appears and goes back.
"$Z" show
sleep .4
type_query 'clip'
printf 'k enter\ns 0.3\n' | send_test_keys
shot clipview
check_text clipview 'No clipboard history'   # empty clipboard sub-view
CLIP_TOP=$(python3 - "$OUT/clipview.png" <<'PY'
import sys
from PIL import Image
img = Image.open(sys.argv[1]).convert('RGB')
w, h = img.size
# The panel is centred; find its top edge by the first non-background row.
bg = img.getpixel((20, 20))
for y in range(h):
    row = [img.getpixel((x, y)) for x in range(w // 2 - 100, w // 2 + 100, 20)]
    if any(sum(abs(a - b) for a, b in zip(px, bg)) > 30 for px in row):
        print(y)
        break
PY
)
printf 'm %s %s\np\nr\ns 0.4\n' "$BACK_X" "$(( CLIP_TOP + 28 ))" | "$P"
shot backroot
check_text backroot 'Clipboard History'   # back at the root view
check_text backroot 'clip'            # the combined query was restored
echo 'PASS back arrow restores the combined view'

# Direct prefix entry shows no arrow; clicking the prefix zone keeps the view.
type_query ';'
shot clipdirect
check_text clipdirect 'No clipboard history'
CLIP_TOP=$(python3 - "$OUT/clipdirect.png" <<'PY'
import sys
from PIL import Image
img = Image.open(sys.argv[1]).convert('RGB')
w, h = img.size
bg = img.getpixel((20, 20))
for y in range(h):
    row = [img.getpixel((x, y)) for x in range(w // 2 - 100, w // 2 + 100, 20)]
    if any(sum(abs(a - b) for a, b in zip(px, bg)) > 30 for px in row):
        print(y)
        break
PY
)
printf 'm %s %s\np\nr\ns 0.4\n' "$BACK_X" "$(( CLIP_TOP + 28 ))" | "$P"
shot clipstill
check_text clipstill 'No clipboard history'  # still the clipboard view: no arrow
echo 'PASS direct sub-view has no back arrow'

# The theme picker entered directly shows its own prefix icon.
type_query '#'
shot themes
check_text themes 'Search themes'
echo 'PASS theme picker view'
