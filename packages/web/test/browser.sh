#!/usr/bin/env bash
# Runs test/browser.html in headless Chrome and checks what it measured: the canvas player,
# pointer input, data binding, events, WebGL2 shader paints, text, 3D, the custom element.
# Needs: built package (pnpm --filter @seedexr/zuri build) and examples (scripts/build-examples.sh).
set -euo pipefail
cd "$(dirname "$0")/../../.."
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
[[ -x "$CHROME" ]] || CHROME=$(command -v google-chrome chromium chromium-browser | head -1 || true)
if [[ ! -x "$CHROME" ]]; then
  # A gate that skips silently is not a gate: skipping has to be asked for.
  [[ -n "${ZURI_SKIP_BROWSER:-}" ]] && { echo "browser checks skipped (ZURI_SKIP_BROWSER is set)"; exit 0; }
  echo "Chrome not found: set CHROME=/path/to/chrome, or ZURI_SKIP_BROWSER=1 to skip the browser checks on purpose" >&2
  exit 1
fi
PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
python3 -m http.server "$PORT" --bind 127.0.0.1 >/dev/null 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null' EXIT
sleep 0.5
"$CHROME" --headless=new --no-first-run --window-size=900,1400 --virtual-time-budget=20000 \
  --user-data-dir="$(mktemp -d)" --dump-dom "http://127.0.0.1:$PORT/packages/web/test/browser.html" 2>/dev/null \
  | python3 -c '
import json, re, sys
dom = sys.stdin.read()
m = re.search(r"<pre id=\"result\">(.*?)</pre>", dom, re.S)
assert m and m.group(1), "the page produced no result"
r = json.loads(m.group(1).replace("&quot;", "\"").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">"))
print(json.dumps(r, indent=1))
assert r.get("ok"), r.get("error")
def check(cond, what):
    assert cond, "FAILED: " + what
    print("ok  ", what)
check(r["buttonSize"] == [320, 160], "canvas backing store follows CSS size")
check(r["buttonIdle"] == [0x4f, 0x7c, 0xff, 255], "button renders its blue")
check(r["clicks"] == 1 and r["hover"] is True, "pointer events reach listeners (hover, click)")
check(r["events"] == ["clicked"], "the click event is reported once")
check(r["buttonRecolored"][:3] != r["buttonIdle"][:3] and r["buttonRecolored"][0] > 200, "setColor recolors through the binding")
fallback = [0x5b, 0x3d, 0xf5, 255]
check(r["shaderRequests"] == 1, "the scene asks the host for one shader texture")
check(r["shaderLater"] != fallback, "WebGL2 drew the shader (not the fallback color)")
check(r["shaderLater"] != r["shaderEarly"], "the shader animates with time")
check(r["shaderOutsideCard"] == [0x0e, 0x10, 0x16, 255], "the shader is clipped to its shape")
check(r["shaderDotOnTop"][:3] == [255, 255, 255], "vector content still draws over the shader")
check(r["textInk"] > 1000 and r["textInkLonger"] > r["textInk"] + 500, "text renders and follows its bound string")
check(r["modelCentre"][3] == 255, "the 3D model renders")
check(r["elementBackground"][3] == 255, "<zuri-player> loads and renders")
check(r["destroyed"], "pause() stops the loop and destroy() releases without error")
print("browser checks passed")
'
