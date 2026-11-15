// Finds where a compiler error points in JSON source text, so the Source tab can mark
// the line. Compiler paths look like `children[0].children[2].color`. Framework-free.

export interface Span {
  start: number;
  end: number;
  /** 1-based line of `start`. */
  line: number;
}

/** The span of the value at `path` in `text`, or undefined when the path does not exist. */
export function locatePath(text: string, path: string): Span | undefined {
  const steps = [...path.matchAll(/([^.[\]]+)|\[(\d+)\]/g)].map((m) => (m[2] !== undefined ? Number(m[2]) : m[1]));
  let i = 0;
  const space = () => {
    while (i < text.length && /\s/.test(text[i])) i++;
  };
  const string = () => {
    i++;
    while (i < text.length && text[i] !== '"') i += text[i] === '\\' ? 2 : 1;
    i++;
  };
  const skipValue = () => {
    space();
    if (text[i] === '"') return string();
    if (text[i] !== '{' && text[i] !== '[') {
      while (i < text.length && !/[,\]}\s]/.test(text[i])) i++;
      return;
    }
    let depth = 0;
    do {
      if (text[i] === '"') {
        string();
        continue;
      }
      if (text[i] === '{' || text[i] === '[') depth++;
      if (text[i] === '}' || text[i] === ']') depth--;
      i++;
    } while (depth > 0 && i < text.length);
  };
  /** After a skipped member: step over the comma, or report the end of the container. */
  const next = (): boolean => {
    space();
    return text[i++] === ',';
  };

  for (const step of steps) {
    space();
    if (text[i++] !== (typeof step === 'number' ? '[' : '{')) return undefined;
    if (typeof step === 'number') {
      for (let n = 0; n < step; n++) {
        skipValue();
        if (!next()) return undefined;
      }
      space();
      if (text[i] === ']') return undefined;
      continue;
    }
    for (;;) {
      space();
      if (text[i] !== '"') return undefined;
      const from = i;
      string();
      const key = text.slice(from + 1, i - 1);
      space();
      i++; // the colon
      if (key === step) break;
      skipValue();
      if (!next()) return undefined;
    }
  }
  space();
  const start = i;
  skipValue();
  return { start, end: i, line: text.slice(0, start).split('\n').length };
}

/** The 1-based source line a compiler or JSON parser error message refers to, if it names one. */
export function errorLine(text: string, message: string): number | undefined {
  const parse = /^invalid JSON: .* at line (\d+)/.exec(message);
  if (parse) return Number(parse[1]);
  const path = /^(children\[\d+\][^:\s]*):/.exec(message);
  return path ? locatePath(text, path[1])?.line : undefined;
}
