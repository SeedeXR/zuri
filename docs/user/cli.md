# The `zuri` command line

One binary for building, inspecting, importing and rendering. Every command accepts a
`.zur` file or JSON source, prints text by default and a single JSON object with `--json`,
and exits non-zero on failure. That makes it equally usable by hand, in CI and by agents.

```bash
cargo build -p zuri-cli --release      # target/release/zuri
```

## Commands

| Command | What it does |
|---|---|
| `zuri new <out.json>` | Write a starter scene (the interactive button). |
| `zuri build <src.json> [-o out.zur]` | Compile JSON source: resolve `@path` assets, convert referenced 3D files, translate shaders, validate, write `.zur`. |
| `zuri dump <file.zur> [-o out.json]` | Turn a `.zur` back into JSON source. `build` of the result reproduces the same bytes. |
| `zuri import <file> [-o out.json]` | Convert `.svg`, `.obj`, `.gltf`, `.glb` or `.fbx` into JSON source. See [importing](importing.md). |
| `zuri validate <file>` | Load the file and instantiate every artboard. Exit 1 with the reason if anything is wrong. |
| `zuri info <file>` | List artboards, animations, state machines and view-model properties. |
| `zuri render <file> -o out.png` | Render one frame, or `--frames N` for a numbered sequence. |
| `zuri bench <file>` | Time `advance` + `render` over many frames. |
| `zuri mcp` | Serve the commands to an AI agent as MCP tools ([below](#as-mcp-tools-zuri-mcp)). |
| `zuri schema [--markdown]` | Print every type and property as JSON (or as the Markdown reference). The JSON also gives each property's permanent `key`, the `class` a type's names are unique in, whether it is a `scene` object, the `target` class of each reference, and the `types` it accepts when only some of the class will do (an Image's `asset` takes an ImageAsset). |

## Render and bench options

| Option | Meaning |
|---|---|
| `--artboard <name or index>` | Which artboard (default: the first). |
| `--animation <name>` / `--state-machine <name>` | What to play (default: first state machine, else the first animation that no Joystick scrubs). |
| `--time <seconds>` | Advance this long before the first frame, in 1/60 s steps like a real host. |
| `--frames <n>` `--fps <n>` | Write `n` frames, `1/fps` apart, as `out-0001.png` ... |
| `--width` `--height` `--scale` | Output size (default: artboard size x scale). |
| `--fit contain\|cover\|fill\|none\|fitWidth\|fitHeight\|scaleDown` | How the artboard fits the output. |
| `--align center\|topLeft\|topCenter\|...\|bottomRight` | Where it sits in the room left over (default `center`). |
| `--background <#rrggbb[aa]>` | Fill behind the artboard (default transparent). |
| `--set name=value` | Set a view-model property. The value is read as the property's own type: a number, `true`/`false`, a color (`#rgb`, `#rrggbb`, `#rrggbbaa`) or text. Repeatable. |
| `--fire name` | Fire a trigger. Repeatable. |

## Machine-readable output

```bash
$ zuri info examples/hello.json --json
{"artboards":[{"animations":[{"duration":60.0,"fps":60.0,"loop":"pingpong","name":"bounce","tracks":5}],"events":[],
  "height":300.0,"name":"Hello","properties":[],"stateMachines":[],"width":400.0}],"bytes":2948,"objects":37,"ok":true}

$ zuri render examples/hello.json --animation nope --json
{"error":"no animation \"nope\". Available: bounce","ok":false}        # exit status 1
```

## Errors are written to be fixed

```
$ zuri build broken.json
error: children[0].children[11].children[0].children[0].children[0].to: no State is named "idel" here. Available: down, entry, entry2, hovered, idle, off, on
```

The message gives the JSON path, what was expected and what is available.

## For agents
1. `zuri schema` - learn the vocabulary (types, allowed parents, properties, enums, defaults).
2. Write JSON source. 3. `zuri validate file.json --json` - fix what it reports.
4. `zuri render file.json -o out.png --time 1` - look at the result.

## As MCP tools (`zuri mcp`)

An agent that has no shell (a chat application, a hosted assistant) can use the same
commands through the [Model Context Protocol](https://modelcontextprotocol.io). `zuri mcp`
serves them on stdin / stdout. Register it wherever the client takes a command, for
example in Claude Code:

```bash
claude mcp add zuri -- /path/to/zuri mcp
```

or in a client that reads a JSON configuration:

```json
{ "mcpServers": { "zuri": { "command": "/path/to/zuri", "args": ["mcp"] } } }
```

The tools are the commands above, except `new` and `bench`, with their options as
arguments by name (`time`, `scale`, `animation`, `state-machine`, `set`, `fire`, ...).

- `render` **returns the picture**, so the agent sees what it made. It draws one frame a
  call (call it again with another `time` for another moment; `time` is at most 3600
  seconds). `output` also writes the PNG.
- `validate`, `info` and `render` take a `file` (a path) or the scene itself as `source`,
  so an agent can write a scene, check it and look at it without keeping a file. (The
  server puts an inline scene and an unkept picture in a temporary folder of its own for
  the length of the call.) A scene given as `source` cannot use `"@path"` assets: they
  are looked up next to a file.
- A call that cannot work (a broken scene, an argument the tool does not have) comes back
  as a result that says why, for the agent to read and try again.
- A relative path is taken from the folder the client started the server in.

The server speaks the protocol revision of 2026-07-28 and, with clients that open with
`initialize`, the earlier ones (2025-11-25 back to 2024-11-05). It offers tools only, and
handles one call at a time. It reads and writes the paths it is given, like the command
line: the client asks its user before a tool runs.
