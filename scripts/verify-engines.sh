#!/usr/bin/env bash
# The engine gate: builds the native libraries and runs the Unity and Unreal tests headless.
# Run it after any change to the C ABI, the Unity package or the Unreal plugin. It is not
# part of verify.sh because it needs the engines installed and takes several minutes.
#
#   bash scripts/verify-engines.sh            # both engines
#   bash scripts/verify-engines.sh unity      # or: unreal
#
# macOS only so far. The engines are looked up in their default install folders; override with
#   UNITY=/path/to/Unity.app/Contents/MacOS/Unity
#   UE_ROOT="/Users/Shared/Epic Games/UE_5.8"
# The throwaway projects it tests in live in target/engines/ (made on first run, safe to delete).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$ROOT/target/engines"
cd "$ROOT"
step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
fail() { printf '\033[31mFAILED: %s\033[0m\n' "$*" >&2; exit 1; }

unity() {
    local editor="${UNITY:-$(ls -d /Applications/Unity/Hub/Editor/*/Unity.app/Contents/MacOS/Unity 2>/dev/null | sort -V | tail -1)}"
    [ -x "$editor" ] || fail "Unity not found; set UNITY to the editor binary"

    step "unity: native plugin, then the C# bindings in plain dotnet"
    bash scripts/build-unity-plugins.sh
    "integrations/unity/Tests~/DotnetHarness/run.sh" | tail -1

    step "unity: EditMode tests in $editor"
    local project="$WORK/unity"
    mkdir -p "$project/Packages" "$project/Assets"
    cat > "$project/Packages/manifest.json" <<EOF
{
  "dependencies": {
    "com.seedexr.zuri": "file:$ROOT/integrations/unity",
    "com.unity.test-framework": "1.6.0"
  },
  "testables": ["com.seedexr.zuri"]
}
EOF
    # Compile the scripts afresh: a cached build would not repeat its warnings in the log.
    rm -rf "$project/results.xml" "$project/Library/Bee" "$project/Library/ScriptAssemblies"
    "$editor" -batchmode -nographics -projectPath "$project" -runTests -testPlatform EditMode \
        -testResults "$project/results.xml" -logFile "$project/editor.log" || true
    [ -f "$project/results.xml" ] || fail "Unity wrote no test results; see $project/editor.log"
    local run diagnostics
    run="$(grep -m1 -o '<test-run [^>]*' "$project/results.xml" || true)"
    echo "$run" | grep -o 'result="[^"]*"\|total="[^"]*"\|passed="[^"]*"\|failed="[^"]*"' | tr '\n' ' '
    echo
    diagnostics="$(grep -E "error CS|warning CS" "$project/editor.log" | sort -u || true)"
    [ -z "$diagnostics" ] || { echo "$diagnostics" | head -5; fail "the Unity package compiles with errors or warnings"; }
    case "$run" in
        *'result="Passed"'*) ;;
        *) fail "Unity tests did not pass; see $project/results.xml" ;;
    esac
    case "$run" in
        *' total="0"'*) fail "Unity ran no tests: is the package still in the project's testables?" ;;
    esac
}

unreal() {
    local engine="${UE_ROOT:-$(ls -d "/Users/Shared/Epic Games/UE_"* 2>/dev/null | sort -V | tail -1)}"
    [ -d "$engine" ] || fail "Unreal Engine not found; set UE_ROOT"

    step "unreal: static library and header"
    bash scripts/build-unreal-libs.sh | tail -2

    step "unreal: RunUAT BuildPlugin in $engine"
    local package="$WORK/unreal/ZuriPackage" log="$WORK/unreal/build.log"
    mkdir -p "$WORK/unreal"
    # Another project's build can hold UnrealBuildTool's lock: wait for it a few times.
    for attempt in 1 2 3 4 5 6; do
        rm -rf "$package"
        "$engine/Engine/Build/BatchFiles/RunUAT.sh" BuildPlugin -Plugin="$ROOT/integrations/unreal/Zuri/Zuri.uplugin" \
            -Package="$package" -TargetPlatforms=Mac -WaitForUATMutex > "$log" 2>&1 || true
        grep -qi "conflicting instance" "$log" || break
        echo "UnrealBuildTool is busy (attempt $attempt); retrying in 60 s"
        sleep 60
    done
    grep -q "BUILD SUCCESSFUL" "$log" || { grep -n "error" "$log" | head -20; fail "BuildPlugin failed; see $log"; }
    echo "BUILD SUCCESSFUL"

    step "unreal: automation tests (headless, no RHI)"
    local project="$WORK/unreal/TestProject" out="$WORK/unreal/automation.log"
    mkdir -p "$project/Plugins"
    printf '{ "FileVersion": 3, "EngineAssociation": "%s", "Plugins": [ { "Name": "Zuri", "Enabled": true } ] }\n' "${engine##*UE_}" > "$project/ZuriTest.uproject"
    rm -rf "$project/Plugins/Zuri" && cp -R "$package" "$project/Plugins/Zuri"
    "$engine/Engine/Binaries/Mac/UnrealEditor-Cmd" "$project/ZuriTest.uproject" -ExecCmds="Automation RunTests Zuri; Quit" \
        -unattended -nullrhi -nosplash -nosound -stdout -FullStdOutLogOutput > "$out" 2>&1 || true
    grep -E "Test Completed|TEST COMPLETE" "$out" | sed 's/^.*Display: //' || true
    # Every test the editor found must have completed with success, and the run must have ended.
    local found passed
    found="$(grep -o "Found [0-9]* automation tests" "$out" | grep -o "[0-9]*" | head -1 || true)"
    passed="$(grep -c "Test Completed. Result={Success}" "$out" || true)"
    [ "${found:-0}" -gt 0 ] || fail "Unreal found no Zuri tests; see $out"
    [ "$passed" = "$found" ] || fail "$passed of $found Unreal tests succeeded; see $out"
    grep -q "TEST COMPLETE. EXIT CODE: 0" "$out" || fail "the Unreal test run did not finish cleanly; see $out"
}

case "${1:-all}" in
    unity) unity ;;
    unreal) unreal ;;
    all) unity; unreal ;;
    *) fail "usage: $0 [unity|unreal]" ;;
esac
printf '\n\033[32mOK (engines)\033[0m\n'
