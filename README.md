# Wallpaper CAVA (revised fork)

Spectrum visualizer on the Wayland wallpaper layer (`wlr-layer-shell`,
`Layer::Bottom`). Spawns `cava` for DSP, renders bars with OpenGL.

## Layout (Cargo workspace)

- `crates/domain` — pure logic: validated config newtypes, color parsing,
  bar geometry, cava frame decoding, `AudioSource` port. No IO.
- `crates/infra` — adapters: cava child process, GL helpers, Wayland shell.
  The only place with `unsafe` (every block has a `// SAFETY:` note).
- `crates/application` — `FrameService`: port + domain wiring, stub-tested.
- `apps/wallpaper-cava` — thin binary: CLI, config, logging, DI wiring.

Rules enforced by the compiler: no `unwrap`/`expect`/`panic`, no `println!`
in hot paths (`tracing` instead), `cargo clippy -- -D warnings` clean.

## Build & run

```bash
cargo build --release
./target/release/wallpaper-cava --config config.toml
```

Validate a config without a compositor:

```bash
./target/release/wallpaper-cava --config config-dp1.toml --check-config
```

## Multi-monitor

- No `preferred_output` → one view per output, all rendering the same bars.
- `preferred_output = "DP-1"` → only that output gets a view. Run one
  instance per monitor with its own config (see `config-dp1.toml`,
  `config-dp2.toml`, `config-hdmi.toml`).
- Output plug/unplug, mode changes: views bind/unbind/resize live, no
  full surface re-creation.
