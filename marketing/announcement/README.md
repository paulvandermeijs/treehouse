# Announcement video

A 50-second video of Treehouse for the README and social posts: a title card, five chapters that each show the real app next to a caption, and an end card, with a voiceover and quiet background music. The app runs on a virtual display with `tools/fake-pi`, so recording needs no model or API key.

## Making it

Linux only (Xvfb). Build the app first, then:

```sh
sudo apt install xvfb xdotool dmz-cursor-theme ffmpeg
cargo build --release
cd marketing/announcement
./record.py out          # records the app, cuts out/clips/<scene>.mp4
./narrate.py             # only after changing a "voice" line or the music; needs ELEVENLABS_API_KEY
node render.mjs out      # draws the captions and cards, out/frames/<scene>/
./compose.py out         # out/treehouse-announcement.mp4, .gif and a poster, and the -latte variants
```

The docs home page shows the app on its own instead, as a silent loop in each theme. `./hero.py out` records the same session in Mocha and in Latte (`THEME=latte` for `record.py`) and writes `docs/public/hero-<theme>.mp4`, `.webm` and a `.jpg` poster.

`render.mjs` needs Playwright with Chromium (`NODE_PATH` pointing at a global install is enough).

| File | What it does |
| --- | --- |
| `scenes.json` | Scene order, captions, narration, themes, the app window's place in the frame |
| `demo-repo.sh` | The "lumen" demo project: two worktrees, one with changes for the diff |
| `run-app.sh` | Starts Treehouse on `:99` with fake-pi and scratch settings |
| `record.py` | Drives the app with xdotool and records it; the pauses follow fake-pi's script |
| `frame.html`, `render.mjs` | One scene's overlay, rendered frame by frame |
| `narrate.py` | Speaks each scene's `voice` line and makes the music with ElevenLabs, into `voice/` |
| `voice/` | The generated narration and music, kept so the video builds without an API key |
| `hero.py` | The looping hero videos for the docs home page |
| `compose.py` | Puts the clips under the overlays, cross-fades the scenes and mixes in the audio |

Captions, card text and the narration (`voice`) live in `scenes.json`; rerun `render.mjs` and `compose.py` after changing them, and `narrate.py` first for a changed line. After a UI change, rerun `record.py` too, and check the click positions in it.

There are two variants: Mocha (dark) and Latte, a light page with the Latte icon (`assets/icon/treehouse-latte.svg`) that makes the dark app stand out more. The app is recorded once, in Mocha, and used in both.

Each line starts half a second into its scene. A scene that is shorter than its line is stretched by holding its last frame, so the video grows with longer lines. The voice, model and music prompt are under `narration` and `music` in `scenes.json`; `music.volume` sets how quiet the music sits under the voice.
