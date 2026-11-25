# lib-error

`error` — the error-raising primitive contract (ADR-0092):
`error.raise` fails evaluation loudly; `error.try` captures it as a
result-ctor value. Hybrid package: `.hnl` interface plus Rust atoms.

## Layout

| path | role |
| --- | --- |
| `src/index.hnl` | the interface — documentation surface; atom spec in `atoms.yaml` (P-013/T-151) |
| `src/lib.rs`, `src/ops.rs` | Rust `Op` impls (`raise`/`try` are native atoms) + `error_op_registry()` |
| `atoms.yaml`, `Cargo.toml`, `xtask/` | atom registry + Rust gate |
| `pkg.hnl`, `scripts/hnl-gate.sh` | manifest + `.hnl` gate |
| `workflows/gate.yaml` | CI — shared `handl-gates` campaign |

## Contract

- Spec-driven layout: `atoms.yaml` is the spec; the Rust `Op` impls
  live in this crate (`src/ops.rs`) and resolve via the host's
  `error_impls()` table (`plugin-lib-stdlib` `ops/kernel_impls.rs`).
- `raise` produces eval errors; `try` wraps them as `Result` values —
  keep that distinction in any new primitive.

## Verification

```bash
HNL=../host-cli/target/debug/hnl scripts/hnl-gate.sh
cargo xtask gate
```

<!-- myco:agent-awareness -->

## Agent awareness

- Session start / any work discussion: run `myco status --prompt`
  before proposing next steps — know the board first.
- User describes new work → offer to capture it (`myco todo add`,
  `issue new`, `proposal new`). Don't wait to be asked.
- Picking up a task: `myco resume <ID>` is the briefing.
- Wrapping up: `myco handoff <ID>` anything resumable; keep statuses
  honest (claimed → in-progress → blocked/done).

<!-- /myco:agent-awareness -->
