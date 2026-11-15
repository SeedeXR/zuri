# Session start

Do this before changing anything. It takes a few minutes and prevents most wasted work.

## 1. Read (in this order)
1. `memory/project_context.md` - what Zuri is and what counts as success.
2. `memory/todo.md` - what is done, what is verified, what is next.
3. `memory/handover_session.md` - the last entries: what just changed and what is open.
4. `memory/architecture.md` - only the sections for the area you will touch.
5. `memory/instruction.md` - conventions and the recipes ("add a property", "add a type").
6. `AGENTS.md` at the repo root - the short operating rules and command list.

## 2. Reconcile memory with reality
Memory is a snapshot; the tree is the truth.
```bash
git status --short | head -30          # uncommitted work from a previous session?
git log --oneline -10                  # what landed since the last handover entry?
bash scripts/verify.sh --fast          # is the tree green right now?
```
If the tree is red before you start, say so in the handover log and fix that first.

## 3. Load the code graph
```bash
graphify update .                      # AST only, no LLM, about two seconds
graphify query "how does a Track write a property?"   # ask before grepping blindly
```

## 4. Confirm understanding
Be able to state, in one sentence each: what the code does now, what it should do, and
where the difference lives. If you cannot, keep reading.

## Operating mindset
- Zero hallucination: observed, inferred and assumed are three different registers.
- Smallest correct change; root cause over symptom; shared path over call site.
- Verify before reporting. A plan is not progress.
- Token efficiency: batch, be surgical, use the graph.

## Before you stop
1. `bash scripts/verify.sh` exits 0 (or the handover says exactly what is red and why).
2. `memory/todo.md` reflects the new state (check off, strike through, add).
3. Append an entry to `memory/handover_session.md` with a `YYYY-MM-DD HH:MM:SS` timestamp.
4. If architecture or conventions changed, update `architecture.md` / `instruction.md`.
