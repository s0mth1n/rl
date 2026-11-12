set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
mkdir -p "$XDG_DATA_HOME/applications"
printf '[Desktop Entry]\nType=Application\nName=:foo\nExec=true\n' > "$XDG_DATA_HOME/applications/literal.desktop"
for mode in applications search calculator actions windows; do
    for query in ':foo' ';foo' '#foo'; do
        "$Z" show -m "$mode"
        sleep .2
        printf 't %s\n' "$query" | send_test_keys
        sleep .3
        grim "$OUT/$mode-${query:0:1}.png"
        if [ "$mode" = search ]; then
            python3 "$here/tests/ocr.py" "$OUT/$mode-${query:0:1}.png" "$OUT/search-ocr.png"
            env -u LD_LIBRARY_PATH tesseract "$OUT/search-ocr.png" "$OUT/search" --psm 6 2>/dev/null
            grep -qi 'Search on Google' "$OUT/search.txt"
        fi
        printf 'k ctrl-a\nk ctrl-c\n' | send_test_keys
        sleep .2
        [ "$(timeout 2 wl-paste -n)" = "$query" ]
        printf 'k bs\n' | send_test_keys
        sleep .2
        "$Z" hide
        sleep .2
    done
done
echo 'PASS all direct nonpicker modes copy and delete leading picker characters literally'
