# Getting started

In ten minutes you will build the tools, write a scene, render it, and play it in a browser.

## 1. Build the command line

You need Rust (1.88 or newer). From the repository root:

```bash
cargo build -p zuri-cli --release      # produces target/release/zuri
alias zuri="$PWD/target/release/zuri"
zuri help
```

## 2. Make a scene

```bash
zuri new button.json        # a starter: an interactive button
zuri build button.json      # -> button.zur (1,141 bytes)
zuri info button.zur
```

`info` lists what is inside: the artboard, five animations, one state machine and the
view-model properties (`hover`, `pressed`, `clicks`, `accent`, `pulse`).

A scene is JSON. Every object has a `type`, properties, and `children`:

```json
{ "zuri": 1, "children": [
  { "type": "Artboard", "name": "Main", "width": 400, "height": 300, "children": [
    { "type": "Shape", "name": "ball", "x": 200, "y": 150, "children": [
      { "type": "Ellipse", "width": 80, "height": 80 },
      { "type": "Fill", "color": "#ff5533" } ] },
    { "type": "Animation", "name": "bounce", "duration": 60, "loop": "pingpong", "children": [
      { "type": "Track", "target": "ball", "property": "y", "children": [
        { "type": "Key", "frame": 0, "value": 150, "interp": "cubic" },
        { "type": "Key", "frame": 60, "value": 60 } ] } ] } ] } ] }
```

`zuri schema` prints every type and property; [the format guide](format.md) explains the model.

## 3. Render it

```bash
zuri render button.zur -o button.png --scale 2
zuri render button.zur -o hover.png --set hover=true --time 0.5
zuri render examples/hello.json -o frame.png --frames 60     # frame-0001.png ... frame-0060.png
```

`render` plays the scene like a real host would (default: the first state machine, or the
first animation) and writes PNG. `--set` and `--fire` drive the view model.

## 4. Play it in a browser

```bash
pnpm install
pnpm --filter @seedexr/zuri build      # builds the WebAssembly runtime and the TypeScript wrapper
bash scripts/build-examples.sh         # examples/*.json -> examples/dist/*.zur
python3 -m http.server 8000            # serve the repository root
# open http://localhost:8000/packages/web/demo/
```

In your own page:

```html
<canvas id="c" style="width: 320px; height: 160px"></canvas>
<script type="module">
  import { ZuriPlayer } from '@seedexr/zuri';
  const player = await ZuriPlayer.create(document.getElementById('c'), {
    src: '/button.zur',
    onEvent: (name) => console.log('event', name),
  });
  player.instance.setColor('accent', '#ff3366');
</script>
```

See [the web guide](web.md) for React, the custom element and the low-level API.

## 5. Next
- Open the scene in [the editor](../../apps/editor/README.md).
- Use it in [Unity](../../integrations/unity/README.md) or [Unreal](../../integrations/unreal/Zuri/README.md).
- Bring in artwork: [SVG and 3D models](importing.md).
