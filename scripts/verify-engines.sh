#!/usr/bin/env bash
# The engine gate: builds the native libraries and runs the Unity and Unreal tests in the
# editors, without a window but on the real GPU (so what is drawn is read back and checked).
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

    step "unity: EditMode and PlayMode tests in $editor"
    local project="$WORK/unity"
    mkdir -p "$project/Packages" "$project/Assets"
    # The physics module is for the ZuriQuad PlayMode test (collider, PhysicsRaycaster).
    cat > "$project/Packages/manifest.json" <<EOF
{
  "dependencies": {
    "com.seedexr.zuri": "file:$ROOT/integrations/unity",
    "com.unity.modules.physics": "1.0.0",
    "com.unity.test-framework": "1.6.0"
  },
  "testables": ["com.seedexr.zuri"]
}
EOF
    # Compile the scripts afresh: a cached build would not repeat its warnings in the log.
    rm -rf "$project/Library/Bee" "$project/Library/ScriptAssemblies"
    # The tests that read rendered pixels or open an editor window need a GPU, so Unity runs
    # with a real graphics device (no -nographics). A test that is skipped fails the gate.
    local name run diagnostics settings="$project/ProjectSettings/ProjectSettings.asset"
    for name in EditMode PlayMode-Gamma PlayMode-Linear; do
        # The PlayMode tests run once per color space. Unity writes the settings file when
        # the EditMode run opens the project.
        case "$name" in
            *-Gamma) sed -i '' 's/m_ActiveColorSpace: .*/m_ActiveColorSpace: 0/' "$settings" ;;
            *-Linear) sed -i '' 's/m_ActiveColorSpace: .*/m_ActiveColorSpace: 1/' "$settings" ;;
        esac
        rm -f "$project/$name.xml"
        "$editor" -batchmode -projectPath "$project" -runTests -testPlatform "${name%-*}" \
            -testResults "$project/$name.xml" -logFile "$project/$name.log" || true
        [ -f "$project/$name.xml" ] || fail "Unity wrote no $name test results; see $project/$name.log"
        run="$(grep -m1 -o '<test-run [^>]*' "$project/$name.xml" || true)"
        printf '%s: ' "$name"
        echo "$run" | grep -o 'result="[^"]*"\|total="[^"]*"\|passed="[^"]*"\|failed="[^"]*"\|skipped="[^"]*"' | tr '\n' ' ' || true
        grep -m1 -o "device=.*colorSpace=.*" "$project/$name.log" || echo
        diagnostics="$(grep -E "error CS|warning CS" "$project/$name.log" | sort -u || true)"
        [ -z "$diagnostics" ] || { echo "$diagnostics" | head -5; fail "the Unity package compiles with errors or warnings"; }
        [[ $run != *' total="0"'* ]] || fail "Unity ran no $name tests: is the package still in the project's testables?"
        [[ $run == *'result="Passed"'*' skipped="0"'* ]] || fail "Unity $name tests did not all pass (a skipped test fails the gate too); see $project/$name.xml"
        [[ $name != PlayMode-* ]] || grep -q "colorSpace=${name#*-}" "$project/$name.log" || fail "the $name tests did not run in the ${name#*-} color space; see $project/$name.log"
    done
    # Every PlayMode test ran: one going missing (a module dropped from the manifest above,
    # a define that stopped resolving) must not pass as "all of the rest passed".
    grep -q 'total="6"' <<<"$run" || fail "expected 6 PlayMode tests; see $project/PlayMode-Linear.xml (update this number when adding one)"
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

    # The tests that read pixels back need a GPU, so the editor runs on the real RHI, off
    # screen. On a Mac that needs the Xcode Metal Toolchain.
    xcrun -f metal > /dev/null 2>&1 || fail "no Xcode Metal Toolchain: xcodebuild -downloadComponent MetalToolchain"
    step "unreal: automation tests (on the GPU, no window; one short sound is played)"
    local project="$WORK/unreal/TestProject" out="$WORK/unreal/automation.log"
    mkdir -p "$project/Plugins"
    printf '{ "FileVersion": 3, "EngineAssociation": "%s", "Plugins": [ { "Name": "Zuri", "Enabled": true } ] }\n' "${engine##*UE_}" > "$project/ZuriTest.uproject"
    rm -rf "$project/Plugins/Zuri" && cp -R "$package" "$project/Plugins/Zuri"
    # Nothing goes to Epic from the test project. By default the editor uploads a crash that happens
    # unattended (a test that crashes), with its log and so the paths of this machine, and sends usage data.
    mkdir -p "$project/Config"
    printf '[/Script/UnrealEd.CrashReportsPrivacySettings]\nbSendUnattendedBugReports=False\n\n[/Script/UnrealEd.AnalyticsPrivacySettings]\nbSendUsageData=False\n' > "$project/Config/DefaultEditorSettings.ini"
    "$engine/Engine/Binaries/Mac/UnrealEditor-Cmd" "$project/ZuriTest.uproject" -ExecCmds="Automation RunTests Zuri; Quit" \
        -unattended -RenderOffscreen -nosplash -stdout -FullStdOutLogOutput > "$out" 2>&1 || true
    grep -E "Test Completed|TEST COMPLETE" "$out" | sed 's/^.*Display: //' || true
    grep -m1 -o "Zuri GPU tests run on the .* RHI" "$out" || fail "the Unreal tests that draw did not run; see $out"
    # The sound test starts a 60 ms click on the machine's audio output, and leaves that part out without one.
    grep -m1 -o "Zuri sound test played on the audio device" "$out" || fail "the Unreal sound test found no audio device; see $out"
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
