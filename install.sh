#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="$HOME/.local"
CONFIG_DIR="$HOME/.config/wallpaper-cava"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

cd "$SCRIPT_DIR"
cargo install --path apps/wallpaper-cava --force --root "$INSTALL_DIR"

mkdir -p "$CONFIG_DIR"
for cfg in config.toml config-dp1.toml config-dp2.toml config-hdmi.toml; do
  if [ -f "$SCRIPT_DIR/$cfg" ] && [ ! -f "$CONFIG_DIR/$cfg" ]; then
    cp "$SCRIPT_DIR/$cfg" "$CONFIG_DIR/"
  fi
done

if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
  echo -e "\nAdd to PATH:\n  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc && source ~/.bashrc"
fi

echo -e "\nDone! Run with: wallpaper-cava --config $CONFIG_DIR/config.toml"
