# Zuri documentation

**Using Zuri**
- [Getting started](user/getting-started.md) - build the tools, make a scene, play it.
- [The document format](user/format.md) - artboards, shapes, animation, state machines, data binding.
- [The `zuri` command line](user/cli.md)
- [Web runtime (`@seedexr/zuri`)](user/web.md) - player, React, custom element, low-level API.
- [Importing SVG and 3D models](user/importing.md)
- [Shader paints](user/shaders.md)
- [Unity](../integrations/unity/README.md) and [Unreal](../integrations/unreal/Zuri/README.md)
- [The editor](../apps/editor/README.md)

**Reference**
- [Schema reference](reference/schema.md) - every type and property (generated from the code).
- [C API](../crates/zuri-ffi/include/zuri.h)
- [Feature parity with Rive](parity.md) - what is done, partial and planned.

**Working on Zuri**
- [AGENTS.md](../AGENTS.md) - the rules and commands, for people and agents.
- [Architecture](../memory/architecture.md) and [how to implement things](../memory/instruction.md)
- [Rendering](dev/rendering.md) - how the reference renderer works, measured performance, the GPU roadmap.
- [The C ABI](dev/abi.md) - conventions every host follows.
- [Testing](../memory/test.md), [documentation standards](../memory/docs.md), [design system](../memory/design.md)
