// Unit tests for the framework-free document modules. They run against the real
// runtime (compiler, schema, instance) so the editor's assumptions cannot drift from it.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { loadRuntime } from '@seedexr/zuri';
import { findTrack, isKeyed, keyAt, liveValue, mapFrame, playable, setKey, valueOf, valuePatch } from './autokey.ts';
import { formatHex, parseHex } from './color.ts';
import { locatePath } from './jsonpath.ts';
import { type Node, append, atIndex, clone, find, fromSource, indexOf, insert, move, node, parentOf, remove, setProps, toSource, walk } from './model.ts';
import { addTarget, defaultDocument, duplicate, group, makeChild, makePenShape, makeShape, makeState, mergeImport, penPath } from './ops.ts';
import { ensureName, removeWithDependents, rename, uniqueName } from './refs.ts';
import { GEOMETRY_TYPES, canParent, classOf, isAsset, isGlobal, makeSchema, propDef, refCandidates, resolveRef } from './schema.ts';

const runtime = await loadRuntime(new URL(import.meta.resolve('@seedexr/zuri/zuri-editor.wasm')));
const schema = makeSchema(runtime.schema());
const example = (name: string): Node[] => fromSource(readFileSync(new URL(`../../../../examples/${name}.json`, import.meta.url), 'utf8'));
const compiles = (roots: readonly Node[]) => runtime.compile(toSource(roots));
const byName = (roots: readonly Node[], name: string): Node => [...walk(roots)].find((n) => n.props.name === name)!;

test('source round-trips and compiles to the same bytes', () => {
  for (const name of ['hello', 'button']) {
    const text = readFileSync(new URL(`../../../../examples/${name}.json`, import.meta.url), 'utf8');
    const roots = fromSource(text);
    assert.deepEqual(JSON.parse(toSource(roots)), JSON.parse(text));
    assert.deepEqual(compiles(roots), runtime.compile(text));
  }
  assert.throws(() => fromSource('{'), /invalid JSON/);
  assert.throws(() => fromSource('{"children":[{"name":"x"}]}'), /children\[0\]: missing "type"/);
  assert.throws(() => fromSource('{}'), /top-level "children"/);
});

test('pre-order index is the runtime object id', () => {
  const roots = example('hello');
  const file = runtime.load(compiles(roots));
  const inst = file.instantiate(0);
  inst.advance(0);
  inst.render(400, 300, { view: [1, 0, 0, 1, 0, 0] });
  // The ring (a stroke-only ellipse outline) is not under (200, 90); the star on the ball is.
  const hit = inst.hitTest(200, 90);
  assert.equal(atIndex(roots, hit!)?.props.name, 'star');
  const ball = byName(roots, 'ball');
  assert.deepEqual(inst.worldMatrix(indexOf(roots, ball.id)), [1, 0, 0, 1, 200, 90]);
  const [x0, y0, x1, y1] = inst.bounds(indexOf(roots, ball.id))!;
  assert.ok(Math.abs(x0 - 152) < 1 && Math.abs(y0 - 42) < 1 && Math.abs(x1 - 248) < 1 && Math.abs(y1 - 138) < 1);
  assert.equal(indexOf(roots, roots[0].id), 0);
  assert.equal(indexOf(roots, 'nope'), -1);
  inst.free();
  file.free();
});

test('tree edits are immutable and share untouched branches', () => {
  const roots = example('hello');
  const ball = byName(roots, 'ball');
  const moved = setProps(roots, ball.id, { x: 10, name: undefined });
  assert.equal(find(moved, ball.id)!.props.x, 10);
  assert.ok(!('name' in find(moved, ball.id)!.props));
  assert.equal(find(roots, ball.id)!.props.x, 200);
  assert.equal(byName(moved, 'ring'), byName(roots, 'ring'));
  assert.equal(setProps(roots, 'nope', { x: 1 }), roots);

  const gone = remove(roots, new Set([ball.id]));
  assert.equal(find(gone, ball.id), undefined);
  assert.equal(remove(roots, new Set(['nope'])), roots);

  const copy = clone(ball);
  assert.notEqual(copy.id, ball.id);
  assert.deepEqual(
    copy.children.map((c) => c.type),
    ball.children.map((c) => c.type),
  );
  const front = append(roots, roots[0].id, copy);
  assert.equal(find(front, roots[0].id)!.children.at(-1)!.id, copy.id);
});

test('move reorders and reparents, and refuses cycles', () => {
  const [a, b, c] = [node('Node', { name: 'a' }), node('Node', { name: 'b' }), node('Node', { name: 'c' })];
  const roots = [node('Artboard', {}, [a, b, c])];
  const names = (r: Node[]) => r[0].children.map((n) => n.props.name).join('');
  assert.equal(names(move(roots, a.id, roots[0].id, 3)), 'bca');
  assert.equal(names(move(roots, c.id, roots[0].id, 0)), 'cab');
  assert.equal(names(move(roots, a.id, roots[0].id, 2)), 'bac');
  const nested = move(roots, a.id, b.id, 0);
  assert.equal(parentOf(nested, a.id)!.id, b.id);
  assert.equal(move(nested, b.id, a.id, 0), nested);
  assert.equal(insert(roots, null, 0, node('ImageAsset'))[0].type, 'ImageAsset');
});

test('the schema says where types live, what their names share and what references point at', () => {
  for (const type of GEOMETRY_TYPES) assert.ok(schema.types.has(type), type);
  assert.ok(canParent(schema, 'Shape', 'Artboard'));
  assert.ok(!canParent(schema, 'Rectangle', 'Artboard'));
  assert.ok(canParent(schema, 'Artboard', null) && !canParent(schema, 'Shape', null));
  // The same source property points at a different class per owner.
  assert.equal(propDef(schema, 'Bind', 'source')!.target, classOf(schema, 'Property'));
  assert.equal(propDef(schema, 'Clip', 'source')!.target, classOf(schema, 'Shape'));
  assert.notEqual(classOf(schema, 'State'), classOf(schema, 'Shape'));
  // Assets and artboards are named document-wide, everything else per artboard.
  assert.ok(isAsset(schema, 'FontAsset') && !isAsset(schema, 'Artboard') && !isAsset(schema, 'Shape'));
  assert.ok(isGlobal(schema, classOf(schema, 'ImageAsset')) && isGlobal(schema, classOf(schema, 'Artboard')));
  assert.ok(!isGlobal(schema, classOf(schema, 'Shape')) && !isGlobal(schema, classOf(schema, 'State')) && !isGlobal(schema, undefined));
  // Every reference property says what it points at.
  for (const t of schema.types.values()) for (const p of t.properties) assert.equal(p.kind === 'reference', p.target !== undefined, `${t.type}.${p.name}`);
});

test('references resolve like the compiler and list sensible candidates', () => {
  const roots = example('button');
  const machine = roots[0].children.find((c) => c.type === 'StateMachine')!;
  const [interaction, check] = machine.children;
  const transition = interaction.children[1].children[0];
  assert.equal(resolveRef(schema, roots, transition.id, 'to', 'hovered')?.type, 'State');
  assert.equal(resolveRef(schema, roots, transition.id, 'to', 'ghost'), undefined);
  assert.deepEqual(refCandidates(schema, roots, transition.id, 'to'), ['entry', 'idle', 'hovered', 'down']);
  assert.deepEqual(refCandidates(schema, roots, check.children[1].children[0].id, 'to'), ['entry2', 'off', 'on']);
  const listener = machine.children.find((c) => c.type === 'Listener')!;
  assert.deepEqual(refCandidates(schema, roots, listener.id, 'target'), ['glow', 'body', 'check']);
  assert.deepEqual(refCandidates(schema, roots, listener.id, 'input'), ['hover', 'pressed', 'clicks', 'accent', 'pulse']);
  assert.deepEqual(refCandidates(schema, roots, listener.id, 'emit'), ['clicked']);
  assert.deepEqual(refCandidates(schema, roots, interaction.children[1].id, 'animation'), ['idle', 'hover', 'press', 'tick', 'untick']);
  // Assets and artboards are offered from the whole document; a property that is not a reference offers nothing.
  const scene = [node('Artboard', { name: 'a' }, [node('Image', { name: 'pic' }), node('NestedArtboard', { name: 'nest' }), node('Shape', { name: 's' }, [node('Clip')])]), node('Artboard', { name: 'b' }), node('ImageAsset', { name: 'photo' }), node('FontAsset', { name: 'font' })];
  const [pic, nest, shape] = scene[0].children;
  assert.deepEqual(refCandidates(schema, scene, pic.id, 'asset'), ['photo']);
  assert.deepEqual(refCandidates(schema, scene, nest.id, 'artboard'), ['b']);
  assert.deepEqual(refCandidates(schema, scene, shape.children[0].id, 'source'), ['pic', 's']); // anything with an outline clips
  assert.deepEqual(refCandidates(schema, scene, pic.id, 'x'), []);
  assert.equal(resolveRef(schema, scene, pic.id, 'asset', 'photo')?.type, 'ImageAsset');
  assert.equal(resolveRef(schema, scene, pic.id, 'x', 'photo'), undefined);
});

test('rename follows references; names stay unique', () => {
  const roots = example('hello');
  const ball = byName(roots, 'ball');
  const renamed = rename(roots, ball.id, 'orb', schema);
  const anim = renamed[0].children.find((c) => c.type === 'Animation')!;
  assert.ok(findTrack(anim, 'orb', 'y'));
  assert.ok(!findTrack(anim, 'ball', 'y'));
  compiles(renamed);
  // A property-name that happens to equal an object's name is not a reference.
  const tricky = rename(setProps(roots, ball.id, { name: 'y' }), ball.id, 'z', schema);
  assert.equal(tricky[0].children.find((c) => c.type === 'Animation')!.children[0].props.property, 'y');

  assert.equal(uniqueName(schema, roots, roots[0], 'Shape', 'ball'), 'ball2');
  assert.equal(uniqueName(schema, roots, roots[0], 'Shape', 'fresh'), 'fresh');
  assert.equal(uniqueName(schema, roots, roots[0], 'Animation', 'ball'), 'ball');
  const unnamed = roots[0].children[0];
  const named = ensureName(roots, unnamed.id, schema);
  assert.equal(named.name, 'fill');
  assert.equal(ensureName(roots, ball.id, schema).roots, roots);
});

test('deleting removes what depended on it and nothing else', () => {
  const roots = example('hello');
  const pruned = removeWithDependents(roots, new Set([byName(roots, 'star').id]), schema);
  const tracks = pruned[0].children.find((c) => c.type === 'Animation')!.children;
  assert.deepEqual(
    tracks.map((t) => t.props.target),
    ['ball', 'shadow', 'ringStroke'],
  );
  compiles(pruned);

  const button = example('button');
  const noHover = removeWithDependents(button, new Set([byName(button, 'hover').id, byName(button, 'body').id]), schema);
  compiles(noHover);
  // A reference that was already broken is the user's to fix, not ours to delete.
  const broken = setProps(roots, byName(roots, 'shadow').id, { name: 'shade' });
  const kept = removeWithDependents(broken, new Set([byName(broken, 'star').id]), schema);
  assert.ok(kept[0].children.find((c) => c.type === 'Animation')!.children.some((t) => t.props.target === 'shadow'));
  // Deleting the last key of a track removes the track.
  const idle = byName(button, 'idle');
  const lastKey = idle.children[0].children[0];
  assert.equal(find(removeWithDependents(button, new Set([lastKey.id]), schema), idle.id)!.children.length, 2);
});

test('the live value is what the runtime draws at the playhead', () => {
  const roots = example('hello');
  const [ball, star, shadow] = ['ball', 'star', 'shadow'].map((name) => byName(roots, name));
  const file = runtime.load(compiles(roots));
  const inst = file.instantiate(0);
  inst.playAnimation('bounce');
  const at = (frame: number, n: Node, prop: string) => {
    inst.seek(frame / 60);
    inst.advance(0);
    return liveValue(inst, indexOf(roots, n.id), schema.props.get(prop)!);
  };
  assert.equal(at(0, ball, 'y'), 90);
  assert.equal(at(60, ball, 'y'), 210);
  // Linear keys, by hand: rotation 0 -> 144 and scaleX 0.6 -> 1.15 over 60 frames.
  assert.equal(at(15, star, 'rotation'), 36);
  assert.equal(at(30, shadow, 'scaleX'), 0.875);
  // Not animated: the base value as it was written (f32 noise removed), or the schema default.
  assert.equal(at(30, ball, 'x'), 200);
  assert.equal(at(30, star.children[0], 'innerRadius'), 0.45);
  assert.equal(at(30, ball, 'opacity'), 1);
  assert.equal(at(30, star.children[1], 'color'), '#ffffffcc');
  // A type without the property, and a node the compiled document does not have.
  assert.equal(at(30, ball, 'color'), undefined);
  assert.equal(liveValue(inst, indexOf(roots, 'nope'), schema.props.get('y')!), undefined);
  inst.free();
  file.free();
  assert.deepEqual(mapFrame(60, 'pingpong', 90), { frame: 30, finished: false });
  assert.deepEqual(mapFrame(60, 'loop', 70), { frame: 10, finished: false });
  assert.deepEqual(mapFrame(60, undefined, 70), { frame: 60, finished: true });
});

test('auto-key writes keys instead of base values', () => {
  const roots = example('hello');
  const anim = byName(roots, 'bounce');
  const ball = byName(roots, 'ball');
  // Update an existing key.
  let out = setKey(schema, roots, anim.id, ball.id, 'y', 60, 250);
  assert.equal(valueOf(keyAt(find(out, anim.id), ball, 'y', 60)!), 250);
  assert.equal(find(out, ball.id)!.props.y, 90);
  // Insert a key between two others: stays sorted.
  out = setKey(schema, out, anim.id, ball.id, 'y', 30, 100);
  assert.deepEqual(
    findTrack(find(out, anim.id)!, 'ball', 'y')!.children.map((k) => k.props.frame),
    [0, 30, 60],
  );
  assert.ok(keyAt(find(out, anim.id), ball, 'y', 30));
  assert.equal(keyAt(find(out, anim.id), ball, 'y', 31), undefined);
  // A new track on an unnamed object names it.
  const fill = find(out, ball.id)!.children[1];
  out = setKey(schema, out, anim.id, fill.id, 'color', 12, '#00ff00');
  const name = find(out, fill.id)!.props.name as string;
  assert.equal(name, 'fill');
  assert.equal(valueOf(findTrack(find(out, anim.id)!, name, 'color')!.children[0]), '#00ff00');
  compiles(out);
  assert.deepEqual(valuePatch('#zz', true), { value: undefined, colorValue: undefined, textValue: '#zz' });
  assert.deepEqual(valuePatch('#ff0000'), { value: '#ff0000', colorValue: undefined, textValue: undefined });
});

test('colors parse and format', () => {
  assert.deepEqual(parseHex('#f53'), [255, 85, 51, 255]);
  assert.deepEqual(parseHex('#11223344'), [17, 34, 51, 68]);
  assert.equal(parseHex('red'), null);
  assert.equal(formatHex([17, 34, 51, 255]), '#112233');
  assert.equal(formatHex([17, 34, 51, 68]), '#11223344');
});

test('new objects compile', () => {
  let roots = defaultDocument();
  const artboard = roots[0];
  roots = append(roots, artboard.id, makeShape('Star', 'star', 100, 80, 60, 40));
  roots = append(
    roots,
    artboard.id,
    makePenShape(
      [
        { x: 10, y: 10, hx: 0, hy: 0 },
        { x: 110, y: 10, hx: 0, hy: 30 },
        { x: 60, y: 90, hx: 0, hy: 0 },
      ],
      true,
      'pen',
    ),
  );
  const pen = byName(roots, 'pen');
  assert.deepEqual([pen.props.x, pen.props.y], [60, 50]);
  assert.equal(pen.children[0].props.d, 'M-50 -40 C-50 -40 50 -70 50 -40 C50 -10 0 40 0 40 Z');
  assert.equal(
    penPath(
      [
        { x: 0, y: 0, hx: 0, hy: 0 },
        { x: 10, y: 5, hx: 0, hy: 0 },
      ],
      false,
    ),
    'M0 0 L10 5',
  );
  for (const type of ['Animation', 'StateMachine', 'Event', 'ViewModel']) roots = append(roots, artboard.id, makeChild(type, roots, find(roots, artboard.id)!, schema));
  const vm = find(roots, artboard.id)!.children.find((c) => c.type === 'ViewModel')!;
  roots = append(roots, vm.id, makeChild('Property', roots, vm, schema));
  const machine = find(roots, artboard.id)!.children.find((c) => c.type === 'StateMachine')!;
  roots = append(roots, machine.id, makeChild('Listener', roots, machine, schema));
  const layer = machine.children[0];
  const state = makeState(roots, layer, 'animation', schema);
  assert.equal(state.props.animation, 'animation');
  roots = append(roots, layer.id, state);
  const fill = byName(roots, 'star').children[1];
  roots = append(roots, fill.id, makeChild('LinearGradient', roots, fill, schema));
  roots = append(roots, fill.id, makeChild('Bind', roots, fill, schema));
  assert.equal(find(roots, fill.id)!.children[1].props.source, 'property');
  compiles(roots);
});

test('the objects no tool draws are made with names and sensible values', () => {
  let roots = example('hello');
  const artboard = roots.find((r) => r.type === 'Artboard')!;
  for (const type of ['Node', 'Layout', 'Bone']) roots = append(roots, artboard.id, makeChild(type, roots, find(roots, artboard.id)!, schema));
  const made = find(roots, artboard.id)!.children.slice(-3);
  assert.deepEqual(
    made.map((n) => [n.type, n.props]),
    [
      ['Node', { name: 'group' }],
      ['Layout', { name: 'layout', gap: 8, padding: 8 }],
      ['Bone', { name: 'bone', length: 60 }],
    ],
  );
  // A Layout goes into a Layout, and the document still compiles.
  const layout = made[1];
  roots = append(roots, layout.id, makeChild('Layout', roots, layout, schema));
  const inner = find(roots, layout.id)!.children[0];
  assert.equal(inner.props.name, 'layout2');
  compiles(roots);

  // A new object goes into the nearest container round the selection: the selected
  // Layout itself, the Layout round a selected Fill; never into a Shape (it holds
  // geometry), and into the artboard when nothing fits or several things are selected.
  const into = (selection: string[], type: string) => addTarget(roots, selection, type, find(roots, artboard.id)!, schema).id;
  const fill = makeChild('Fill', roots, inner, schema);
  roots = append(roots, inner.id, fill);
  const ball = byName(roots, 'ball');
  assert.deepEqual(
    [into([inner.id], 'Layout'), into([fill.id], 'Bone'), into([ball.id], 'Node'), into([ball.children[0].id], 'Layout'), into([], 'Node'), into([inner.id, ball.id], 'Node'), into([made[2].id], 'Layout')],
    [inner.id, inner.id, artboard.id, artboard.id, artboard.id, artboard.id, made[2].id],
  );
});

test('a Bind added to a Run binds its text', () => {
  const roots = example('text');
  const run = byName(roots, 'body').children.find((c) => c.type === 'Run')!;
  assert.equal(makeChild('Bind', roots, run, schema).props.property, 'text');
});

test('group and duplicate keep the document valid', () => {
  const roots = example('hello');
  const [ball, star] = [byName(roots, 'ball'), byName(roots, 'star')];
  const grouped = group(roots, [ball.id, star.id], schema)!;
  const wrapper = find(grouped.roots, grouped.id)!;
  assert.deepEqual(
    wrapper.children.map((c) => c.props.name),
    ['ball', 'star'],
  );
  assert.equal(grouped.roots[0].children.indexOf(wrapper), 3);
  compiles(grouped.roots);
  assert.equal(group(roots, [ball.id, ball.children[0].id], schema), undefined);

  const ring = byName(roots, 'ring');
  const dup = duplicate(roots, [ring.id], schema);
  const copy = find(dup.roots, dup.ids[0])!;
  assert.equal(copy.props.name, 'ring2');
  assert.equal(copy.children[1].props.name, 'ringStroke2');
  assert.equal(dup.roots[0].children.indexOf(copy), roots[0].children.indexOf(ring) + 1);
  compiles(dup.roots);

  // Duplicating a state machine keeps its transitions inside the copy.
  const button = example('button');
  const machine = button[0].children.find((c) => c.type === 'StateMachine')!;
  const twice = duplicate(button, [machine.id], schema);
  const copied = find(twice.roots, twice.ids[0])!;
  assert.equal(copied.children[0].children[1].children[0].props.to, 'hovered2');
  compiles(twice.roots);
});

test('an imported document merges into the active artboard', () => {
  const roots = append(defaultDocument(), null, node('MeshAsset', { name: 'mesh' }));
  const imported = [
    node('MeshAsset', { name: 'mesh' }),
    node('Artboard', { name: 'Imported', width: 10, height: 10 }, [node('Fill', { color: '#fff' }), node('Model', { name: 'model', asset: 'mesh' }), node('Animation', { name: 'spin' })]),
  ];
  const merged = mergeImport(roots, imported, roots[0].id, 'cube', schema);
  assert.deepEqual(
    merged.roots.map((r) => r.props.name),
    ['Artboard', 'mesh', 'mesh2'],
  );
  const wrapper = find(merged.roots, merged.id!)!;
  assert.equal(wrapper.type, 'Node');
  assert.deepEqual(
    wrapper.children.map((c) => [c.type, c.props.asset]),
    [['Model', 'mesh2']],
  );
  assert.equal(mergeImport(roots, [node('Artboard')], roots[0].id, 'x', schema).id, undefined);
});

test('compiler error paths are located in the source text', () => {
  const text = toSource(example('hello'));
  const at = locatePath(text, 'children[0].children[2].children[1].color');
  assert.equal(text.slice(at!.start, at!.end), '"#ffffff55"');
  assert.equal(text.split('\n')[at!.line - 1].trim(), '"color": "#ffffff55",');
  const whole = locatePath(text, 'children[0].children[1]');
  assert.ok(text.slice(whole!.start).startsWith('{'));
  assert.equal(locatePath(text, 'children[9].x'), undefined);
  assert.equal(locatePath('{"a": "x\\"]", "children": [ {"k": [1, {"z": true}]} ]}', 'children[0].k[1].z')!.line, 1);
});

test('every new layer gets an entry state with a name of its own', () => {
  // A drawable called "entry" is in another class: it neither takes the name nor shifts it.
  let roots = append(defaultDocument(), null, node('Artboard', { name: 'other' }, [node('Layer', {}, [node('State', { name: 'entry' })])]));
  roots = append(roots, roots[0].id, node('Shape', { name: 'entry' }));
  roots = append(roots, roots[0].id, makeChild('StateMachine', roots, roots[0], schema));
  const machine = roots[0].children.at(-1)!;
  roots = append(roots, machine.id, makeChild('Layer', roots, machine, schema));
  roots = append(roots, roots[0].id, makeChild('StateMachine', roots, roots[0], schema));
  const states = [...walk([roots[0]])].filter((n) => n.type === 'State').map((n) => n.props.name);
  assert.deepEqual(states, ['entry', 'entry2', 'entry3']);
});

test('only what the compiled file has by name is played', () => {
  const [machine, animation, bare] = [node('StateMachine', { name: 'm' }), node('Animation', { name: 'a' }), node('StateMachine')];
  const compiled = [node('Artboard', {}, [machine, animation, bare])];
  assert.equal(playable(compiled, machine), machine);
  assert.equal(playable(compiled, animation), animation);
  assert.equal(playable(compiled, bare), undefined);
  assert.equal(playable(compiled, { ...machine, props: { name: '' } }), undefined);
  assert.equal(playable(compiled, { ...animation, props: { name: 'renamed since' } }), undefined);
  assert.equal(playable(compiled, node('Animation', { name: 'added since' })), undefined);
  assert.equal(playable(compiled, undefined), undefined);
});

test('edits are keyed only while the stage shows the timeline at the playhead', () => {
  const roots = [...example('hello'), node('Artboard', { name: 'other' }, [node('Shape', { name: 'far' })])];
  const [ball, bounce] = [byName(roots, 'ball'), byName(roots, 'bounce')];
  const animate = { mode: 'animate', animationId: bounce.id, previewing: false, doc: roots, artboardId: roots[0].id };
  assert.ok(isKeyed(schema, animate, ball));
  assert.ok(isKeyed(schema, animate, ball.children[1]));
  // A state machine preview plays the machine: its pose is not the playhead's, so nothing is keyed from it.
  assert.ok(!isKeyed(schema, { ...animate, previewing: true }, ball));
  assert.ok(!isKeyed(schema, { ...animate, mode: 'design' }, ball));
  assert.ok(!isKeyed(schema, { ...animate, animationId: null }, ball));
  // Not a scene object, the artboard itself, an object of another artboard.
  assert.ok(!isKeyed(schema, animate, bounce));
  assert.ok(!isKeyed(schema, animate, roots[0]));
  assert.ok(!isKeyed(schema, animate, byName(roots, 'far')));
});
