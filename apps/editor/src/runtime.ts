// Loads the Zuri editor runtime (the wasm build that can compile JSON source) and its
// schema once at startup. Everything else imports `runtime` and `schema` from here.

import { type Runtime, loadRuntime } from '@seedexr/zuri';
import wasmUrl from '@seedexr/zuri/zuri-editor.wasm?url';
import { type Schema, makeSchema } from './doc/schema.ts';

export let runtime: Runtime;
export let schema: Schema;

export async function initRuntime(): Promise<void> {
  runtime = await loadRuntime(wasmUrl);
  schema = makeSchema(runtime.schema());
}
