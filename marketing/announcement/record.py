#!/usr/bin/env python3
"""Records the app scenes for the announcement.

Starts Treehouse with run-app.sh on a virtual display, records the window with
ffmpeg while xdotool plays one continuous session, and cuts that recording
into one clip per chapter in <out>/clips/<scene id>.mp4.

Usage: record.py <out folder>

THEME=latte records the app in Latte instead of Mocha. The whole session is
kept in <out>/raw.mkv, with its scene marks in <out>/marks.json.

The pauses follow fake-pi's script, so change them together.
"""

import json
import os
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = Path(sys.argv[1] if len(sys.argv) > 1 else "out").resolve()
WORK = OUT / "app"
DISPLAY = ":99"
FPS = 30
W, H = 1440, 900  # Treehouse's default window size
# Where the demo project lives while recording; its path shows in the trust dialog.
DEMO_DIR = os.environ.get("DEMO_DIR", "/home/ada/code")

env = dict(os.environ, DISPLAY=DISPLAY)
mouse = [W // 2, H // 2]


def xdo(*args):
    subprocess.run(["xdotool", *map(str, args)], env=env, check=True)


def move(x, y, seconds=0.45):
    """Glides the pointer, so viewers can follow it."""
    x0, y0 = mouse
    steps = max(1, int(seconds * 60))
    for i in range(1, steps + 1):
        t = i / steps
        e = t * t * (3 - 2 * t)
        xdo("mousemove", round(x0 + (x - x0) * e), round(y0 + (y - y0) * e))
        time.sleep(seconds / steps)
    mouse[:] = [x, y]


def click(x, y, pause=0.6):
    move(x, y)
    time.sleep(0.15)
    xdo("click", 1)
    time.sleep(pause)


def open_diff_view(pause):
    """Opens the diff-view worktree, then leaves the sidebar so its hover card
    doesn't cover the chat."""
    click(64, 149, pause=0.2)
    move(760, 480, seconds=0.35)
    time.sleep(pause - 0.2)


def type_text(text):
    xdo("type", "--delay", 45, text)


def key(*keys):
    xdo("key", *keys)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    app = subprocess.Popen(
        [str(HERE / "run-app.sh"), str(WORK), DISPLAY],
        env=dict(env, DEMO_DIR=DEMO_DIR, THEME=os.environ.get("THEME", "mocha")),
        stdout=open(OUT / "app.log", "w"), stderr=subprocess.STDOUT,
    )
    window = None
    for _ in range(100):
        found = subprocess.run(["xdotool", "search", "--name", "^Treehouse$"], env=env,
                               capture_output=True, text=True).stdout.split()
        if found:
            window = found[0]
            break
        time.sleep(0.2)
    if not window:
        sys.exit("Treehouse didn't open a window; see app.log")
    xdo("windowmove", window, 0, 0)
    # The app sets its pointer only once the pointer moves over it.
    xdo("mousemove", mouse[0] - 10, mouse[1])
    xdo("mousemove", *mouse)
    time.sleep(2)

    raw = OUT / "raw.mkv"
    rec = subprocess.Popen(
        ["ffmpeg", "-v", "error", "-y", "-f", "x11grab", "-framerate", str(FPS), "-draw_mouse", "1",
         "-video_size", f"{W}x{H}", "-i", f"{DISPLAY}+0,0",
         "-c:v", "libx264", "-preset", "ultrafast", "-qp", "0", str(raw)],
        stdin=subprocess.PIPE,
    )
    # ffmpeg takes a moment to start capturing; time the marks from its
    # first frame (a lossless frame is far bigger than the file header), so
    # they match the recording.
    while not raw.exists() or raw.stat().st_size < 50_000:
        time.sleep(0.02)
    start = time.time()
    marks = {}

    def mark(name):
        marks[name] = time.time() - start

    time.sleep(1)

    # 05 Trust: the project opens with the trust prompt.
    mark("trust")
    time.sleep(1.5)
    click(912, 483, pause=1.6)
    mark("trust_end")

    # 01 Worktrees: a new worktree, then switching to another.
    mark("worktrees")
    time.sleep(0.5)
    key("ctrl+n")
    time.sleep(0.6)
    # The branch field doesn't take focus by itself on a bare X server.
    click(720, 173, pause=0.2)
    key("ctrl+a")
    type_text("onboarding")
    time.sleep(0.4)
    key("Return")
    time.sleep(1.6)
    open_diff_view(pause=1.2)
    mark("worktrees_end")

    # 02 Approvals: ask for a change; Pi wants to run a command.
    mark("approval")
    click(700, 815, pause=0.2)
    type_text("Show the worktree's diff in a pane next to the chat")
    time.sleep(0.3)
    key("Return")
    time.sleep(3.2)
    click(534, 367, pause=1.8)
    mark("approval_end")

    # 03 Sub-agents: open the helper Pi started, allow it, let it finish.
    mark("subagents")
    time.sleep(0.4)
    click(1180, 663, pause=1.4)
    click(620, 314, pause=4.8)
    open_diff_view(pause=2.2)
    mark("subagents_end")

    # 04 Diff: the worktree's changes in the dock, then side by side.
    mark("diff")
    click(1422, 19, pause=2.0)
    click(873, 19, pause=1.4)
    move(860, 520)
    for _ in range(6):
        xdo("click", 5)
        time.sleep(0.25)
    time.sleep(1.4)
    mark("diff_end")

    rec.communicate(b"q")
    app.terminate()
    (OUT / "marks.json").write_text(json.dumps(marks, indent=2))

    clips = OUT / "clips"
    clips.mkdir(exist_ok=True)
    for name in [m for m in marks if not m.endswith("_end")]:
        subprocess.run(
            ["ffmpeg", "-v", "error", "-y", "-ss", f"{marks[name]:.3f}", "-to", f"{marks[name + '_end']:.3f}",
             "-i", str(raw), "-c:v", "libx264", "-preset", "slow", "-crf", "12", "-pix_fmt", "yuv420p",
             str(clips / f"{name}.mp4")],
            check=True,
        )
        print(f"{name}: {marks[name + '_end'] - marks[name]:.1f}s")


if __name__ == "__main__":
    main()
