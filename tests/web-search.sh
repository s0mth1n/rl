#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
mkdir -p "$OUT/bin"
cat > "$OUT/bin/xdg-open" <<'PY'
#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
with Path(os.environ['OUT'], 'opened.jsonl').open('a') as f:
    f.write(json.dumps({'argv': sys.argv[1:], 'pid': os.getpid(), 'sid': os.getsid(0)}) + '\n')
PY
chmod +x "$OUT/bin/xdg-open"
restart() {
    "$Z" quit
    sleep .2
    PATH="$OUT/bin:$PATH" "$Z" > "$OUT/restart-$1.log" 2>&1 &
    record_test_pid "$!"
    sleep .5
}
restart defaults
fifo="$XDG_RUNTIME_DIR/search-keyboard.fifo"
mkfifo "$fifo"
exec {ZL_TEST_KEYBOARD_FD}<>"$fifo"
"${ZL_KEYBOARD_BIN:-$here/target/release/examples/vkbd}" < "$fifo" &
ZL_TEST_KEYBOARD_PID=$!
record_test_pid "$ZL_TEST_KEYBOARD_PID"
query() {
    "$Z" hide
    "$Z" show -m "$1"
    sleep .2
    printf 't %s\ns .25\n' "$2" | send_test_keys
}
shot() {
    sleep .2
    grim "$OUT/$1.png"
    python3 "$here/tests/ocr.py" "$OUT/$1.png" "$OUT/$1-ocr.png"
    env -u LD_LIBRARY_PATH tesseract "$OUT/$1-ocr.png" "$OUT/$1" --psm 6 2>/dev/null
}
query search '!g rust + & /?'
shot triggered
! grep -qiE 'Ask AI|Use an LLM' "$OUT/triggered.txt"
grep -qi 'Search on Google' "$OUT/triggered.txt"
printf 'k enter\ns .3\n' | send_test_keys
python3 - "$OUT/opened.jsonl" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1])]
assert rows[0]['argv'] == ['https://www.google.com/search?q=rust%20%2B%20%26%20%2F%3F'], rows
assert rows[0]['pid'] == rows[0]['sid'], rows
PY
echo 'PASS explicit trigger exact URL argv and detached xdg-open'
query combined '!wiki Rust'
shot no-ai
! grep -qiE 'Ask AI|Use an LLM' "$OUT/no-ai.txt"
grep -qi 'Search on Wikipedia' "$OUT/no-ai.txt"
echo 'PASS explicit web trigger selects one provider in combined mode'
query combined 'unmatched-query'
shot fallback
for text in 'Search on Google' 'Search on DuckDuckGo' 'Search on Wikipedia' 'Search on YouTube'; do
    grep -qi "$text" "$OUT/fallback.txt" || { echo "FAIL missing $text"; exit 1; }
done
echo 'PASS ordinary query shows all fallback providers without AI'
query combined '!ai explain rust'
shot ai-literal
! grep -qi 'Use an LLM' "$OUT/ai-literal.txt"
grep -qi 'Search on Google' "$OUT/ai-literal.txt"
echo 'PASS !ai is ordinary search text'
query search '!g'
shot trigger-only
grep -qi 'No items found' "$OUT/trigger-only.txt"
query search 'ordinary query'
shot direct-fallback
! grep -qiE 'Ask AI|Use an LLM' "$OUT/direct-fallback.txt"
printf 'k enter\ns .3\n' | send_test_keys
python3 - "$OUT/opened.jsonl" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1])]
assert rows[-1]['argv'] == ['https://www.google.com/search?q=ordinary%20query'], rows
PY
echo 'PASS direct search empty trigger and fallback activation'
cat > "$XDG_CONFIG_HOME/zl/config.toml" <<'TOML'
[[search_providers]]
name = 'Local Docs'
trigger = '!doc'
url = 'https://example.invalid/docs/{query}?again={query}'
icon = ' BOOK-OPEN '
TOML
restart custom
query search '!doc Rust & C'
shot custom
grep -qi 'Search on Local Docs' "$OUT/custom.txt"
! grep -qi 'Search on Google' "$OUT/custom.txt"
printf 'k enter\ns .3\n' | send_test_keys
python3 - "$OUT/opened.jsonl" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1])]
assert rows[-1]['argv'] == ['https://example.invalid/docs/Rust%20%26%20C?again=Rust%20%26%20C'], rows
PY
echo 'PASS custom providers replace defaults and repeated URL placeholders'
printf 'search_providers = []\n' > "$XDG_CONFIG_HOME/zl/config.toml"
restart empty
query search 'no provider'
shot empty
grep -qi 'No items found' "$OUT/empty.txt"
echo 'PASS empty provider list disables search rows'
"$Z" quit
sleep .2
printf '' > "$XDG_CONFIG_HOME/zl/config.toml"
mkdir -p "$OUT/empty-path"
PATH="$OUT/empty-path" "$Z" > "$OUT/restart-missing.log" 2>&1 &
record_test_pid "$!"
sleep .5
query search '!g spawn failure'
printf 'k enter\ns .3\n' | send_test_keys
grep -q 'failed to open search URL' "$OUT/restart-missing.log"
grim "$OUT/spawn-failure-hidden.png"
python3 - "$OUT/spawn-failure-hidden.png" "$OUT/opened.jsonl" <<'PY'
import sys
from PIL import Image
assert Image.open(sys.argv[1]).convert('RGB').getpixel((640, 400)) == (32, 36, 44)
assert len(open(sys.argv[2]).readlines()) == 3
PY
echo 'PASS missing xdg-open logs spawn failure and still hides'
"$Z" hide
