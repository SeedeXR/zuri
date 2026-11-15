// Unit tests for the framework-free document modules. They run against the real
// runtime (compiler, schema, instance) so the editor's assumptions cannot drift from it.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { loadRuntime } from '@seedexr/zuri';
import { cubicEase, findTrack, keyAt, mapFrame, sample, setKey, valueAt, valueOf, valuePatch } from './autokey.ts';
import { formatHex, lerpHex, parseHex } from './color.ts';
import { locatePath } from './jsonpath.ts';
import { type Node, append, atIndex, clone, find, fromSource, indexOf, insert, move, node, parentOf, remove, setProps, toSource, walk } from './model.ts';
import { defaultDocument, duplicate, group, makeChild, makePenShape, makeShape, makeState, mergeImport, penPath } from './ops.ts';
import { ensureName, removeWithDependents, rename, uniqueName } from './refs.ts';
import { ASSET_TYPES, CLASS, GEOMETRY_TYPES, SCENE_TYPES, canParent, expectedClass, makeSchema, refCandidates, refClass, resolveRef } from './schema.ts';

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

test('schema mirrors match the real schema', () => {
  for (const type of [...SCENE_TYPES, ...GEOMETRY_TYPES, ...ASSET_TYPES]) assert.ok(schema.types.has(type), type);
  assert.ok(canParent(schema, 'Shape', 'Artboard'));
  assert.ok(!canParent(schema, 'Rectangle', 'Artboard'));
  assert.ok(canParent(schema, 'Artboard', null) && !canParent(schema, 'Shape', null));
  assert.equal(refClass('State'), CLASS.state);
  assert.equal(expectedClass('Bind', 'source'), CLASS.property);
  assert.equal(expectedClass('Clip', 'source'), CLASS.drawable);
  // Every reference property name the schema knows has a class here.
  const refs = new Set([...schema.types.values()].flatMap((t) => t.properties.filter((p) => p.kind === 'reference').map((p) => p.name)));
  assert.deepEqual([...refs].sort(), ['animation', 'artboard', 'asset', 'emit', 'input', 'source', 'target', 'to']);
});

test('references resolve like the compiler and list sensible candidates', () => {
  const roots = example('button');
  const machine = roots[0].children.find((c) => c.type === 'StateMachine')!;
  const [interaction, check] = machine.children;
  const transition = interaction.children[1].children[0];
  assert.equal(resolveRef(roots, transition.id, 'to', 'hovered')?.type, 'State');
  assert.equal(resolveRef(roots, transition.id, 'to', 'ghost'), undefined);
  assert.deepEqual(refCandidates(roots, transition.id, 'to'), ['entry', 'idle', 'hovered', 'down']);
  assert.deepEqual(refCandidates(roots, check.children[1].children[0].id, 'to'), ['entry2', 'off', 'on']);
  const listener = machine.children.find((c) => c.type === 'Listener')!;
  assert.deepEqual(refCandidates(roots, listener.id, 'target'), ['glow', 'body', 'check']);
  assert.deepEqual(refCandidates(roots, listener.id, 'input'), ['hover', 'pressed', 'clicks', 'accent', 'pulse']);
  assert.deepEqual(refCandidates(roots, listener.id, 'emit'), ['clicked']);
  assert.deepEqual(refCandidates(roots, interaction.children[1].id, 'animation'), ['idle', 'hover', 'press', 'tick', 'untick']);
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

  assert.equal(uniqueName(roots, roots[0], CLASS.drawable, 'ball'), 'ball2');
  assert.equal(uniqueName(roots, roots[0], CLASS.drawable, 'fresh'), 'fresh');
  assert.equal(uniqueName(roots, roots[0], CLASS.animation, 'ball'), 'ball');
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

test('sampling matches the runtime', () => {
  const roots = example('hello');
  const anim = byName(roots, 'bounce');
  const ball = byName(roots, 'ball');
  assert.equal(valueAt(schema, anim, ball, 'y', 0), 90);
  assert.equal(valueAt(schema, anim, ball, 'y', 60), 210);
  assert.equal(valueAt(schema, anim, ball, 'x', 30), 200);
  assert.equal(valueAt(schema, undefined, ball, 'opacity', 30), 1);
  // Compare the cubic segment against the runtime through the ball's world matrix.
  const file = runtime.load(compiles(roots));
  for (const frame of [7, 20, 33, 51]) {
    const inst = file.instantiate(0);
    inst.play('bounce');
    inst.advance(frame / 60);
    const y = inst.worldMatrix(indexOf(roots, ball.id))![5];
    assert.ok(Math.abs((valueAt(schema, anim, ball, 'y', frame) as number) - y) < 0.01, `frame ${frame}`);
    inst.free();
  }
  file.free();
  assert.ok(Math.abs(cubicEase([0.42, 0, 0.58, 1], 0.5) - 0.5) < 1e-3);
  const track = node('Track', {}, [node('Key', { frame: 0, value: '#000000', interp: 'hold' }), node('Key', { frame: 10, colorValue: '#ffffff' }), node('Key', { frame: 20, value: '#ff0000' })]);
  assert.equal(sample(track, 5, schema), '#000000');
  assert.equal(sample(track, 15, schema), '#ff8080');
  assert.equal(sample(node('Track'), 5, schema), undefined);
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
  assert.equal(valueAt(schema, find(out, anim.id), ball, 'y', 60), 250);
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

test('colors parse, format and interpolate', () => {
  assert.deepEqual(parseHex('#f53'), [255, 85, 51, 255]);
  assert.deepEqual(parseHex('#11223344'), [17, 34, 51, 68]);
  assert.equal(parseHex('red'), null);
  assert.equal(formatHex([17, 34, 51, 255]), '#112233');
  assert.equal(formatHex([17, 34, 51, 68]), '#11223344');
  assert.equal(lerpHex('#000000', '#ffffff', 0.5), '#808080');
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
  for (const type of ['Animation', 'StateMachine', 'Event', 'ViewModel']) roots = append(roots, artboard.id, makeChild(type, roots, find(roots, artboard.id)!));
  const vm = find(roots, artboard.id)!.children.find((c) => c.type === 'ViewModel')!;
  roots = append(roots, vm.id, makeChild('Property', roots, vm));
  const machine = find(roots, artboard.id)!.children.find((c) => c.type === 'StateMachine')!;
  roots = append(roots, machine.id, makeChild('Listener', roots, machine));
  const layer = machine.children[0];
  const state = makeState(roots, layer, 'animation');
  assert.equal(state.props.animation, 'animation');
  roots = append(roots, layer.id, state);
  const fill = byName(roots, 'star').children[1];
  roots = append(roots, fill.id, makeChild('LinearGradient', roots, fill));
  roots = append(roots, fill.id, makeChild('Bind', roots, fill));
  assert.equal(find(roots, fill.id)!.children[1].props.source, 'property');
  compiles(roots);
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
