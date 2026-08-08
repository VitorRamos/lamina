# kitchen-sink

Tour of the Lamina language surface in one multi-stage image.

| Area | What you will see |
|------|-------------------|
| Keywords | `use`, `arg`, `const`, `let`, `fn` / `pub fn`, `pub target`, `if` / `else`, `for` / `in`, `true` / `false` |
| Operators | `+` (string/int/list), `==` `!=` (String/Int/Bool), `&&` `\|\|` (Bool) |
| Soft forms | `param(…)`, `Stage.from` / `Stage.from_arg`, `Mount.*` |
| Stage API | workdir, run, run_with, copy, copy_many, copy_from, env, arg, arg_default, user, entrypoint, cmd, expose, label, healthcheck, platform, name |
| Modules | `./helpers.lam` + `std/golang.lam` |
| Targets | `app` (production multi-stage), `debug` (sidecar shell image) |

```bash
cargo run -q -p lamina-cli -- check examples/kitchen-sink
cargo run -q -p lamina-cli -- explain examples/kitchen-sink --target app
# optional image build (needs Buildx + network for apk/go):
cargo run -q -p lamina-cli -- build examples/kitchen-sink --target app -t kitchen-sink:dev
# both pub targets (tags kitchen-sink:app and kitchen-sink:debug):
# cargo run -q -p lamina-cli -- build examples/kitchen-sink --all-targets
```
