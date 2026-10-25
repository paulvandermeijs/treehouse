#!/usr/bin/env bash
# Starts Treehouse on a virtual display with fake-pi, scratch settings and the
# demo project, so scenes can be recorded without a model or a real desktop.
# Usage: run-app.sh <work folder> [display]
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
work=${1:?usage: run-app.sh <work folder> [display]}
display=${2:-:99}
mkdir -p "$work"
work=$(cd "$work" && pwd)

"$here/demo-repo.sh" "$work/demo" >/dev/null
rm -rf "$work/config" "$work/data" "$work/pi"
mkdir -p "$work/config/treehouse" "$work/data" "$work/pi"
cat > "$work/config/treehouse/settings.json" <<J
{
  "projects": ["$work/demo/lumen"],
  "pi_command": "$repo/tools/fake-pi",
  "theme": "mocha"
}
J

if ! DISPLAY=$display xdotool getdisplaygeometry >/dev/null 2>&1 </dev/null; then
  Xvfb "$display" -screen 0 1920x1080x24 -nolisten tcp >"$work/xvfb.log" 2>&1 &
  sleep 1
fi
export DISPLAY=$display
export XDG_CONFIG_HOME=$work/config XDG_DATA_HOME=$work/data PI_CODING_AGENT_DIR=$work/pi
export TREEHOUSE_NO_SHELL_ENV=1 FAKE_PI_HOLD=${FAKE_PI_HOLD:-6}
exec "$repo/target/release/treehouse"
