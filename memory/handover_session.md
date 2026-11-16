# Handover log

Newest entry first. Each entry: what was done, what changed architecturally, what was
verified and how, what is open. Timestamps are local time, `YYYY-MM-DD HH:MM:SS`.

---

## 2026-11-16 10:00:00 - Session 2 (continued): elastic keys

(Joysticks below went to `main` as d92135e, CI green on the three systems.)

**Done.** A Key's `interp` can be `elastic` (swinging on arriving), `elasticIn` (on
leaving) or `elasticInOut` (both): the value swings past the next key's and back, each
swing smaller, and is exactly there at the end. `amplitude` (1 or more) and `period`
(a swing's length as a part of the segment: 0.5 by default, at least 0.05) are keys
129 and 130. These are the three options Rive's page names (Easing, Amplitude,
Period). The curve is this project's: `1 + a (1 - t)^6 sin((t - shift) turn)`, shifted
to start at 0. Rive's page gives no formula and no defaults, so a file tuned by eye
against Rive may look different; the usual `2^(-10 t)` decay would need `exp` in the
wasm, and this one ends exactly on the value. What a key needs for it (reach, turn,
shift) is worked out once, where the Key is built.

**Code review: 10 findings.** The direction was a property called `easing` that only
elastic keys read: a name too general to spend for good, so the direction is in the
`interp` name now and key 131 was never published. `elasticInOut` ran a swing in half
the stated `period`: it keeps the period now. A Transition accepted `elastic` and
faded linearly: the compiler refuses it there and says what a Transition takes. A
period of 0 meant a thousand swings: floored at 0.05. The per-key constants were
recomputed at every sample, with an `acos` that lost its footing at very large
amplitudes: computed once per key, with `asin`. The editor's timeline drew an elastic
key as a square (which means `hold`): it has a shape of its own. The format guide's
example was not from a test: it is the scene test's now. Colors: they do not go past
the next key's color, which the format guide now says.

**Ponytail review: 2 findings.** Applied: one evaluation of the curve instead of four.
**Refused:** dropping the in / out / in-out direction as unused. Rive has it, and
parity with Rive is what the owner asked for; it costs no property now that it is part
of `interp`.

**Verified.** `bash scripts/verify.sh` exits 0. A unit test of the curve (ends at
three amplitudes and periods, first peak, amplitude, the three ways) and a scene test
with hand-computed positions (117.8, 35.6, 50, -0.8, 185.9) and the compiler's refusal
on a Transition; thirteen breaks (seven on the first version, six on the rework) each
failed one of them. Three of my own expectations in the tests were wrong on the first
run and are corrected. Sizes: 79.0 KB lite (limit 80: under 1 KB left), 132 KB with
text, 180 KB editor. **Not verified:** how it looks (no example uses it); the editor's
new key shape and the key inspector (not opened).

---

## 2026-11-16 08:30:00 - Session 2 (continued): joysticks

(The browser test fix below went to `main` as 40a6d75, CI green on the three systems.)

**Done.** `Joystick` (type 29): `handleX` and `handleY`, from -1 to 1, each scrub an
animation (`xAnimation`, `yAnimation`; keys 125 to 128). -1 is the animation's first
frame, 1 its last. The handle is keyed by the animation that plays, or bound to the
view model. `examples/joystick.json` (a face that looks where the handle goes; four
frames rendered and looked at). Rive's page on joysticks, read on 2026-11-16, does not
say how the handle maps onto the timeline, so that mapping is this project's choice.

**How.** The file builder lists (node, handle property, animation) per axis.
`Instance::advance` poses them after the player and the binds, when anything was set
since the last advance, and runs the binds once more for Properties that read what
was posed. `schema::default`'s table was a fixed 128 entries and key 128 walked off
its end: it is sized from the schema now.

**Code review: 10 findings; the first version was wrong in several ways.** The handle
was the position properties `x` / `y`, so the editor's arrow keys and drags would have
moved it by pixels and the reference described it as a position: it has properties of
its own. A joystick posed its animation even while that animation was being played by
name, so a pose animation could never be watched or edited on the timeline: one that
is played is left alone (`zuri render --animation lookAcross` at 0 s and 1 s now
differ). Autoplay could pick a pose animation as "the first animation", which then
fought the joystick every frame: autoplay passes over them. A handle that overflowed
into a zero-length animation asked the key sampler for a frame that is not a number
and it indexed out of range: the sampler takes that as the first key. A pose animation
that keys a handle chased itself: refused at load. A Property bound to a posed
property was a frame late: the binds run again after posing. The pass ran every frame:
only when something was set.
**Left as a documented ceiling (`ponytail:` in `advance`, and the format guide):** a
property that both the playing animation and a joystick animate is written twice a
frame and the scene never settles.

**Ponytail review: 3 findings, applied** (one flat entry per axis, the shared
animation lookup).

**Verified.** `bash scripts/verify.sh` exits 0. `features.rs:
joysticks_scrub_an_animation_on_each_axis`; seven breaks each failed it, one only after
I rewrote an assertion that could not tell the difference (the handle had been left
where both answers agreed). Sizes: 78.7 KB lite (limit 80), 132 KB with text, 179 KB
editor. No C ABI change. The engine gate was rerun from nothing on this
tree (its throwaway projects had been deleted): Unity EditMode 19 of 19, PlayMode 5 + 5
on Metal; Unreal 8 of 8 with the audio device. **Not verified:** a Joystick in the
editor (it lists its properties from the schema; not opened); a scene with a Joystick
in Unity or Unreal (the engine tests have none; autoplay is the same core call).

---

## 2026-11-16 07:00:00 - Session 2 (continued): a flaky sound test, and build outputs that vanished

(The editor test below went to `main` as 1bf36a6, CI green on the three systems.)

**The flake.** `packages/web/test/browser.mjs` failed once in three runs on the check
added with `Sounds` (96e6fef): the page waited ten turns of its clock for a sound to be
decoded and started. That clock is virtual (`--virtual-time-budget`): ten timer turns
are 16 ms of it and almost no real time, so real work on other threads loses the race
under load. The player's own sound checks and the editor test waited the same way
(never seen to fail).
**The fix, and a claim of mine that measuring corrected.** The first fix waited with a
`fetch` per turn, on my belief that the virtual clock stands still while one is
pending. Measured: each fetch moves it by 10 ms, whatever the server takes (20 ms,
100 ms and an immediate answer all gave 100 ms for ten). So both test servers now
answer `/wait` after 100 ms, and `settle` waits on a condition with it: up to 5 s of
real time, then it throws naming what it waited for. A sound gets 3 s before the
player's 300 ms lateness rule (by the page's clock) applies. Every sound wait in the
web and editor tests goes through it, the player's taps included.
**Code review: 8 findings**, all on the first fix. Fixed as above: the `ended`
counter counted every sound (now only the stopped one); the waits still spent the
page's clock; the player's sound checks were left as they were; the bound shrank on a
fast machine and gave up silently; a lone unconditioned fetch; the whole page fetched
per turn. **Refused:** the React test's `until` (2000 timer turns). It waits for work
that the page itself and the network do (which the virtual clock does wait for), has
no lateness rule to trip, and has not failed; it would need the endpoint in a third
server. Noted in `memory/todo.md`.
**Ponytail review: 1 finding, applied** (count `ended` in the `stop` hook).
**Verified.** `bash scripts/verify.sh` exits 0. In the final form the three browser
tests were run 59 times on the Mac (8 one at a time, the rest three at once to load
the machine): 58 passed. **One editor run failed and I do not know why**: it was under
that load, I had kept only its first line ("the page ran to its end", so the page
threw), and 30 more runs with the whole output kept did not bring it back. The checker
prints what the page saw when a check fails, so the next one will say. Two mutations of
`Sounds` fail the web test.

**A wrong belief, tested and dropped.** The review of the editor test read the long
runs (about a minute instead of 18 s) as Chrome lingering after its dump. Stopping
Chrome at the dump changed nothing, so that is not it; the change is out again and
`memory/todo.md` says what was tried.

**Incident, not caused here.** At 05:23 every `node_modules`, `target`, the packages'
`dist` folders and this session's scratch folder under `/private/tmp` disappeared, in
this repository and in the projects beside it, while a test loop was running. Nothing
this session ran removes them, and no tracked file changed (`git status` showed only
the three files being edited). It looks like a clean-up of build outputs across the
machine (the disk was 90 % full); another long-running session exists on it. The
dependencies were reinstalled from the lockfile and everything rebuilt (same wasm
sizes). `target/engines` (the engine gate's throwaway projects) is gone too: the next
`verify-engines.sh` starts from nothing and takes longer.

---

## 2026-11-16 06:20:00 - Session 2 (continued): the editor has a browser test

(The preview-sound unit below went to `main` as 96e6fef, CI green on the three systems.
Its refused finding, a test of the preview path, is what this unit builds.)

**Done.** `apps/editor/test/`: `browser.mjs` serves the editor with Vite and loads
`index.html` in headless Chrome through the web package's `chrome.mjs`; `drive.ts`
starts the editor by importing its own `main.tsx`, then presses its buttons, sends
pointer events to its stage, and reads the document from the store and the picture
from the canvas. Ten checks (listed in `apps/editor/README.md`, Status). It runs in
`scripts/verify.sh` after the editor's build, so in CI on the three systems.

**What had to give.** Headless Chrome with `--dump-dom` has no display to drive
`requestAnimationFrame`, so the page produced nothing at first: `drive.ts` replaces it
and calls the editor's frames itself, a 60th of a second apart. Pointer events made in
a page cannot be captured, so `setPointerCapture` is a no-op there. Both are test
seams in the page, not in the editor.

**Code review: 9 findings.** Fixed: the test opened the file through the store with a
name it typed itself, so the editor's own open path was not run (`openBytes` is
exported and called; breaking its rename now fails the test); a failed start, which
the editor reports in the page, surfaced as a null dereference (its message is passed
on; tried with a runtime that refuses to load); state was read after a fixed number of
turns although sizing the stage and starting a sound happen in the browser's own time
(`until` waits for the condition, with a bound); the stand-in clock advanced per
callback, not per frame; "nothing beside the artboard" accepted anything but 255; a
missing example gave a decompile error (the status and what to build, now); Vite
watched the whole editor folder, the desktop shell's build included (no watcher, no
hot reload); the sound recorder dropped `connect`'s further arguments; the pointer
capture stub is marked `ponytail:`. **Refused:** moving the sound recorder and
`frames` into a module shared with the web and React tests. There are two copies of
the recorder, not three, and the three `frames` differ (two tick players, this one
calls the editor's frames); the ponytail review looked at the same question and said
to wait for a third copy.
**Its side observation, not fixed here:** Chrome sometimes takes up to a minute to
exit after writing the page (in `memory/todo.md` now).

**Ponytail review: 6 cuts, all applied** (the page's wrapper, the console
pass-through, `cancelAnimationFrame` and its map, a hand-rolled `find`, `strictPort`,
a check that `press` already makes).

**Verified.** `bash scripts/verify.sh` exits 0 with the test in it. Breaks of the
editor that each failed it: the preview not playing the sound, the Layout button
making a Node, the stage sending `move` for a press, `openBytes` keeping the `.zur`
name, the runtime refusing to load. One more "mutation" proved nothing: the line I
added could never run, and stopping a sound at the end of a preview is not observable
with a 60 ms sound.
**Not covered by it:** everything in "Built, not verified" in the editor's README;
the File menu and file dialogs (the file is opened through the store); real pointer
capture; the desktop shell.

---

## 2026-11-16 05:45:00 - Session 2 (continued): the editor's preview plays sounds

(The Layout unit below went to `main` as 0a324b7, CI green on the three systems.)

**Done.** The web package's sound code, two private methods of `ZuriPlayer`, is now the
exported class `Sounds`: `new Sounds(file)` decodes in the background, `play(id, volume)`
plays one, `stop()` cuts off what is playing. The player uses it, and so does the
editor's state machine preview (one per compiled file, made when it is first
previewed, stopped when the preview ends or the document is compiled again).

**Code review: 9 findings.** The first version only moved the code. Fixed in `Sounds`,
so the player gains too: the audio output was opened only by a touch that came after
the first `Sounds` existed, so the click on Preview did not count and a Cue's sound was
silent until the next touch (`navigator.userActivation.hasBeenActive` now opens it; the
listeners stay, for browsers without it and to wake a sleeping output); a sound asked
for while still decoding was dropped (it now plays once decoded, under the same
300 ms lateness rule); nothing could stop a long sound (`stop()`); the editor decoded
again at every preview start and kept the buffers of a file long gone (per compiled
file now). Also the todo wording and this entry.
**Refused:** an automated test of the preview path. The editor has no browser test
harness (`memory/todo.md` has the item now); what can break is in `Sounds`, which has
its test, and the preview was checked by hand as below.

**Ponytail review: nothing to cut.**

**Verified.** `bash scripts/verify.sh` exits 0. `test/browser.mjs`: `Sounds` on its own
plays a sound asked for while it decodes, none for an unknown id, and `stop` cuts it off
once; three mutations each failed it. In Chromium against the dev server: the sound
example's pad starts its sound once per click; a scene with a Cue on a looping
animation started 5 sounds in 2.6 s after the click on Preview and nothing else, none
after Stop; with the `userActivation` line removed the same scene started none until
the stage was clicked. **Not verified:** hearing any of it; Safari and Firefox (Safari
may still want a touch after the output is made: the listeners cover that, one sound
late); `stop()` in the editor with a sound long enough to be cut (the test covers the
class).

---

## 2026-11-16 05:00:00 - Session 2 (continued): Layout wraps, and a padding per side

(The engine sound unit below went to `main` as e35eeea, CI green on the three systems.)

**Done.** Five Layout properties (keys 120 to 124): `paddingLeft` / `Top` / `Right` /
`Bottom`, each added to `padding` on its side, and `wrap`: a child that does not fit
along the flow starts a further line, a `gap` away. A line is as large across as its
largest child, the lines start at the top, and `justify`, `alignItems` and `grow` work
per line. The room is the Layout's own size along the flow or the one a parent's
`grow` / `stretch` gives it. Clipping to the box needed no code: a Clip that names the
Layout does it, and that now has a test and a line in the format guide.
`examples/chips.json` (rendered at both ends of its animation and looked at).

**How.** `lay_out` runs its two passes (sizes inside-out, places outside-in) a second
time when a wrapping Layout came out of placing with another size along its flow than
it was sized with: `given` holds that size, and the second sizing breaks its lines
there. Scenes without such a Layout run once, as before. Ceiling (`ponytail:` note): two
rounds, so a Layout that is sized by how such a child wrapped and in turn sets that
child's size is one round behind.

**Code review: 7 findings.** The first version broke lines only at the Layout's own
width, so the usual case (a row of tags in a column that stretches it) never wrapped,
and a grown one kept its old lines: fixed as above, tested both ways. One line got the
whole box across while several got their own height, so children jumped when a second
line appeared: a wrapping Layout's lines are now always as tall as what is on them. The
side paddings had a default of -1 meaning "take `padding`", which an animation or a
scrub in the inspector crosses: they are now added to `padding`, default 0. "This line
has a child" was tested by its length, which a negative gap or a child without size
makes zero: now by count. A dead `max`, the duplicated `boxes` helper in the tests, the
missing handover entry. **One claim was wrong:** that four 30-wide children with a gap
of -30 in 40 of room should make four lines; they take 30 together and stay on one.

**Ponytail review: 3 findings, all applied** (one loop instead of two in the sizing
pass; the dead per-line maximum; the example cut from ten chips to six).

**Verified.** `bash scripts/verify.sh` exits 0. `features.rs:
layouts_wrap_onto_lines_and_pad_each_side`; twenty deliberate breaks of the layout code
(twelve on the first version, eight on the rework) each failed it. Sizes: 78.2 KB lite
(limit 80), 131 KB with text, 179 KB editor. No C ABI change, so the engine gate was
not rerun. **Not verified:** the new properties in the editor's inspector (it lists
them from the schema; not opened); `flat` in the sizing pass (all of a Layout on one
line) has no test that tells it from the last line's length, because every case found
where they differ has the size set by the parent afterwards.

---

## 2026-11-16 03:40:00 - Session 2 (continued): sound in Unity and Unreal

(The audio unit below went to `main` as d4ad142, CI green on the three systems.)

**Done.** Both engines play the sound of an event; this is the rule-3 item the audio
unit put off (the four C ABI functions now have their wrappers).
- Unity: P/Invokes; `ZuriFile.SoundCount` / `SoundId(index)` / `GetEncodedSound(id)`;
  `ZuriInstance.TryGetEventSound`; `ZuriWav` (PCM 8/16/24/32-bit, 32-bit float, the
  extensible header: Unity has no call that decodes audio bytes at run time);
  `ZuriAsset.GetSound(id)` (decoded once); `ZuriPlayer.Muted` and `AudioSource`, played
  with `PlayOneShot` before the event handlers run, in play mode only. The importer
  warns about a sound that is not uncompressed WAV.
- Unreal: importing a `.zur` makes a `USoundWave` of each sound with the engine's
  `USoundFactory` and keeps it inside the asset (`UZuriFile::Sounds`; a new object name
  on each reimport, since the old wave may be playing). An instance copies that map when
  it is made, `Advance` plays an event's sound with `PlaySound2D` in a game world and
  broadcasts `OnZuriSound(Sound, Volume)` before `OnZuriEvent`. `bMuted` on the
  instance; the widget's and the component's `bMuted` are what their instance starts with.
- `scripts/verify-engines.sh`: the Unreal editor runs with sound, the gate requires the
  line "Zuri sound test played on the audio device", and the throwaway project gets a
  `Config/DefaultEditorSettings.ini` that turns off crash-report and usage uploads.

**Incident.** A draft of `Zuri.Sound` crashed the editor (it used objects after
`UnloadPackages` had collected them). Unreal's crash reporter uploaded that report to
Epic, its default when unattended: a minidump and the log, with this machine's paths in
it. It cannot be recalled; the owner was told. With the opt-out above, the next run hit
an engine ensure and no report was made.

**Code review: 10 findings.** Fixed: `Advance` read `File->Sounds`, so it could
dereference a file the editor had nulled, and after a reimport it looked old ids up in
the new file's sounds (the instance now keeps its own map; tested); the component wrote
`bMuted` to its instance every tick, undoing the documented way to take over playback
(now once, at BeginPlay; tested); the Unity inspector restarted the player when Muted
or Audio Source changed (drawn outside the change check; not tested); an Unreal asset
with sounds but no Sound Waves was silent without a word (a warning when an instance is
made; tested); Unity said nothing at import about a sound it cannot play (importer
warning; not tested); the README's harness line; the index-to-id round trip in
`GetSound`. **Documented, not fixed:** a Unity player that outlives a reimport may play
the wrong sound until `Rebuild()` (it already shows the old picture until then; README
Limitations). **Refused:** making the Unreal gate pass without an audio device. The gate
is local (not CI), and a part that silently skips is how the GPU tests once went
unnoticed; it fails the same way without a GPU.

**Ponytail review: 3 findings, all applied** (widget `SetMuted` deleted; `GetEncodedSound`
takes the id; an exact 16-bit harness check in place of a peak scan).

**Verified.** `bash scripts/verify.sh` exits 0. `bash scripts/verify-engines.sh`: Unity
EditMode 19 of 19, PlayMode 5 + 5 on Metal (Gamma, Linear); Unreal 8 of 8 on Metal with
the audio device. `Zuri.Sound`: import, the report with volume 0.8, reimport, an older
instance keeping its wave, a file filled with `SetData`, save / unload / load from
`/Temp/`, then a `UZuriComponent` in a playing world whose wave the audio device starts,
and does not while muted. Mutations that made it fail: the mute check removed, a fixed
wave name on reimport, `File->Sounds` in `Advance`, `bMuted` written every tick, the
warning removed.
**Not verified:** hearing it; a cooked or packaged Unreal game; sound from a
`UZuriWidget`; that the UMG designer stays silent; formats other than WAV in Unreal;
the two Unity editor fixes above; Unity or Unreal on Windows and Linux.

---

## 2026-11-16 01:50:00 - Session 2 (continued): audio

(The React unit below went to `main` as 406585a, CI green on the three systems.)

**Done.** `AudioAsset` (encoded bytes) and an Event that names one as its `asset`, with
a `volume`: when the Event is reported, the host plays the sound. The runtime makes no
sound and has no audio code.
- Core: `File::event_sound`, `Instance::take_reports` (the Events emitted, as objects;
  `take_events` names them as before).
- C ABI (additions; `ZURI_ABI_VERSION` stays 1): `zuri_file_sound_count` / `_id` /
  `_bytes`, `zuri_instance_event_sound`. `smoke.c` and `abi.rs` cover them.
- Web: `ZuriFile.sounds`, `Instance.reports()`, and the player plays an event's sound.
  Sounds are decoded in the background at load (an `OfflineAudioContext`: no output is
  opened for it). One `AudioContext` for the page is opened at the person's first touch
  or key anywhere on it, shared by every player and never closed. `player.muted`.
- CLI: `zuri info` lists an artboard's events with their sounds (it listed no events).
- Editor: a Sound button in Assets, an icon, and an Event's row shows its sound and
  volume (looked at in Chromium with `examples/sound.json`). The editor's own preview
  does not play sounds.
- `examples/sound.json` with `examples/assets/click.wav` (60 ms, made by a script).

**Code review: 10 findings.** The first web version opened an `AudioContext` per
player at load, woke it on `pointerdown` on the canvas only, awaited decoding before
the first frame and closed the context on `destroy`. So: silent on phones (a touch
counts when the finger lifts), a Chrome warning and an audio device per player, a
slower first frame, and a sound cut off when its player was destroyed in its own
event handler. All four are gone with the shape described above. Also fixed: a NaN
volume in a hand-made file reached the player's gain and stopped its frame loop
(`event_sound` reads through the finite-checking `f`; tested); an Event without a
name was reported through the C ABI as "" where core drops it (filtered; a test that
fails without it); `muted` existed as an option nothing passed and a field (the field
stays); `zuri info`'s documented output was stale; `reports()` and `sounds` were not
in the web docs.
**Refused for this unit, on purpose:** AGENTS.md rule 3 says a C ABI addition goes
into every wrapper. The Unity and Unreal wrappers do not have these four functions
yet. They get them with playback, which is the next unit: exposing them now without
anything that uses them would be API to reshape a day later.

**Ponytail review: 8 findings, all applied.**

**Verified.** `bash scripts/verify.sh` exits 0. Headless Chrome: the pad's sound starts
once per tap at volume 0.8 with its 60 ms buffer, not while muted, and not before
anything on the page was touched (nor in a burst afterwards). Engine gate on this tree
(before the last C ABI change, which only filters unnamed events; rerun noted in the
next entry if it was): Unity EditMode 19/19, PlayMode 4+4 on Metal; Unreal 7/7.
**Not verified:** hearing it (the checks watch the Web Audio calls); a phone; Safari and
Firefox; the 300 ms "still waking" rule (headless Chrome runs with audio allowed).

---

## 2026-11-16 01:25:00 - Session 2 (continued): the React bindings run in a browser, and three bugs in them

(The editor buttons below went to `main` as a305511, CI green on the three systems.
The editor packaging run 43764260398 at that commit is green too: see `memory/todo.md`.)

**Done.** `packages/react/test/`: Vite serves a small StrictMode app that uses the
built package; headless Chrome loads it; `browser.mjs` checks 11 things, counting the
players made and alive at each step. It is in `verify.sh`, and the build type-checks
the test app. The headless-Chrome launcher is shared with the web test
(`packages/web/test/chrome.mjs`); a failed check prints what Chrome said, as before.

**Three bugs in `useZuri`, found by the code review of the test and fixed:**
- For the render in which `src` (or another option the player is made from) changed,
  the hook still handed out the player it had just destroyed; an effect using it
  threw. It hands out a player only for the options it was made from.
- A `src` given as `new URL(...)` inline was a new object every render: the player was
  destroyed and reloaded for ever (the reviewer counted 235 fetches). A URL is compared
  by its address.
- A canvas that was not in the first render never got a player. `canvasRef` is a
  callback ref now (same use: `<canvas ref={canvasRef} />`).
A side effect of the last: StrictMode's double mount no longer makes and destroys an
extra player.

**The first version of the test was weak** (the review broke the hook seven ways and
five passed). It now fails for each of six breakages, tried one by one: no destroy on
cleanup, a player overtaken while loading not destroyed, `fit` not remaking the
player, the stale player handed out, a URL compared by identity, the canvas read only
at mount. One of mine was still not caught on the first try: under Chrome's virtual
time a load finishes before the next timer, so "overtaken while loading" never
happened until the two changes were committed with `flushSync`.

**Also from the review:** a missing file was answered by Vite with the page itself
(now a real 404, and the message is checked); the test ran the source, not the built
package (it imports `dist` now); `package.json` was reformatted by `pnpm add` (restored).
**Ponytail review: 12 findings**, applied, except that `@types/react-dom` stays: the
test app is type-checked now.

**Verified.** `bash scripts/verify.sh` exits 0. Not tried: Next.js, server rendering,
React 18 (the peer range says >=18; the tests run 19).

---

## 2026-11-16 00:50:00 - Session 2 (continued): the editor can add a group, a Layout, a Bone

(Layout below went to `main` as 18dbdf9, CI green on the three systems.)

**Found** while trying Layout in the browser: the editor had no way to make a Node, a
Bone or a Layout. The inspector's Add menu leaves out the types that live in the
hierarchy, and only the drawing tools made any of those.

**Done.** Three buttons above the hierarchy tree: Group, Layout, Bone. The new object
goes into the nearest container round the one selected object (itself, or the Layout
round a selected Fill; not into a Shape, which holds geometry), else into the artboard
(`doc/ops.ts: addTarget`, unit-tested), with a name and sensible values (`makeChild`),
and is selected and scrolled into view.

**Code review: 10 findings.** The first version was a popup menu inside the tree. Its
keys reached the global shortcuts (Escape cleared the selection, arrows nudged the
selected object), it sat inside `role="tree"`, it was clipped in a narrow panel, and
focus was lost after choosing: it is three buttons now, as in the Assets tab next to
it, outside the tree. Also fixed: a group landed inside a selected Shape and a Layout
at the artboard root from the same selection (one rule, above); nothing tested where
the object goes; two notes claimed more than was tried. Fixed along the way, for
every menu and button in the editor: an open menu owns the keyboard, and Space
presses the button that has the focus (the pan shortcut used to swallow it).
**A bug of my own, found in the browser:** an effect returned what `scrollIntoView`
returns, which in this Chrome is a Promise; React refused it and the panel did not
render. Type checks and unit tests had passed.

**Ponytail review: 6 findings**, all applied or made moot by the rewrite.

**Verified** in Chromium against the dev server: Layout, then Group (inside it), then
Bone (inside the group), each selected; a Layout's properties and a Fill from the
inspector; the stage drawing nested Layouts with the selection box in the right
place; Escape on an open toolbar menu closes it and keeps the selection; no console
errors or warnings. `bash scripts/verify.sh` exits 0 (editor: 25 tests).

---

## 2026-11-16 00:20:00 - Session 2 (continued): Layout

(Text on a path below went to `main` as dcc6340, CI green on the three systems.)

**Done.** `Layout`: a box that arranges its drawn children in a row or a column
(`flow`), `gap` apart, `padding` in from the edge; `justify` along the flow and
`alignItems` across it; a size that fits what it holds when it has none of its own;
`grow` and `stretch` for Layouts inside Layouts; Fill / Stroke children paint the box
(`radius`); a child's `x` / `y` nudge it from its place. `instance.rs: lay_out` sizes
from the inside out, then places from the outside in; the result is `slots` (added to
x / y in `placed`) and `sizes`. `examples/card.json`: a card that fits its content,
with a button that grows with its bound label (rendered and looked at, with two
labels). The editor has an icon and a default for a new Layout.

**Decisions.**
- By hand, not taffy (size budget): recorded in `memory/architecture.md`.
- A Text takes the room of its lines, not of its letters' ink (`text.rs: layout` also
  returns the line box): a button does not change height between "Open" and "page".
- A Text set along a path takes no room in a Layout (where it is does not depend on
  where it is put; measuring it fed back into the layout).
- `stretch` overrides a child's own size across the flow, and nothing shrinks: two
  places where this is not CSS. Said in the docs.

**Code review: 10 findings.** Fixed: a scaled or turned Layout inside a Layout was
measured turned but placed unturned, so its neighbour landed on it (it is measured the
same way both times; a scaled one grows in the parent's measure, a turned one keeps
its size); children hidden by `solo` took room; the followed-Text feedback (above);
the skin bind pose was taken before Layouts had placed anything, so a skinned Path in
a Layout was bent at rest; text measured by ink (above); sizes could go below zero
with negative padding; a changed Text was laid out twice a frame (once to measure,
once to draw: `lay_text` now keeps the outlines); the editor's resize handles took a
Layout for a box centered on its position (`stage/handles.ts`); duplicated helpers.
**Refuted by trying:** "a nested artboard that changes size is not given new room".
An Artboard's width and height can be neither keyed nor bound today (the compiler
refuses both), so it cannot change size; the propagation I wrote for it was removed.
**Not fixed, noted in the code:** every Layout is arranged again on any change in the
artboard, flattening its children's outlines to measure them; a skinned Path is
measured as it was last drawn.

**Ponytail review** measured smaller forms in a scratch copy and handed over a tested
patch (boxes from outlines through matrices, plain loops): applied as it stood, plus
its two doc cuts. Lite build: 78.3 -> 76.9 KB brotli with the patch, 77.1 KB with the
review fixes on top (it was 75.5 KB before Layout).

**Verified.** `bash scripts/verify.sh` exits 0; sizes in `memory/test.md`. Not driven
in the editor UI (the type appears there through the schema; nobody added a Layout
by hand). No engine run: no C ABI change.

---

## 2026-11-15 23:50:00 - Session 2 (continued): text on a path

(The font picker below went to `main` as 4b6f96e, CI green on the three systems,
shell test included. The engine gate was run on the core as of that commit:
Unity EditMode 19/19, PlayMode 4+4 on Metal in Gamma and Linear; Unreal 7/7 on Metal.)

**Done.** `"follow": <a Shape>` on a Text sets it along that Shape's outline: baseline
on the line, each letter turned to it. `position` (0..1) is where the text starts, or
with `align` where it is centered or ends, and can be animated. `text.rs: layout` takes
an optional function from a distance along the line to a point and a direction; a
glyph stands on the chord from where it starts to where it ends. `examples/badge.json`:
three Runs round a circle, turning (rendered and looked at).

**Decisions.**
- The outline is the Shape's, flattened finely (tolerance 0.01, not the 0.25 drawing
  uses) so that letters turn steadily: with the coarse outline a letter stalled and
  jumped as `position` moved (measured: steps of 0 and 0.4 against 0.1 to 0.2).
- A Text set along an outline is laid out again only when what it stands on changed
  (a fingerprint of the matrices involved, and geometry without a cached outline):
  `instance.rs: refresh_bent`.
- A second line runs beside the first at the same places along the path. Not made
  to respace: documented.

**Code review: 9 findings.** Fixed: a file whose Text follows itself (the compiler
refuses it, a hand-made file can say it) recursed until the stack overflowed (only a
Shape is followed; `runtime.rs: a_text_that_follows_itself_is_just_a_text` aborts
without the check); an outline of no length, or a Text scaled to nothing, drew the
text flat at its own origin (it draws nothing); letters took the direction of the
flattened piece under their middle and so turned in jumps (chord and fine flattening,
above); a mirrored Shape or Text put the text on the other side of the line, upside
down (the outline is walked backwards when seen in a mirror); every change anywhere
in the artboard laid such a Text out again (the fingerprint); the outline was measured
by a second copy of `point_along`'s loop (`geom::length`, `geom::point_at`,
`point_along` built on them); a large `position` lost the letters' spacing to rounding
(it wraps before it is multiplied). Documented, not changed: lines after the first.
Two of my own new assertions did not fail when their rule was removed and were
rewritten until they did.

**Ponytail review: 9 findings, all applied** (the glyph builder is a `Mat`, not four
fields; a let-else; two test blocks that re-checked generic behaviour removed; dead
properties in the example; shorter docs).

**Verified.** `bash scripts/verify.sh` exits 0; sizes in `memory/test.md`. No engine
run after this unit: no C ABI change.

---

## 2026-11-15 23:25:00 - Session 2 (continued): fonts in the editor (owner request)

(Kerning below went to `main` as e2dc852, CI green on the three systems.)

**Asked for** (owner, mid-session): "for fonts collect all the best open source fonts
that can be used commercially, creative commons, apache etc and also check fonts that
are in the machine the software installed and reflects them."

**Done.** **Font** in the Assets tab opens a font picker (`panels/FontPicker.tsx`, a
native `<dialog>`):
- *Open fonts*: 66 families (sans, serif, mono, display, script), listed in
  `apps/editor/src/fonts/catalog.json`. The file is written by `scripts/font-catalog.mjs`
  from `github.com/google/fonts` at a pinned commit: each family's licence, category and
  files come from its own `METADATA.pb`, and a licence outside OFL / Apache 2.0 / UFL
  stops the script. 62 are OFL 1.1, 3 Apache 2.0 (Roboto Slab, Permanent Marker,
  Satisfy), 1 Ubuntu Font Licence. That repository has no Creative Commons fonts, so
  there are none: said in the README, not papered over.
- *Installed* (desktop): the shell scans the system font folders (macOS, Windows, Linux)
  reading only each file's `name` table, and `read_font` hands over one font, made a
  file of its own when it sits in a collection (`zuri_import::font::single_face`).
- Choosing a font fetches it, shows it in its own letters with its licence, and "Add to
  document" embeds it as a FontAsset.

**Decisions.**
- Fonts are downloaded on first use, not bundled: they are 60 KB to 4.5 MB each (the
  66 would be tens of MB in the installer and the repository). A download is checked
  against the git hash in the catalogue. Cost: choosing an open font needs a
  connection, and GitHub sees the request. The desktop app's content security policy
  allows `raw.githubusercontent.com/google/fonts/` and nothing else new.
- `read_font` serves only what `system_fonts` listed (the path comes from the webview).

**Code review: 10 findings.** Fixed: the editor's shortcuts still acted on the document
behind the modal (Backspace deleted the selection; a modal dialog now owns the keyboard:
checked in the browser); a crafted `.ttc` could make `single_face` copy the file once per
table record, without bound, and overflow the 16-bit header arithmetic (tables must fit
in the file, at most 256; tested); the licence line said only "commercial use and
embedding are allowed" (it now says what each licence asks when the font is passed on,
written after reading the three licence texts at the pinned commit; the README has a
Licences paragraph, including that a subset is a changed font); the identifier
`UFL-1.0` does not exist in SPDX (it is `Ubuntu-font-1.0`: checked against spdx.org);
focus went to the wrong element on open and was not given back on close; the security
policy let the page reach every repository on the host (now the one path) and allowed
font URLs it does not need; the shell's test passed with no fonts found and failed on
one odd font (non-empty on macOS and Windows, nine in ten must read back); a click
that ended on the backdrop closed the picker (it no longer closes on the backdrop); a
dot in a font's name was cut off as a file extension; the trail in `memory/`.
Also: a download is dropped when another font is chosen.

**Ponytail review: 9 findings, all applied** (no editable sample text, no date line and
its API call, one preview name, no memo, the loading functions moved into
`io/assets.ts`, the commit pinned in one place, a shorter README).

**Verified.** `bash scripts/verify.sh` exits 0 (editor: 24 tests). In Chromium against
the dev server: see the README's "Verified" list. The shell's tests on this Mac: 652
installed fonts listed, read back as single fonts.
**Not verified.** The Installed tab in the desktop window (the page's calls to the
shell are by reading only), a catalogue download under the packaged app's content
security policy, installed fonts on Windows and Linux beyond the shell test in CI.

---

## 2026-11-15 23:05:00 - Session 2 (continued): kerning

(`zuri mcp` below went to `main` as e42475c after one more CI round: the first run
failed on Windows only, on an "unused mut" lint where a Unix-only line is compiled out.)

**Done.** Text is kerned with the font's own pairs: `text.rs: kerning` reads the pair
adjustments of the GPOS `kern` feature, else the `kern` table, and `layout` moves a
glyph by its kerning against the glyph before it (same font and size, also across
Runs; not the first glyph of a line). The build-time subsetter drops GPOS, so it writes
the kerning among the kept glyphs as a `kern` table, or keeps the font's own GPOS when
that is smaller. A subset font draws pixel-for-pixel like the whole one, kerning included.

**The golden `text_settled.png` changed** because the letters of the example are kerned
now (looked at before and after on "AVATAR Toy Wave": AV, To, Wa close up).

**Decisions.**
- GPOS is read through ttf-parser (feature `opentype-layout`), not by hand: 8 KB brotli
  in the text build (117.6 -> 126.3 KB; the lite build has no text and is unchanged), for
  a parser that is already tested. Recorded as a size lever in the todo.
- Kerning is always on; there is no property for it.
- One script: the `kern` feature of the default language of `latn`, else `DFLT`, else
  the font's first script.

**Code review: 10 findings** (the reviewer compared the written `kern` table against an
independent fontTools reading of Outfit's GPOS: no mismatch over 241 x 241 glyphs).
Fixed: a hostile font could make the lookup allocate and loop without bound (64 lookups
of 256 subtables at most, indices taken lazily); listing most of a font's characters made
the "subset" bigger than the font (the font's own GPOS is kept when smaller than the
flat pairs, and a subset that is not smaller is not used); kerning sat on the left
glyph's advance, so a line ending before a dropped space was measured short and poked
out of a right-aligned box (it is on the right glyph now and skipped at line starts);
kerning across Runs of different sizes pushed a large letter over a small one (same size
only); every `kern` feature of every script was summed (one script now); a subtable that
cannot be read hid the ones after it; the multi-subtable `kern` writer had no test
(`font.rs` unit test reads three subtables back with ttf-parser); stale and truncated
lines in the todo.
**Refuted, with the source:** "a class subtable whose right glyph is in class 0 should
fall through to the next subtable". HarfBuzz's `PairPosFormat2.hh` (main, read
2026-11-15) applies the subtable whenever the left glyph is covered and both classes
are in range, class 0 included; this code does the same.
**Not fixed:** "minimum" and "override" subtables of a legacy `kern` table are read as
plain kerning: ttf-parser does not expose those bits (a `ponytail:` note says so).
One of my own new tests did not fail when its rule was removed; it was rewritten until
it did.

**Ponytail review: 7 findings, all applied.**

**Verified.** `bash scripts/verify.sh` exits 0 (on the second run: the first stopped at
the headless-Chrome step with no output from Chrome, and that step passed when run alone
and in the rerun; seen once, cause not found). Sizes: lite 75.1, with text 126.3,
editor build 172.8 KB brotli; `examples/text.zur` 30.4 KB. No engine run: no C ABI
change (the engines link the same core, so their text is kerned too; not looked at).

---

## 2026-11-15 22:25:00 - Session 2 (continued): `zuri mcp`

**Done.** `zuri mcp` serves the commands as Model Context Protocol tools on stdin /
stdout: `schema`, `validate`, `info`, `render`, `build`, `dump`, `import`
(`crates/zuri-cli/src/mcp.rs`). A tool call becomes the command's `Args` and goes
through `run`, so there is no per-tool code beyond the table of tools. `validate`, `info`
and `render` also take the scene inline (`source`); `render` returns the PNG as image
content, one frame a call.

**Protocol, as read on 2026-11-15** (modelcontextprotocol.io/specification): the current
revision is 2026-07-28, newer than what the model knew. It has no `initialize`: every
request names its version in `params._meta["io.modelcontextprotocol/protocolVersion"]`
(and `clientCapabilities`), servers implement `server/discover`, every result has
`resultType`, `tools/list` results have `ttlMs` and `cacheScope`, an unsupported version
is error -32022, `ping` is gone, and a call with bad arguments is a result with
`isError`, not a protocol error. The server does that, and still answers `initialize`
(2025-11-25 back to 2024-11-05) with `ping` and batches, for clients of older revisions.
Not read: the 2026-07-28 schema.ts (field-level required flags).

**Code review: 10 findings** (the reviewer read the spec pages and piped messages into
the binary). Fixed: arguments were not checked against the tool's schema (a misspelt
`stateMachine` drew the default and reported success; `help: true` returned the help
text; now an argument the tool does not have is named in an `isError` result, and
`required` is in the schemas); temporary files leaked on early returns and had
predictable names in the shared temp folder (each call now has a folder of its own,
made to fail if it exists, mode 0700, removed when the call ends however it ends: the
test gives the server an empty temp folder and checks it is empty afterwards); `null`
arguments became the text "null"; a line that is not UTF-8 ended the server silently
(lines are read as bytes and refused with -32700); non-object messages got no reply
(-32600; arrays are answered as batches, which 2025-03-26 requires); `frames` and `time`
were unbounded in a server that cannot be cancelled (`frames` and `fps` are not offered,
`time` is capped at an hour); the -32022 error advertised a version it then refused; a
parse error carried `"id": null`; the docs said `info` lists events and that `source`
never touches the disk.
Also: `zuri mcp --help` prints the help and extra arguments are an error; a panic inside
a command fails that call, not the server.

**Ponytail review: 12 findings, 11 applied** (errors as JSON values, the `new` tool and
`frames` / `fps` dropped, the docs' tool table replaced by four lines). Kept: refusing a
request without `clientCapabilities`, which the revision says a server must.

**Verified.** Four integration tests spawn the binary (`crates/zuri-cli/tests/mcp.rs`):
both protocol eras, the failing calls, the leftover-file check, `--help`. By hand, before
and after the review fixes: the official MCP Inspector (`npx @modelcontextprotocol/inspector
--cli`, 2.13.0) listed the tools, got a validate error and a misspelt argument as
`isError` text, and renders of `examples/tail.json` and `examples/text.json` as PNGs. `claude mcp add` syntax checked against its `--help`.
**Not verified:** use from a chat client (Claude Desktop) or from Claude Code itself
(adding a server changes the user's configuration: not done without asking); Windows
only through CI.

---

## 2026-11-15 21:40:00 - Session 2 (continued): text Runs

**Done.** A `Run` inside a Text is a stretch of it in its own style: `text`, `size`,
`weight`, optionally a font (`asset`; the Text's otherwise) and its own Fill / Stroke.
A Text with Runs sets them one after another as one text: lines wrap between and inside
Runs, a line is as tall as the largest size on it, the Text's paints paint the Runs that
have none, and a Run's own paints go on top. `text.rs: layout` takes spans and returns
one outline per span; a Run is a scene node (its text can be bound, its size animated,
it has bounds).

**The golden `text_settled.png` changed** because `examples/text.json` now has a bold,
coloured Run in its body line (looked at: the sentence reads as before, "at run time"
is bold and green). Before the example was touched, the old golden passed with the new
layout: a Text without Runs is set as it was.

**Decisions.**
- A Run takes its font from the Text when it names none, but not `size` or `weight`
  (16 and 400 unless set). A number left out is the schema default everywhere in the
  runtime (animation, Binds, `direct` states); reading it from the parent in one place
  is the mistake the Constraint limits review found one unit earlier.
- No style objects shared between Runs (Rive's text styles): repeat `size` / `weight`.

**Code review: 10 findings.** Fixed: lines of a long one-size text drifted by an f32
step from about the fifth line (line tops were added up; they are now the exact sum of
sizes times the line height, and a test pins line 12 to the bit: it fails on the old
arithmetic); a Text with k Runs was laid out k + 1 times, parsing every font each time
(2000 short Runs: 11.6 s by the reviewer's measure, 0.02 s now: `draw` lays a Text out
once for itself and its Runs); Runs with their own paints were painted under the Text's
(a wide outline ran over them); the box given to the Text's paints was that of the
unstyled Runs only; a Run without `asset` drew nothing (it uses the Text's font, and the
font subsetter keeps its characters: `import.rs`); `bounds()` did not know a Run; the
editor's default Bind on a Run bound `opacity` (now `text`, with a test); stale comments
and a stale "Limits today" line in `format.md`; the Text / Runs lookup was written three
times (`Artboard::runs`). The trail (this entry, todo) was the tenth.
**Not tested:** the box for a ShaderPaint on a Text with styled Runs (fixed by reading;
no test draws a shader on text).

**Ponytail review: 3 findings, all applied** (the line breaker has no separate
no-wrap path; a dead fallback and a stale comment removed).

**Size.** Lite 75.1 KB brotli (it has no text: the Run painting and layout pre-pass are
compiled only with the `text` feature); with text 117.6 KB; editor build 164.6 KB.

**Not verified.** Runs in the editor UI (adding one, editing its text) and in the
engines; in a browser only through the example the browser check already loads.

**Verified.** `bash scripts/verify.sh` exits 0. No engine run: no C ABI change.

---

## 2026-11-15 21:10:00 - Session 2 (continued): Constraint axis and limits

**Done.** `axis` (`both`, `x`, `y`) on `translation` and `scale` Constraints: take one
axis from the target, keep the other. `min` / `max` on `translation`, `rotation` and
`scale`: what is taken from the target stays within them (`instance.rs: constrained`).

**Code review: 9 findings.** The first version read an omitted `min` / `max` as "no
limit" inside `constrained()` only, while animation, `direct` states and Binds read an
omitted number as the schema default 0: keying a limit would have started from 0. The
defaults are now real (-1e9 and 1e9), read through `f()` like every other number, and
the reference shows them. Also fixed: crossed limits flipped between the two bounds
(`max` wins now); a mirrored target (negative x scale) slipped under `max` and was
un-mirrored by `min` (the limit is on the size, the sign is kept); the scale test could
not fail for the y axis (the leader is now 2 x 3, both axes and both limits asserted).
**Not changed, with reasons:**
- A rotation is read from the target's matrix, between -180 and 180, so a limited
  follower jumps when its target turns through 180. Counting the target's turns is
  state the runtime does not keep; written in the docs and as a `ponytail:` note.
- `axis` / `min` / `max` on the other kinds of Constraint are ignored without an error,
  as `distance`, `orient`, `position` and `bones` are on the kinds that do not read
  them; each property's doc says which kinds read it.

**Ponytail review: 3 findings, all applied** (the limits live in the match arms, not in
a pass after them).

**Still missing (todo).** Limits relative to the parent (a `space` property), copy
factor and offset, dependency-sorted evaluation.

**Verified.** `bash scripts/verify.sh` exits 0; lite wasm 74.6 KB brotli. No engine run:
no C ABI or integration change.

---

## 2026-11-15 20:55:00 - Session 2 (continued): skinning

**Done.** A `Skin` inside a Path binds the path to a Bone: `target` and `weights`, one
number from 0 to 1 per point of the path (none = the bone holds every point alike). A
point goes where its bones take it from the bind pose, in proportion to their weights
(`instance.rs: skinned`, used by `geometry()`, so drawing, hit tests, bounds and
`followPath` all see the bent outline). `examples/tail.json`: a tapered tail on four
bones, swaying; rendered at three moments and looked at (it bends smoothly; the outline
is a polyline with a point every 20 units, which shows as slight facets at 2x).

**Decisions.**
- The bind pose is what the document's own values say (`x`, `y`, `rotation`, `length`),
  before Constraints, animation and Binds, so that a person or an agent can write a path
  over the bones from the JSON alone. (Rive stores bind matrices in the file; an explicit
  bind is the upgrade if an editor ever needs to rebind in another pose.)
- Weights are written out. No automatic weighting in the runtime: it would be code in
  the player for something a build step can do (todo).
- `Skin` is not a scene node: the loader turns it into `Artboard.skins`.

**Code review: 10 findings.** Fixed: the bent outline was dropped from the cache after
Constraints had read it, so `followPath` on a skinned Shape followed the last frame's
outline when frames were drawn in between (now dropped before; the test fails with it in
the old place); a Skin without weights lost points that a keyed `d` adds (empty weights
now mean "all"); a bone with no size in the bind pose bound the path through the
identity and moved it (it now holds nothing); negative and huge weights (0 to 1 now);
weights were not checked on a Skin without a target; `target`'s doc string did not
mention Skin; the docs' point-counting rule did not cover `Q`, `S`, `T`, `A`, `H`, `V`
(they say now, and advise `M L C Z` for a skinned path); the path was parsed once per
Skin and the points-per-element rule written twice (`El::points`); the bind pre-pass ran
for artboards without skins.
**Not changed:** a Skin without a `target` loads and does nothing, as a Constraint
without one does (an object being edited is not an error); its weights are checked.

**Ponytail review: 3 findings, all applied** (`numbers` reads through the path parser's
`skip` / `num`, a one-line point count, no special case for empty `weights`).

**Size.** Lite wasm 72.9 -> 74.5 KB brotli (`LITE_LIMIT` 80). First version cost 6.9 KB
raw; a closure inlined five times in an iterator chain was most of it. As loops: 4.8 KB.

**Not verified.** Adding or editing a Skin in the editor UI (the editor reads the schema,
so the type is offered under a Path; nobody clicked it). No engine run: no C ABI or
integration change.

**Verified.** `bash scripts/verify.sh` exits 0: 80 Rust tests, web and editor tests,
browser checks; the new example builds.

---

## 2026-11-15 20:35:00 - Session 2 (continued): size pass on the player

(Time read from `date`. The entries below dated 2026-11-16 carry estimated times: `git log`
dates the commits they describe 2026-11-15. Order is still newest first.)

**Done.** The lite wasm went from 93,972 to 72,851 bytes brotli (269,932 to 226,244 raw),
with text 122 to 114 KB, editor build 161 to 160 KB. `twiggy top` on a build that keeps
names showed two pieces of the standard library in the player that nothing needs:
- exact float parsing (`dec2flt`), from `str::parse::<f32>` in `parse_svg_path`. Replaced
  by `geom::decimal`: digits into a u64, scaled by a power of ten in f64.
- float formatting (`flt2dec`), from the panic message of `f32::clamp` at the five places
  where a bound is computed (with constant bounds the check is compiled away). Replaced by
  `geom::within`, which is `clamp` without the panic, NaN handling included.

**Code review: 8 findings.** Fixed: my first replacement for `clamp` (`.max().min()`)
turned NaN into the lower bound, where `clamp` keeps it (`within` keeps it); the doc
comment claimed more accuracy than `decimal` has (it now says what is true, as a
`ponytail:` note, and the test pins "within one step of std" on the reviewer's halfway
cases and compares bit patterns, so `-0` is checked); nothing failed when the win was
undone (`packages/web/scripts/build.mjs` now fails when the lite wasm is over
`LITE_LIMIT`, 80,000 bytes brotli; checked by lowering it); the exponent was capped
instead of saturating; two scanners for one grammar (see below).
**Not fixed, on purpose:** `decimal` rounds twice, so a number within about 1e-16 of
halfway between two f32s can be one step off std, and one that close below f32::MAX reads
as infinite, which path data refuses (std read it as f32::MAX). Exact rounding is what
the 15 KB were. Coordinates there are not drawable anyway.

**Ponytail review: 6 findings, all applied.** `decimal` now reads a number at the start
of its input and says how long it was, so `num` lost its own scanner (20 lines); `powi`
in place of a scaling loop; the test's cases on one line. One visible change comes with
the merge: `5e9.91` reads as two numbers (`5e9`, `.91`), as SVG specifies; it was an error.

**The budget.** Still 100 KB for the lite build. `LITE_LIMIT` is a ratchet under it: raise
it in the commit of the feature that needs the room.

**Verified.** `bash scripts/verify.sh` exits 0 (see the commit's CI run for the three
systems). No engine run: no C ABI or integration change.

---

## 2026-11-16 15:10:00 - Session 2 (continued): bones and IK

**Done.**
- `Bone`: a joint with a `length`; a Bone inside a Bone starts at its parent's far end.
  It carries Shapes, Images and Nodes like a Node (opacity, order, clips, listeners).
- Constraint `"constrain": "ik"` with `bones`: turns that many bones so the last one's far
  end reaches the target (`instance.rs: reach`). World placement goes through `placed()`,
  which adds the per-bone turn IK asks for.
- `examples/arm.json`: an arm following a target that rides an ellipse (followPath + ik).
  Rendered at three moments and looked at.

**The solver was replaced during review.** The first version iterated (cyclic coordinate
descent, 12 rounds). The code review showed it stalls when the target lies on the first
bone's line or at the elbow, spins a zero-length bone, and leaves longer chains short.
It is now exact: each bone in turn is solved as a triangle with the rest of the chain
taken as one rigid piece (law of cosines), bending to the side the animated pose bends
to. The test reaches all of the reviewer's failing targets and a four-bone chain within
0.3 units.

**Code review: 10 findings.** Also fixed: the IK pass discarded other Constraints already
applied between the chain's first and last node (they are applied again); a second `ik`
rescaled the first one's result on shared bones (each scales only its own turn); every
step re-placed every node in range (only the chain now); Bone had no opacity / order and
could not be clipped or listened to; `ik` on something that is not a Bone was silently
ignored (a load error now); the editor had no icon or inspector rule for Bone.
Not done: a golden image for the example (the feature test checks positions and pixels).

**Ponytail review: 8 findings, all applied** (the reviewer's version was checked against
a million samples of the triangle angle and taken as it stood): one turn per bone kept
beside it instead of a second list, the reach clamp folded into the cosine's, the chain
built by walking up. They took back 1.2 KB raw.

**Size.** The lite wasm is 94.0 KB brotli of the 100 KB budget. The next large features
(layout, skinning, audio, scripting) will not fit without a size pass or a decision to
move the budget; `memory/todo.md` lists the known levers.

**Not in this unit.** Skinning (path vertices weighted to bones).

**Verified.** `bash scripts/verify.sh` exits 0: 78 Rust tests, 8 Node tests, browser
checks, editor 20 tests + build (the new `examples/arm.json` builds); wasm brotli
94 / 122 / 161 KB. No engine run: no C ABI or integration change.

---

## 2026-11-16 12:30:00 - Session 2 (continued): editor installers, nine-slice images

**Editor installers on three systems.** `.github/workflows/editor.yml` runs `tauri build`
on macOS, Linux and Windows, starts the packaged program and checks it is still running
after 20 seconds, and keeps the installers as artifacts. It runs by hand, on a `v*` tag,
or when the branch `bundle` is pushed (not on every push: it is slower than the gate).
Run 43734090861 passed on all three in 6 minutes each: .dmg 3 MB, .msi + .exe 6 MB,
.deb + .rpm + AppImage 89 MB. Nothing is published or signed.
Known weakness of the check: a window that came up blank would also "stay up".

**Nine-slice images.** `sliceLeft` / `sliceTop` / `sliceRight` / `sliceBottom` on an
Image (`raster.rs: stretch`, `draw.rs`). The slices are in pixels of the picture as the
ImageAsset describes it, so a host that decodes the picture at another size draws the
same thing.

**Code review: 8 findings.** Fixed: a middle under one pixel wide panicked in the render
loop (`clamp` with min > max; slices are numbers and can be animated through such
values); a box smaller than its ends switched abruptly to a plain stretch (ends are now
shortened in proportion); slices were in pixels of whatever bitmap the host supplied;
the slice payload was a positional array in a third order (now one tuple per axis).
Written down, not changed: one slice pixel is one artboard unit, so a double-density
picture needs a scaled parent to be sliced at half size; `stretch` runs twice per pixel
(`ponytail:` note with the upgrade); the editor workflow's liveness check (above).

**Ponytail review.** The reviewer agent was cut off by the account's usage limit twice;
I did the pass myself meanwhile and found nothing to cut. It then completed: the runtime
code is lean (it measured six variants, none smaller), and one finding, a duplicated
scene in the test, was applied.

**Verified.** `bash scripts/verify.sh` exits 0: 77 Rust tests, 8 Node tests, browser
checks, editor 20 tests + build; wasm brotli 93 / 121 / 160 KB. No engine run: no C ABI
or integration change in this unit.

---

## 2026-11-16 09:00:00 - Session 2 (continued): keyboard Listeners

**Done.**
- Listener events `keyDown` / `keyUp` with an optional `key`, named as the web's
  `KeyboardEvent.key` names keys. Core `Instance::key(name, down) -> bool` (true when a
  Listener reacted); nested artboards hear keys unless a `solo` hides them.
- C ABI `zuri_instance_key` (an addition). Web: `Instance.key`, and the player forwards
  the keys of the focused canvas (or of `<zuri-player>` itself, whose canvas is in a shadow
  root). Unity: `ZuriInstance.Key`, `ZuriPlayer.Key`, `ZuriPlayer.KeyName(KeyCode)`. Unreal:
  `UZuriInstance::Key`, and `SZuriWidget` / `UZuriWidget` forward keys while focused
  (`IsFocusable`, default off, as `UButton`).
- `examples/button.json`: Enter presses the button (1,172 bytes now).

**Decisions.**
- Web player: a key the scene reacted to is the scene's (default prevented) while it is
  down; Tab always moves on; keys with Ctrl / Cmd / Alt are left to the browser; a held
  key is heard once; keys still down are released when the focus leaves.
- Unity does not forward keys by itself. The only API both input backends share (IMGUI
  `OnGUI`) gets no input when a project uses the Input System package alone (read in the
  package's own docs by the agent). The README gives the one line a game needs.
- Loading refuses a `key` on a Listener that is not a key Listener, an empty `key`, and a
  `target` on a key Listener. A key reported with no name is ignored.

**Reviews.** Code review 9 findings, ponytail 8: fixed or applied, except removing the
focus setup from the browser test's element case (kept minimal anyway) and a blur check
(no example scene has a keyUp Listener to observe it with).

**Not done.** Focus as a scene concept (which object has it, tab order), gamepad, text
input.

**Verified** on the committed tree. `bash scripts/verify.sh` exits 0: 76 Rust tests, 8
Node tests, browser checks (Enter on a canvas presses the button once; shortcuts and
other keys pass through; `<zuri-player>` takes keys), editor 20 tests + build; wasm
brotli 92 / 120 / 159 KB. `bash scripts/verify-engines.sh`: Unity EditMode 19 of 19,
PlayMode 4 + 4 on Metal; Unreal 7 of 7 on the Metal RHI (`Zuri.Keys` is new). CI run
43732896677: macOS, Linux, Windows green.
**Not verified.** A real key press in either engine or a browser window (tests call the
handlers or dispatch synthetic events); the Unity README's two forwarding one-liners
compile but were not run with a keyboard.

---

## 2026-11-16 05:40:00 - Session 2 (continued): font subsetting

**Done.** A FontAsset with `glyphs` is cut down when `zuri build` or the editor's export
writes the file (`crates/zuri-import/src/font.rs`, called from `zuri_import::compile`).
It keeps the glyphs for the texts set in that font, every string a bind could put into
a text, and the characters `glyphs` lists. Glyph numbers do not change: unused `glyf`
outlines and `gvar` data are emptied and the tables the runtime does not read are left
out. Variable fonts stay variable. `examples/text.zur`: 111 KB -> 24 KB.

**Why hand-written.** The example font is variable and its weight is animatable, so the
font must stay variable; keeping glyph numbers makes every other table valid as it is,
which is about 150 lines. It runs only in the build tool, never in the runtime wasm.

**Code review: 10 findings.** Fixed: a `.zur` that was dumped and rebuilt was subset
again and silently lost characters (the built file no longer carries `glyphs`); every
font kept the characters of all fonts' texts (now per font); the schema text promised
subsetting wherever a file is built (the browser's `Runtime.compile` does not: now said);
the composite-glyph walk was untested (an accented text is in the test; breaking the walk
on purpose fails it); two crates named `ttf-parser` separately (one workspace entry).
Collections: first rejected, then (ponytail) simply handled: the first font is subset.
Checked by hand with a system `.ttc` outside the repo: 2.1 MB -> 30 KB, identical pixels.
Written down as ceilings in `font.rs`: CFF fonts stay whole; tables the runtime does not
read are dropped (`post`, `kern`, GPOS...); spacing of characters that were not kept.

**Ponytail review: 5 findings, 3 applied.** Not applied: dropping the checksums (a font
without them is not a valid font for any other reader), and moving the run-time text
case into the test's scene template (the `assert!(set_string)` guards the replace).

**Verified.** `bash scripts/verify.sh` exits 0: 75 Rust tests (7 importer tests), 8 Node
tests, browser checks (they load the subset `text.zur`), editor 20 tests + build. No
engine run: no C ABI or integration change. CI result: see `memory/todo.md` if it differs.

---

## 2026-11-16 03:30:00 - Session 2 (continued): view-model depth

**Done.**
- `enum` Properties with `Option` children. A number (the Option's position) wherever
  numbers are used; a name when bound to a text or when the host uses the string calls.
  C ABI: `ZURI_ENUM = 5`, `zuri_instance_property_option_count` / `_name` (additions; no
  signature changed). Web `properties[].options`; Unity `ZuriProperty.Options` and an
  inspector popup; Unreal `FZuriProperty::Options`; CLI `--set mode=busy`.
- Bind `direction`: `toSource` (read an animated value from the view model) and `twoWay`.
- Number to text: `decimals` and `format` (`"Score: {}"`).
- Listener `"event": "change"` with `"with": <Property>`: runs when the Property changes.

**Size.** This unit first took the lite wasm from 88.4 to 95.6 KB brotli. Two standard
library calls were the cause: `str::replacen` (the string searcher) and slicing a `str`
(its panic formatting), about 10 KB. The `{}` fill now works on bytes, and `fixed()`
writes digits by hand. Final: 91.9 KB (budget 100). Per-function growth was measured by
the reviewer; the largest remaining parts are `apply_binds` and load-time validation.

**Code review: 10 findings, all fixed.** Formatting a huge number panicked; a Bind that
writes back could push an enum off its Options or write text into a number Property (now
a load error: the Property must be able to hold the value); Option names collided with
objects' names (Options have their own name class now); an enum's start value was not
checked (and may now be given by name); enums passed `into` a nested artboard must list
the same Options; a `change` Listener missed a trigger fired by a later Listener
(triggers fired by a `change` Listener now fire for the whole next frame); a text bind
allocated every frame (cached by the Property's value).

**Ponytail review: 12 findings, 9 applied.** Not applied: removing the text cache (the
project standard is no per-frame allocation), the Unity loop-variable copy (harmless),
folding the Unreal warning into the generic one (it would lose the Option's name).

**Also.** The session was interrupted by a usage limit while the two reviews ran; both
were resumed and completed.

**Verified** on the final tree. `bash scripts/verify.sh` exits 0: 74 Rust tests, 8 Node
tests, browser checks, editor 20 tests + build, Unity dotnet harness; wasm brotli
92 / 120 / 158 KB. `bash scripts/verify-engines.sh`: Unity EditMode 17 of 17, PlayMode 4
in gamma and 4 in linear on Metal; Unreal BuildPlugin and 6 of 6 automation tests on the
Metal RHI (`Zuri.Enum` is new). CI run 43719162543: macOS, Linux and Windows green.
**Not verified.** Choosing an Option in the Unity inspector popup (it draws; a menu cannot
be driven in batch mode); `Options` from an Unreal Blueprint; nothing of this unit in the
editor app's UI.

---

## 2026-11-16 00:40:00 - Session 2 (continued): Unity library on Linux and Windows

**Done.** `scripts/build-unity-plugins.sh` builds the native library for the system it
runs on (x86_64 on Linux and Windows, universal on macOS). All plugin `.meta` files now
live in `integrations/unity/PluginMeta~/` and are copied next to the library when it is
built; `Plugins/` holds only generated files (and a `.gitkeep`). CI builds the plugin on
all three systems. The root README has a table of what has run where.

**How the Linux / Windows metas were verified.** An agent imported placeholder libraries
in Unity 6000.4 on this Mac, set the importer through the `PluginImporter` API and read
it back (editor OS and x86_64, Standalone Linux64 / Win64 only). Unity wrote them in a
newer format (`serializedVersion: 3`); the stored ones use the same format as the macOS
meta (version 2), which Unity 6000.4 reads back to the same settings, because the package
declares Unity 2021.3 as its minimum and only 6000.4 is installed here to check.

**Reviews.** Code review 8 findings, ponytail 3. Fixed: the build used the host's
architecture but always filed the library under `x86_64`; the macOS metas would have been
orphans on other systems (one rule for all three now); the importer test threw on a
folder it did not know; the READMEs claimed CI results before CI had run them.
Not done: stripping local symbols on Linux and Windows; a test that sees the stored
Linux / Windows metas on every run (they are checked only when their library is present).

**Verified.** CI run 43710631608: macOS, Linux, Windows green including the new step.
`bash scripts/verify-engines.sh unity` on the Mac: EditMode 16 of 16, PlayMode 4 + 4.

---

## 2026-11-15 23:30:00 - Session 2 (continued): three platforms, engines on the GPU

**Owner's instruction (2026-11-15):** "the software should work in mac, windows and linux",
and the Xcode Metal Toolchain is now installed. Recorded in `memory/project_context.md`.

**CI (`.github/workflows/ci.yml`).** `scripts/verify.sh`, then the Tauri editor shell
(build + tests), then the Unreal static library, on macos-latest, ubuntu-latest and
windows-latest. Developed on the branch `ci` (a scratch branch, so that `main` only gets
reviewed work). What the first runs found, all fixed:
- Windows: the C smoke test needed `clang` and `-D_CRT_SECURE_NO_WARNINGS`.
- macOS runner: the "binary embeds the home directory" check fired falsely. Rust's own
  standard library is built on GitHub's macOS runners and carries `/Users/runner/work/rust`.
  The check now looks for this build's three source roots, plus `$HOME` everywhere except
  on that runner.
- Portability, found by review before the first run: the browser checks were bash +
  Python (now `packages/web/test/browser.mjs`: node only, real MIME types, a timeout,
  Chrome's stderr on failure); `tsc` was spawned through a shell script; `node --test
  test/` only works on Node 26; Windows paths in `cargo-ship.sh`; LF line endings.
- Linux passed everything on its first run.
The state of each system is in `memory/todo.md` (only what has run there counts).

**Engines on a real GPU (macOS).** Until now every engine run was headless.
- Unity (Metal, gamma and linear): 4 PlayMode tests draw a Canvas `ZuriRawImage` and a
  world-space `ZuriQuad` through a camera and read the pixels back (upright, right
  colours), and click through `GraphicRaycaster` / `PhysicsRaycaster`. An EditMode test
  draws the `ZuriPlayer` inspector and clicks its Fire button.
- Unreal (Metal RHI): `Zuri.Texture` (GPU texture read back equals the runtime's pixels),
  `Zuri.Widget` (`UZuriWidget` through Slate at layout scale 2, pointer and touch input),
  `Zuri.Component` (dynamic material on a mesh actor).
- No plugin bug was found. Each agent broke a scratch copy on purpose (un-flipped UVs,
  ignored widget offset, skipped texture update...) and the new tests caught every one.
- `scripts/verify-engines.sh` runs all of it and fails if a GPU test is skipped or
  missing. It needs the Metal Toolchain and has no headless mode any more.
- Not possible here: an IL2CPP build (the macOS IL2CPP module is not installed in Unity).

**Reviews.** The CI change: a correctness review (9 findings) and a ponytail review (14),
before its first push to the scratch branch. The engine tests and gate: code review (10)
and ponytail (12). Fixed from them, beyond the list above: the engine gate could pass
with the drawing tests left out (a silent `-nullrhi` fallback and two "headless"
switches, all removed); the quad test could vanish without notice (the gate now expects
4 PlayMode tests); a failed render crashed the Unreal test run instead of failing an
assertion; the path check had been narrowed too far (the `$HOME` check is back).
Not taken: a cargo cache in CI (the slowest job is 12 minutes), merging the two Unity
test fixtures' helpers, deleting the dedicated "upright" test.

**Verified.** CI run 43708678100: macOS, Linux and Windows all green. Locally
`bash scripts/verify.sh` exits 0 and `bash scripts/verify-engines.sh` passes both halves
on the final tree (Unity 15 EditMode + 4 PlayMode x 2 colour spaces; Unreal 5 of 5 on
the Metal RHI).
**Not verified.** Anything on Linux or Windows beyond what `verify.sh` and the two extra
CI steps run (see `memory/todo.md`); a person has still not looked at a scene in either
engine's editor window.

---

## 2026-11-15 21:00:00 - Session 2 (continued): constraints, engine gate

**Done.**
- `Constraint` (child of any drawable): `translation`, `rotation`, `scale`, `transform`,
  `distance` (closer / further / exact) and `followPath` (with `orient`), blended by
  `strength`. A second pass of `Instance::update_world` applies them in document order,
  in artboard space. Each part moves in place (the matrix is never rebuilt), so skew and
  mirroring survive and strength 0 changes nothing.
- `scripts/verify-engines.sh`: the Unity and Unreal checks, from throwaway projects it
  creates under `target/engines/`. Until now they lived in a session scratch folder.
- `advance` no longer recomputes world transforms when no property changed.

**Code review: 10 findings.**
- Fixed: the first version decomposed and rebuilt the matrix, which turned a mirrored
  object upside down and dropped skew (a pop as soon as strength left 0); `followPath`
  re-flattened its target on every idle frame (transforms are now skipped when nothing
  changed); the engine gate accepted zero Unity tests, a half-finished Unreal run, the
  wrong engine version when two are installed (plain `sort`), and a warm project whose
  compiler warnings were cached away; the editor created a Bind under a Constraint with
  a property a Constraint does not have.
- Known ceiling (in the code and in `docs/user/format.md`): a target later in the
  document is read before its own constraints.

**Ponytail review: 4 findings, 3 applied** (no `hypot`, `point_along` without copying
the contours, a shorter pass loop). Kept `sort -V` in the engine gate: `ls` alone sorts
5.10 before 5.8.

**Not done.** IK (planned with bones), min / max limits, per-axis copies.

**Verified.** `bash scripts/verify.sh` exits 0: 71 Rust tests, 7 Node tests, browser
checks, editor 20 tests + build, Unity dotnet harness. wasm brotli 88 / 116 / 154 KB.
`bash scripts/verify-engines.sh` exits 0 on the final tree: Unity EditMode 14 of 14,
Unreal BuildPlugin and 2 of 2 automation tests.

---

## 2026-11-15 19:20:00 - Session 2 (continued): feather, trim, fit and alignment

**Grounding.** Semantics were read from Rive's docs on 2026-11-15 (rive.app/docs:
vector-feathering, manipulating-shapes/trim-path, runtimes/web/layouts, events/overview):
feather has a direction (outer / inner, "like an inner shadow or a vignette") and an
offset; trim modes are Sequential and Synced; fits are Layout, Contain, ScaleDown, Cover,
FitWidth, FitHeight, Fill, None with nine alignments; sending data to runtime code through
event properties is deprecated there, so that todo item was dropped.

**Done.**
- Fill / Stroke: `featherX`, `featherY` (the paint moves with the outline: a drop shadow);
  Fill: `featherInner` (crisp outline times one minus the blurred, offset outline).
- Stroke: `trimMode: synced`.
- Fit: `fitWidth`, `fitHeight`, `scaleDown`, and alignment. Core `Instance::fit` takes the alignment;
  C ABI: the alignment is OR-ed into the `fit` argument (`ZURI_ALIGN_*`), no signature
  changed; web `align`; CLI `--align`; Unity `ZuriAlignment`; Unreal `EZuriAlignment`.
- Behaviour change: a `fit` value the runtime does not know now means contain (it used to
  mean none). Written in `zuri.h`.

**Code review: 10 findings.**
- Fixed: a feather offset moved the outline but not its gradient or shader; the CLI fitted
  once instead of every frame (wrong for a scene that animates its artboard size);
  `--align` accepted anything containing the right words; the inner feather copied the
  contours and rasterized a padded region it cannot paint; `docs/dev/rendering.md` did
  not describe the inner pass.
- Not changed: `ZURI_ABI_VERSION` stays 1 although an old runtime would misread the new
  `fit` values. Nothing has been released; the policy is now written in `docs/dev/abi.md`
  (additions before the first release do not bump it).

**Ponytail review: 5 findings, 4 applied** (one `fit` function that takes the alignment,
a five-line `--align` parser, a loop for synced trim, no clamping in the web wrapper).
Not applied: moving the offset into `Raster::fill` (the reviewer measured no gain).

**Verified.** `bash scripts/verify.sh` exits 0: 70 Rust tests, 7 Node tests, browser
checks, editor 20 tests + build, Unity dotnet harness. wasm brotli 87 / 115 / 153 KB.
Engines, on the final library: Unity EditMode 14 of 14 with no compiler errors or
warnings, dotnet harness 49 checks, Unreal `BuildPlugin` BUILD SUCCESSFUL, `Zuri.Button`
and `Zuri.Image` succeed (the alignment check is inside `Zuri.Button`).
**Not verified.** Anything on screen in either engine; `UZuriWidget` / `UZuriComponent`
alignment (compiles, no test); only `None` + `BottomRight` is tested through the wrappers.

---

## 2026-11-15 17:10:00 - Session 2 (continued): Milestone 1, state machine depth

**Done** (file format and core only; C ABI unchanged).
- Transition: `interp` (linear / hold / cubic with the ease points), `pause` (freeze the
  state being left), `chance` (random choice among the transitions that could fire; a
  xorshift generator that restarts with the machine, so runs repeat exactly).
- `Action` children of Transitions and Listeners change view-model Properties.
- Condition `with`: compare against another Property.
- State `kind: direct`: BlendAnimations each at its own weight (`value` or `input`).

**Code review of this unit: 10 findings.** The first design of `direct` was wrong and was
replaced: it mixed over a pose captured on entry, which crept when fading out, popped
when a fade in ended, froze what earlier layers animated and never settled. It is now a
pure pose: design values, then each animation at its weight (`instance.rs: apply_state`),
so it fades, seeks and settles like an animation state (test:
`direct_states_mix_animations_over_the_design_pose`).
- Also fixed: `chance` draws depended on history after a restart or seek; a trigger fired
  by a Transition's Action was lost for layers that had already run (it now fires for the
  whole next frame); an overshooting ease curve over-mixed; an Action without `input`
  and a `with` on a string or color were silently accepted; the editor's tooltip for the
  new state kind.
- Kept: a `direct` state overrides earlier layers on the properties it animates, as every
  state does (documented).

**Ponytail review: 9 findings, 8 applied.** They brought the lite wasm from 90.7 KB back
to 86.8 KB brotli, below where the unit started (87.1): the schema's doc strings are no
longer compiled into player builds (3.2 KB), `direct` and `blend` share one list, and
the `chance` draw is a single pass without a Vec. Not applied: folding the final mix of a
`direct` state into one loop, because a property keyed by two of its animations would be
mixed twice during a fade (the test now has such a pair).

**Watch.** Size: each feature batch adds 2 to 3 KB brotli to the lite wasm (budget 100).
Measure after every batch; `memory/todo.md` has a size-pass item.

**Verified.** `bash scripts/verify.sh` exits 0: 68 Rust tests, 7 Node tests, browser
checks, editor 20 tests + build, Unity dotnet harness.
**Not verified.** Nothing of this batch was tried in the editor UI (the inspector is
schema-driven, so the properties appear; the state graph was not looked at).

---

## 2026-11-15 15:30:00 - Session 2 (continued): Milestone 1, first batch

**Done** (all in the file format and core; the C ABI did not change).
- Components: `play` on a NestedArtboard, and `Bind` with `into` to write the parent's
  view-model Properties into the child's (triggers fire through in the same frame).
  Resolved at load in `file.rs: build_nests`; mistakes are load errors.
- Timeline events: `Cue` children of an Animation (`instance.rs: cues`).
- Draw order: `order` (number, animatable) and `solo` on a Node (whole number, 1-based,
  0 = all). Drawing, hit-testing, listeners and `bounds` all go through `drawn_children`.
- Blend modes hue, saturation, color, luminosity (`raster.rs: blend_hsl`).
- `examples/components.json` uses all of it; rendered and looked at.
- wasm size: every runtime sort now goes through `geom::sort_keyed`, because each
  `sort_by` call site compiled its own ~6 KB copy of the standard sort. Lite build
  87 KB brotli with the four features in (84.6 before them, 88.8 without this).

**Reviews of this unit.** Code review: 10 findings. Ponytail: 5, all applied.
- Fixed: listeners and nested artboards hidden by a `solo` still took pointer input;
  `solo` was a float and passed through other children during a cross-fade (now a whole
  number, which steps); `seek` made nested artboards report their cues; a blend state's
  animations reported cues at weight 0; a `Bind` with `into` outside a NestedArtboard was
  silently dropped; `bounds` ignored `solo`; `drawn_children` allocated per node per
  frame (now borrows unless `order` is used); `order` / `solo` changes dropped the
  cached outline.
- Kept, documented in `docs/user/format.md`: `order` glides in a cross-fade like any
  number; a state that is being faded out does not report cues.

**Verified.** `bash scripts/verify.sh` exits 0: 65 Rust tests, 7 Node tests, browser
checks, editor 20 tests + build, Unity dotnet harness. wasm brotli 87 / 115 / 149 KB.
**Not verified.** The editor now offers whole-number properties (`solo`, `points`) for
keying: its unit tests pass, the UI was not driven. No engine run (bindings unchanged).

---

## 2026-11-15 13:00:00 - Session 2 (continued): small runtime follow-ups

**Done.**
- Gradients are dithered (8.8 fixed-point ramp, position threshold `(143x + 79y) mod 256`).
  A Bayer matrix was tried first and rejected: on a vertical gradient its rows do not
  average evenly, which drew the very lines it was meant to remove.
- The compiler checks what a reference reaches (`schema::ref_types`: asset kinds, Listener
  targets, Clip sources) and that host-facing names are unique (`schema::host_named`).
  Schema JSON carries `types`; the editor's `ASSET_FOR` / `POINTER_TYPES` mirrors are gone.
- A cross-fade that interrupts one already on screen now starts from the picture as it is
  (nothing plays underneath; documented in `docs/user/format.md`). The old code jumped to
  the interrupted state's full pose first: reproduced (x went 50 -> 99) by running the new
  test against the previous `instance.rs`, then fixed. Two fades that fire in one frame
  are one fade from the state on screen.
- `scripts/cargo-ship.sh`: the wasm builds embedded the home directory in panic paths
  (15 strings in `zuri.wasm`). It wraps cargo with `--config build.rustflags` remaps (so
  paths with spaces and config-file flags keep working) and `--check`s shipped binaries;
  web, Unity and Unreal builds all go through it.

**Decided not to do** (reasons in `memory/todo.md`): the bundler `zuri.wasm` copy, and
`zuri_instance_fit_view` / `flip_y`.

**Reviews of this unit.** Ponytail: 4 findings, all applied. Code review: 10 findings.
- Fixed: duplicate Artboard names were not caught (wrong scope key); a fade chained in the
  same frame froze the state on screen; the path script broke on paths with spaces, had no
  failure checks, overrode config-file flags, and only the web build checked its output;
  the dither step was even (half the thresholds per row).
- Kept, now documented: an interrupting fade stops what the interrupted states animated
  (the alternative pops); a Clip whose source is a Text clips everything away in the lite
  build (the text itself is absent there too); Property names are unique per artboard
  across ViewModels (the host's lookup is per artboard).

**Verified.** `bash scripts/verify.sh` exits 0: 62 Rust tests, 7 Node tests, browser checks,
editor 20 tests + build, Unity dotnet harness. wasm brotli 85 / 112 / 147 KB.
`build-unity-plugins.sh` and `build-unreal-libs.sh` exit 0 through `cargo-ship.sh`, and
their `--check` finds no home path in the dylib or the static library.
**Not verified.** No in-engine run for this unit (Unity EditMode, Unreal automation): the
bindings did not change, only the library under them.

---

## 2026-11-15 10:40:00 - Session 2 (continued): runtime read-back APIs, second review loop

**Done (todo "Runtime API follow-ups" and one C ABI follow-up).**
- Core + C ABI + web: `seek` (scrub a directly played animation), `states` / `layer_state`
  (the State each layer is in), `property` / `property_color` (what an object draws with
  after animation and binds), `event_names` / `declared_event_*`. Web also gained
  `playStateMachine` / `playAnimation`. No signature changed; `ZURI_ABI_VERSION` stays 1.
- Schema JSON carries each property's `key`, each type's `class` and `scene`, and each
  reference's `target` class.
- Editor: track sampling, easing and reference-class mirrors are deleted; values come from
  the runtime (`liveValue`), scrubbing uses `seek`, the state graph rings the active state.
  Kept by hand, because the schema does not carry them: `ASSET_FOR`, `POINTER_TYPES`,
  `GEOMETRY_TYPES`, `REQUIRED`; `mapFrame` (the editor owns the playhead).
- Unity and Unreal wrappers bind the new calls: `Seek`, `DeclaredEvents`, `LayerStates`,
  `TryGetProperty` / `TryGetPropertyColor` (Unity); `Seek`, `GetDeclaredEvents`,
  `GetLayerStates` (Unreal).

**Code review of this unit: 10 findings.**
- Fixed: `seek` did not reach nested artboards (they now restart and run to the same time;
  one step, see the `ponytail:` note); declared events missed nested artboards' events and
  rescanned the file per call (now built once per artboard, nested ones appended); keying
  during a state machine preview wrote the machine's pose; the inspector re-rendered on
  every stage draw; six new exports lacked `guard`; property keys were truncated to u16;
  three doc comments were displaced; a size row in `memory/test.md` was wrongly changed.
- Web `property` no longer falls back to colors: `propertyColor` is separate (no failed
  first call on the hot path).

**Ponytail review of this unit: 5 findings, 4 applied.** Not applied: deleting the Unity
EditMode test that repeats the dotnet harness (it is the only in-engine run of the new
bindings).

**Editor bugs found on the way and fixed.** A second layer's entry State could get a
duplicate name; an unnamed state machine threw every preview frame (playing now names it);
two arrow nudges inside one frame lost a step.

**Open.** Two animations (or state machines) may share a name when unreferenced; hosts
play by name and get the first (in `memory/todo.md`).

**Verified.** `bash scripts/verify.sh` exits 0: 60 Rust tests, 7 Node tests, 14 browser
checks, editor 20 tests + build, Unity dotnet harness. wasm brotli 84 / 112 / 146 KB.
Engines, on the final native library (macOS only): Unity EditMode 13 of 13 passed with no
compiler errors or warnings, dotnet harness 48 checks; Unreal `RunUAT BuildPlugin` BUILD
SUCCESSFUL, automation tests `Zuri.Button` and `Zuri.Image` succeed. Not verified: anything
on screen in either engine, other platforms, IL2CPP.

---

## 2026-11-15 05:10:00 - Session 2: first review loop (code review + ponytail review), first push

**Owner's standing rule.** Every push to GitHub comes after a code review and a ponytail
(over-engineering) review with all findings fixed; then the next item in `memory/todo.md`.
It is now rule 8 in `AGENTS.md`.

**Code review: 10 findings, all fixed, each runtime one with a test in `features.rs`.**
- `parse_svg_path` looped forever on a number after `Z` (reachable from a file, a key, or a bound string).
- Nested artboards were capped in depth but not in number: 8 self-nests per level meant 8^8 instances. Now 4096 in total, then an error.
- A reference that was never set read as object 0, so a `NestedArtboard` without `artboard` nested its own parent. Now it points at nothing.
- Cross-fades compounded when the outgoing state did not animate a property (several times too fast, frame-rate dependent). The fade now starts every frame from the pose captured when the transition fired.
- Shader paints inside nested artboards never drew and leaked requests. Textures and requests now live on the root instance.
- Playback clocks were f32 seconds (jitter after hours, frozen after days). Now f64.
- `zuri render --set` picked the setter from the value's shape, not the property's type.
- `loadRuntime()` cached a failed fetch for the life of the page.
- `browser.sh` skipped silently without Chrome and the gate accepted it. Skipping now needs `ZURI_SKIP_BROWSER=1`.
- Any property change dropped a node's cached outline (text was re-laid-out every frame when only moved).
- Also fixed from the reviewer's overflow list: a segment after `Z` lost the subpath start; `set_image` after a scene settled never appeared; `ZuriPlayer.tick` did not drain events without a listener; `zuri_file_image_bytes` returned a dangling pointer for an unknown id (now NULL); `-0` did not survive dump + build.

**Ponytail review: about 250 lines removed** (crates and web 81, editor 86 net, engine plugins 86 net), by the reviewers that had read each area. No public behaviour changed except: Unity `ZuriInstance.Render(IntPtr, ...)` and `ZuriAsset.Bytes` are gone (unused), decompile's "invalid parent" message is now the loader's.

**Not applied, on purpose.** Unity `SafeHandle` (needs an IL2CPP build to confirm), removing `ZuriPlayer`'s pass-through setters, merging Unity's playback enum into one name, merging Unreal's four pointer functions (public API; the enum disambiguates same-named animations and state machines). Recorded under "Known issues and debt" in `memory/todo.md` where they are debt.

**Editor.** Escape in a number field committed instead of reverting (reproduced, fixed). Drags did not clean up on `pointercancel` (fixed by one shared `drag`). Tauri file commands now refuse paths outside the extensions the editor handles: this stops a compromised page reading keys or writing scripts; it is not a path scope (any `.json` / `.zur` is still reachable), see the `ponytail:` note in `src-tauri/src/main.rs`.

**Repository decisions taken for the first push (reversible).** `research/` is ignored (public repo, PDFs: the owner has not decided). Built native libraries are not stored in git (Unity now matches Unreal): run `scripts/build-unity-plugins.sh` after cloning.

**Verified.** `bash scripts/verify.sh` exits 0 (58 Rust tests, 6 Node tests, 14 browser checks, editor 16 tests + build, Unity dotnet harness). wasm brotli: 83 / 111 / 145 KB. Unity EditMode and Unreal build + automation were re-run after the plugin edits; the result against the final native library is in the commit that follows this entry.

**Not verified.** Everything listed as unverified in the previous entries still is, plus: the Tauri commands' new extension check through real dialogs; the Unity inspector after `DrawPropertiesExcluding`; `SZuriWidget` with `TStrongObjectPtr` at runtime.

---

## 2026-11-15 03:25:00 - Session 1 (end): editor result, full gate green

**Editor: agent report, checked by the integrator where cheap.** Integrator observed:
16 unit tests pass, `tsc --noEmit` + `vite build` succeed, `cargo check` of the Tauri shell
succeeds, screenshots show the Tailwind + brand restyle with the logo. Agent observed
(Playwright against the dev server, console clean): the feature list in `memory/todo.md`,
and a `pnpm tauri dev` window rendering the editor. Not exercised: native dialogs, Tauri
import / build commands, the release bundle. The agent was interrupted once by a usage
limit and resumed; its work was intact.

**State at the end of the session.** `bash scripts/verify.sh` exits 0: format, clippy,
53 Rust tests, examples, generated schema reference, wasm build + 6 Node tests +
type-level test, 14 headless-Chrome checks, editor tests + build, Unity dotnet harness,
graphify refresh. Unity (12 EditMode tests) and Unreal (build + 2 automation tests) were
re-run against the final runtime; those two are not part of `verify.sh` because they need
the engines installed.

**Nothing is committed, pushed or published.** Owner decisions are listed at the top of
`memory/todo.md` (license, npm scope, package ids, memory budget, whether to commit `research/`).

**Where to start next session.** `memory/todo.md`: the three "follow-ups" lists (they are
small and came from real use by the editor and engine agents), then Milestone 1.

---

## 2026-11-15 03:10:00 - Session 1 (continued): Unity result, runtime fixes, engines re-verified

**Unity package: agent report, then re-run by the integrator.** Compiles in Unity
6000.4.8f1 with no C# errors or warnings; 12 EditMode tests pass through the native
plugin; a standalone macOS player ran as arm64 and x86_64 (agent's observation); the
plain-dotnet P/Invoke harness passes 41 checks. Not verified: on-screen appearance (all
runs were `-nographics`), inspector GUI, real EventSystem raycasts, IL2CPP, other Unity
versions, other platforms.

**Runtime bugs the Unity agent found, fixed with tests (`features.rs`).**
- A press stuck when the pointer was released outside the artboard: an `up` listener
  without a target now hears a release anywhere, and leaving the surface ends a press.
- View-model accessors were not type-checked (a number could be written into a color
  property): each accessor now only touches properties of its own type.
- `zuri.h` declares the JSON functions only under `#ifdef ZURI_JSON`.

**Final engine verification against the finished runtime (run by the integrator, not
taken from agent reports).** Native libraries rebuilt with `scripts/build-unity-plugins.sh`
and `scripts/build-unreal-libs.sh`, then:
- Unity `-runTests -testPlatform EditMode`: `Passed, total 12, passed 12, failed 0`;
  0 `error CS` / `warning CS` lines in the log.
- Unreal `RunUAT BuildPlugin`: `BUILD SUCCESSFUL`, three targets succeeded;
  `Automation RunTests Zuri` with `-nullrhi`: `Zuri.Button` and `Zuri.Image` succeed, exit code 0.
- dotnet harness: `OK` (41 checks). Web: 6 Node tests + 14 headless-Chrome checks pass.

**Note for whoever reruns Unreal tests on macOS:** the editor writes its log to
`~/Library/Logs/Unreal Engine/<Project>Editor/<Project>.log`, not to stdout.

**Still open.** The editor's migration to Tailwind + brand tokens + logo (agent in
progress); C ABI follow-ups listed in `memory/todo.md`.

---

## 2026-11-15 03:02:00 - Session 1 (continued): brand, Tailwind, Unreal result, ABI fixes

**Owner direction received.** A logo (`zuri-logo.svg`), "brand and colours borrow from
Rive", and "CSS is Tailwind". This replaced the earlier plain-CSS decision for the editor.

**Done.**
- `brand/` created: the logo as supplied, a `currentColor` variant, icon PNGs rendered by
  Zuri's own SVG importer, `tokens.css` (CSS variables), `tailwind.css` (Tailwind v4
  `@theme`), `README.md` (palette, roles, rules). The palette values were read from
  rive.app's page source (not recalled); contrast ratios in the README were computed.
- The editor agent was told to migrate to Tailwind v4 + these tokens + the logo.
- `memory/design.md` now describes the Tailwind styling rules; the demo page uses the tokens.

**Unreal plugin: agent report, spot-checked against its logs.** Compiles and links with
UE 5.8 (three targets, `BUILD SUCCESSFUL`); automation tests `Zuri.Button` and
`Zuri.Image` pass with `-nullrhi`. Not verified: anything on screen (Metal Toolchain
missing on this machine), Win64, Linux. Details in `memory/todo.md`.

**ABI gaps the Unreal agent found, now fixed in the core / C ABI.**
- `zuri_last_error` could describe an earlier call: every fallible setter, getter and
  `play_*` now records its own reason (`smoke.c` checks it).
- `zuri.h` gained `#define ZURI_ABI_VERSION 1` and notes on booleans, image formats and
  `get_string`.
- A scene with shader paints never settled on hosts that cannot draw shaders: it now keeps
  redrawing only once a host has supplied a shader texture (`features.rs` test).
These are additive / behavioural; no existing signature changed. The Unreal plugin is
being recompiled and retested against the updated library (result in the next entry).

**Also added since the first entry:** stroke dashes, pointer input and events through
nested artboards, public `ZuriPlayer.tick()`, wider byte-input types in the web package
with a type-level test.

---

## 2026-11-15 02:55:00 - Session 1: foundation built end to end

**Starting point.** An empty repository with `research/` (three reports and
`memory_structure.md`).

**Decisions taken (reasons in `memory/architecture.md`).**
- In-house deterministic CPU reference renderer and a zero-dependency core, instead of
  depending on Linebender's pre-1.0 sparse-strip crates. GPU backends come later behind
  the three raster calls in `draw.rs`.
- One C ABI for every host, including the web (no wasm-bindgen).
- Generic object model driven by schema tables; JSON as the text twin.
- State-machine inputs are view-model properties.
- Shader paints are evaluated by the host and composited by the core.
- 3D files are converted at authoring time into one packed mesh layout.

**Grounding notes (things the research got wrong or left open).**
- Two reports name a crate `vello_gpu`; crates.io has `vello_hybrid` (0.2.0) and
  `vello_cpu` (0.4.0) as of this date.
- Stable Tauri is 2.x (`@tauri-apps/cli` 2.12.4); crates.io also lists a 3.0 alpha.
- `memory_structure.md` leaves the memory budget figure blank (see `memory/test.md`).
- Headless Chrome on this machine does not drive `requestAnimationFrame` (no display
  link), so the browser check ticks players itself through `ZuriPlayer.tick()`.

**Built and verified.**
| Area | Proof |
|---|---|
| `zuri-core` runtime | 27 unit tests, 11 feature tests, 5 runtime tests with 5 golden images |
| C ABI | header/exports sync test; `smoke.c` compiled with `-Wall -Wextra -Werror` and run |
| Importers + shader translation | 6 tests on real Blender 5.3 exports (FBX, GLB, OBJ) and an SVG; the three 3D importers give the same 1,646 triangles and the same picture |
| CLI | every command run by hand and in `verify.sh`; `dump` + `build` reproduces identical bytes |
| Web package | 6 Node tests (wasm output matches the native goldens), type-level test, headless Chrome check with 14 assertions including WebGL2 shader paints |
| Sizes | wasm brotli: 81 KB lite, 108 KB with text, 142 KB editor build |
| Performance | Apple M4, one core: 0.14 - 0.48 ms per frame at example sizes, 4 - 6.6 ms at 1080p for 2D scenes, 0 of 600 idle frames rendered (`docs/dev/rendering.md`) |

**Bugs found by running things, and fixed.**
- A trigger fired every chained transition in a frame (now at most one per layer per frame).
- A transition did not start blending until the frame after it fired.
- `trim` computed wrong ranges when `start + offset` exceeded 1.
- Name resolution was not kind-aware (a State and an Animation could not share a name).
- `Runtime.load()` did not accept the output of `Runtime.compile()` under TypeScript 7's
  typed-array types (found by the editor's typecheck; now pinned by a type-level test).

**Built, not fully verified.**
- React package: builds and type-checks; not run in a browser.
- The demo page (`packages/web/demo`) relies on `requestAnimationFrame`; the player's own
  loop has not been watched in a headed browser (its logic is covered through `tick()`).

**In progress when this entry was written.** Three parallel agents: the editor
(`apps/editor`), the Unity package (`integrations/unity`), the Unreal plugin
(`integrations/unreal`). Their results are recorded in the next entry above this one.

**Open (owner decisions).** License, npm scope, Unity package id, memory budget figure,
whether `research/` is committed; first commit and push (nothing has been committed).

**Next.** `memory/todo.md`, Milestone 1.
