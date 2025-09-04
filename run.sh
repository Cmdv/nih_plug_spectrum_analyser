#!/bin/bash

# Build and Install Plugin Script
# This script builds the plugin, removes old version, and installs the new one

set -e  # Exit on any error

# Prompt for sudo password upfront and keep alive
echo "🔐 This script needs sudo access to install the plugin..."
sudo -v
while true; do sudo -n true; sleep 60; kill -0 "$$" || exit; done 2>/dev/null &

PLUGIN_NAME="plugin-learn"
CLAP_DIR="/Library/Audio/Plug-Ins/CLAP"
PLUGIN_CLAP="${PLUGIN_NAME}.clap"

echo "🔨 Building plugin..."
cargo xtask bundle $PLUGIN_NAME --release

echo "✅ Build completed!"

echo "🗑️  Removing old plugin if it exists..."
if [ -d "$CLAP_DIR/$PLUGIN_CLAP" ]; then
    sudo rm -rf "$CLAP_DIR/$PLUGIN_CLAP"
    echo "✅ Old plugin removed"
else
    echo "ℹ️  No existing plugin found"
fi

echo "📦 Installing new plugin..."
sudo cp -r "./target/bundled/$PLUGIN_CLAP" "$CLAP_DIR/"

echo "🎉 Plugin installed successfully!"
echo "📍 Location: $CLAP_DIR/$PLUGIN_CLAP"

# Start development session with tmux
SESSION_NAME="plugin_dev"
LOG_FILE="/Users/cmdv/Library/Logs/Bitwig/nih.log"

echo "📺 Starting development session..."

# Kill existing session if it exists
tmux has-session -t $SESSION_NAME 2>/dev/null && tmux kill-session -t $SESSION_NAME

# Create new tmux session
tmux new-session -d -s $SESSION_NAME -n main

# Split window vertically (left/right)
tmux split-window -h -t $SESSION_NAME:main

# Left pane: Start Bitwig
tmux send-keys -t $SESSION_NAME:main.0 'echo "🎵 Starting Bitwig Studio..."' Enter
tmux send-keys -t $SESSION_NAME:main.0 'NIH_LOG='$LOG_FILE' "/Applications/Bitwig Studio.app/Contents/MacOS/BitwigStudio"' Enter

# Right pane: Monitor logs
tmux send-keys -t $SESSION_NAME:main.1 'echo "📋 Monitoring NIH-plug logs..."' Enter
tmux send-keys -t $SESSION_NAME:main.1 'echo "Waiting for log file..."' Enter
tmux send-keys -t $SESSION_NAME:main.1 'while [ ! -f '$LOG_FILE' ]; do sleep 1; done && tail -f '$LOG_FILE Enter

# Focus on left pane (Bitwig)
tmux select-pane -t $SESSION_NAME:main.0

# Attach to session
echo "🎯 Attaching to tmux session '$SESSION_NAME'"
echo "   Left pane: Bitwig Studio"
echo "   Right pane: Log monitoring"
echo "   Use Ctrl+B then arrow keys to switch panes"
echo "   Use Ctrl+B then d to detach"

tmux -CC attach-session -t $SESSION_NAME
