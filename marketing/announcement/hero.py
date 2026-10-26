#!/usr/bin/env python3
"""Makes the looping app video on the docs home page, one per theme.

Records the same session as the announcement with record.py, once in Mocha
and once in Latte, and turns each into a silent loop: the end cross-fades
into the start, so the video has no visible seam. Writes
docs/public/hero-<theme>.mp4 and .webm, and a poster image.

Usage: hero.py <out folder>
"""

import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = Path(sys.argv[1] if len(sys.argv) > 1 else "out").resolve()
PUBLIC = HERE.parent.parent / "docs" / "public"
THEMES = ["mocha", "latte"]
FADE = 0.8
# Half the recorded size: sharp on a high-density screen at the hero's width.
SIZE = "1280:800"


def ffmpeg(*args):
    subprocess.run(["ffmpeg", "-v", "error", "-y", *args], check=True)


def loop(raw, marks, name):
    start, end = marks["trust"], marks["diff_end"]
    length = end - start
    # The loop starts FADE seconds in and ends by fading into that same frame.
    graph = (
        f"[0]trim={start}:{end},setpts=PTS-STARTPTS,scale={SIZE}:flags=lanczos,split[a][b];"
        f"[a]trim={FADE},setpts=PTS-STARTPTS[body];"
        f"[b]trim=0:{FADE},setpts=PTS-STARTPTS[head];"
        f"[body][head]xfade=transition=fade:duration={FADE}:offset={length - 2 * FADE:.3f},format=yuv420p[v]"
    )
    mp4, webm = PUBLIC / f"{name}.mp4", PUBLIC / f"{name}.webm"
    ffmpeg("-i", str(raw), "-filter_complex", graph, "-map", "[v]", "-an",
           "-c:v", "libx264", "-preset", "slow", "-crf", "26", "-tune", "animation",
           "-movflags", "+faststart", str(mp4))
    ffmpeg("-i", str(mp4), "-c:v", "libvpx-vp9", "-crf", "38", "-b:v", "0", "-row-mt", "1",
           "-deadline", "good", "-an", str(webm))
    ffmpeg("-i", str(mp4), "-frames:v", "1", "-q:v", "3", str(PUBLIC / f"{name}.jpg"))
    for f in (mp4, webm):
        print(f"{f} ({f.stat().st_size / 1e6:.1f} MB)")


def main():
    for theme in THEMES:
        out = OUT / f"hero-{theme}"
        subprocess.run([str(HERE / "record.py"), str(out)], env=dict(os.environ, THEME=theme),
                       check=True, stdout=subprocess.DEVNULL)
        marks = json.loads((out / "marks.json").read_text())
        loop(out / "raw.mkv", marks, f"hero-{theme}")


if __name__ == "__main__":
    main()
