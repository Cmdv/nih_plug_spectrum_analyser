#!/bin/bash

# Build and Install Plugin Script (without development session)
# This script builds the plugin, removes old version, and installs the new one
# Usage: ./build.sh [--debug]
#   --debug: Enable WGPU debug logging

set -e  # Exit on any error

# Parse command-line arguments
DEBUG_MODE=false
for arg in "$@"; do
    case $arg in
        --debug)
            DEBUG_MODE=true
            shift
            ;;
        *)
            ;;
    esac
done

# Prompt for sudo password upfront and keep alive
echo "🔐 This script needs sudo access to install the plugin..."
sudo -v
while true; do sudo -n true; sleep 60; kill -0 "$$" || exit; done 2>/dev/null &

PLUGIN_NAME="spectrum_analyser"
CLAP_DIR="/Library/Audio/Plug-Ins/CLAP"
PLUGIN_CLAP="${PLUGIN_NAME}.clap"

echo "🗑️  Removing old plugin if it exists..."
if [ -d "$CLAP_DIR/$PLUGIN_CLAP" ]; then
    sudo rm -rf "$CLAP_DIR/$PLUGIN_CLAP"
    echo "✅ Old plugin removed"
else
    echo "ℹ️  No existing plugin found"
fi

echo "🔨 Building plugin..."
if [ "$DEBUG_MODE" = true ]; then
    echo "🐛 Debug mode enabled - WGPU logging active"
    RUST_LOG=wgpu_core=debug,wgpu_hal=debug cargo xtask bundle $PLUGIN_NAME --release
else
    cargo xtask bundle $PLUGIN_NAME --release
fi

echo "✅ Build completed!"

echo "📦 Installing new plugin..."
sudo cp -r "./target/bundled/$PLUGIN_CLAP" "$CLAP_DIR/"

echo "🎉 Plugin installed successfully!"
echo "📍 Location: $CLAP_DIR/$PLUGIN_CLAP"
echo "🎵 Ready to use in your DAW!"
