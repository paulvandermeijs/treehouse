# Announcement video

A 50-second video of Treehouse for the README and social posts: a title card, five chapters that each show the real app next to a caption, and an end card. The app runs on a virtual display with `tools/fake-pi`, so recording needs no model or API key.

## Making it

Linux only (Xvfb). Build the app first, then:

```sh
sudo apt install xvfb xdotool dmz-cursor-theme ffmpeg
cargo build --release
cd marketing/announcement
./record.py out          # records the app, cuts out/clips/<scene>.mp4
node render.mjs out      # draws the captions and cards, out/frames/<scene>/
./compose.py out         # out/treehouse-announcement.mp4, .gif and a poster
```

`render.mjs` needs Playwright with Chromium (`NODE_PATH` pointing at a global install is enough).

| File | What it does |
| --- | --- |
| `scenes.json` | Scene order, captions, the app window's place in the frame |
| `demo-repo.sh` | The "lumen" demo project: two worktrees, one with changes for the diff |
| `run-app.sh` | Starts Treehouse on `:99` with fake-pi and scratch settings |
| `record.py` | Drives the app with xdotool and records it; the pauses follow fake-pi's script |
| `frame.html`, `render.mjs` | One scene's overlay, rendered frame by frame |
| `compose.py` | Puts the clips under the overlays and cross-fades the scenes |

Captions and card text live in `scenes.json`; rerun `render.mjs` and `compose.py` after changing them. After a UI change, rerun `record.py` too, and check the click positions in it.

The video has no sound yet.
