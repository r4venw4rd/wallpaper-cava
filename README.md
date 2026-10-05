# Wallpaper CAVA (revised fork)

Forked from [rs-pro0/wallpaper-cava](https://github.com/rs-pro0/wallpaper-cava)
— rewritten as a Cargo workspace (see Layout below) with multi-output
support. Upstream did the hard part (layer-shell + EGL plumbing); this fork
keeps the protocol behavior and restructures everything around it.

Spectrum visualizer on the Wayland wallpaper layer (`wlr-layer-shell`,
`Layer::Bottom`). Spawns `cava` for DSP, renders bars with OpenGL.

## Compatibility

Works on any compositor speaking `wlr-layer-shell` (Hyprland, Sway, …).
Tested on Hyprland only — other compositors should work but are untested.
To find your output names: `hyprctl monitors all` (Hyprland),
`swaymsg -t get_outputs` (Sway), or `wlr-randr` (generic).

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

## Multi-monitor (single instance, single config)

No `preferred_output` → one view per output, all rendering the same bars.
Per-output tuning lives in the same `config.toml`:

```toml
[outputs."DP-1"]
bars_amount = 96
bars_gap = 0.2
background_color = { hex = "#000000", alpha = 0.0 }

[outputs."DP-1".colors]
gradient_color_1 = '#20000077'
gradient_color_2 = '#dc143cdd'

[outputs."HDMI-A-1"]
bars_amount = 32
```

Anything unset falls back to the global `[bars]`/`[colors]`/`[general]`
sections. Cava runs at the max `bars_amount` across outputs; smaller views
get a resampled copy (average-pool down, lerp up). Output plug/unplug and
mode changes bind/unbind/resize views live — no full re-creation.

The old flow still works: set `preferred_output = "DP-1"` (or keep one
config per monitor, e.g. `config-dp1.toml`) to pin an instance to a single
output.
