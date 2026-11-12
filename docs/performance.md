# Performance pass

Baseline: ba64c49. These are measurements on the same x86_64 Linux host
(Intel Xeon Platinum 8259CL), Rust 1.99.1, release profile unchanged. They are
not estimates for a desktop GPU or another machine.

## Offline workloads

`PERF_APPS=6000 cargo run --release --example perf` makes 6000 desktop-file
fixtures and 5000 clipboard entries. Each run reports the median of 100 app
search/frame samples and 50 clipboard search samples. The table below is the
median of three runs per binary, alternating baseline then changed binary.
The shadow benchmark uses 110 samples. Filesystem caches were warm.

| Workload | Before | After | Change |
| --- | ---: | ---: | ---: |
| App search, empty | 1.770 ms | 1.778 ms | +0.4% |
| App search, `f` | 5.980 ms | 5.276 ms | -11.8% |
| App search, `firefox` | 3.327 ms | 2.554 ms | -23.2% |
| App search, `zzz` (no match) | 2.032 ms | 1.196 ms | -41.2% |
| App search, `é` | 2.249 ms | 1.154 ms | -48.7% |
| Clipboard, all entries | 3.866 ms | 3.775 ms | -2.3% |
| Clipboard, `fixture` (all match) | 4.833 ms | 4.768 ms | -1.3% |
| Clipboard, `4999` | 2.999 ms | 0.916 ms | -69.5% |
| Clipboard, `missing` | 2.984 ms | 0.925 ms | -69.0% |
| Allocate and draw 640px frame, no icon I/O or Wayland | 5.648 ms | 5.377 ms | -4.8% |
| Two context-menu shadows | 1.319 ms | 0.357 ms | -73.0% |

Small differences in the empty/all-match workloads are noise, not a claimed
improvement. Search scores and row ordering are unchanged. The renderer does
not use a cheaper approximation: it reuses identical horizontal shadow
values and skips pixels the old outline loop already skipped individually.

## Daemon measurements

`tools/headless-sway.sh bash tests/headless.sh bash tests/performance-session.sh`
uses a private 1280x800 sway 1.7 session. Nine fresh daemon processes use warm
filesystem caches and no application fixtures. Startup ends when the daemon
answers `hide`. First-show and warm-show timing ends at the `show` IPC reply,
not at frame presentation. It must not be described as screen latency.

| Metric | Before | After |
| --- | ---: | ---: |
| Startup to ready reply, median | 47.35 ms | 45.67 ms |
| First show IPC reply, median | 0.163 ms | 0.148 ms |
| Warm show IPC reply, median | 0.899 ms | 0.867 ms |
| Hidden idle RSS | 8440 KiB | 8660 KiB |
| Shown idle RSS | 10732 KiB | 10784 KiB |
| Hidden CPU over 3 seconds | 0% | 0% |
| Shown CPU over 3 seconds | 1.33% | 1.33% |
| Stripped release binary | 7200000 bytes | 7289088 bytes |

There is no claimed startup, idle CPU, RSS or binary-size improvement.
Startup differences are small and noisy. The binary is 1.24% larger; hidden
RSS is 220 KiB higher in this run. CPU observations are short samples rounded
by the kernel's accounting ticks.

## Correctness checks

- Scalar pixel-oracle tests compare old and new shadow/outline drawing for
  fractional bounds, clipping, negative origins, alpha and radius variants.
- Clipboard-filter tests retain ellipsis and Unicode lowercase behavior.
- Baseline and changed offline frame images have identical pixels.
- Baseline and changed live context-menu empty/selected/pasted panel crops
  have identical pixels, including the shadow.
- Existing fuzzy-score tests pass without changing expected scores.

The benchmark includes fixture setup and clipboard timings in its printed
setup duration; that duration is not a startup measurement. The separate
daemon script provides the startup samples. Live compositor tests still own
input, scrolling and lifecycle behavior. Real desktop GPU behavior, cold
boot, very large clipboard payloads and long-duration memory trends were not
measured in this pass.
