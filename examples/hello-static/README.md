# hello-static

Minimal multi-stage Lamina example (`copy_from` between stages).

```bash
# from repo root
cargo run -p lamina-cli -- check --path examples/hello-static
cargo run -p lamina-cli -- explain --path examples/hello-static --target app
cargo run -p lamina-cli -- build --path examples/hello-static --target app -t hello-static:dev

docker run --rm hello-static:dev
# → hello-static
```
