#!/usr/bin/env python3
"""Puts the announcement together from render.mjs's overlay frames and
record.py's app clips, then writes the video, a GIF and a poster image.

Usage: compose.py <out folder>

Each chapter lasts as long as its clip in <out>/clips/<id>.mp4; the title and
end card last their "hold" from scenes.json. Scenes cross-fade into each other.
"""

import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = Path(sys.argv[1] if len(sys.argv) > 1 else "out").resolve()
SPEC = json.loads((HERE / "scenes.json").read_text())
FPS, W, H = SPEC["fps"], SPEC["width"], SPEC["height"]
INTRO = 1.6  # matches INTRO_SECONDS in render.mjs
FADE = 0.4
WIN = SPEC["window"]
BASE = "0x1e1e2e"


def ffmpeg(*args):
    subprocess.run(["ffmpeg", "-v", "error", "-y", *args], check=True)


def duration(path):
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)],
        check=True, capture_output=True, text=True,
    ).stdout
    return float(out)


def encode():
    return ["-c:v", "libx264", "-preset", "slow", "-crf", "16", "-pix_fmt", "yuv420p", "-r", str(FPS)]


def scene_video(scene):
    frames = OUT / "frames" / scene["id"] / "%03d.png"
    target = OUT / "scenes" / f"{scene['id']}.mp4"
    target.parent.mkdir(parents=True, exist_ok=True)
    if scene["kind"] == "chapter":
        clip = OUT / "clips" / f"{scene['id']}.mp4"
        length = duration(clip)
        ffmpeg(
            "-f", "lavfi", "-i", f"color=c={BASE}:s={W}x{H}:r={FPS}:d={length}",
            "-i", str(clip),
            "-framerate", str(FPS), "-i", str(frames),
            "-filter_complex",
            # The overlay's intro frames, then its last frame held to the end.
            f"[2]tpad=stop_mode=clone:stop_duration={length}[fg];"
            f"[1]scale={WIN['w']}:{WIN['h']}:flags=lanczos,setsar=1,fps={FPS},fade=in:d=0.5[app];"
            f"[0][app]overlay={WIN['x']}:{WIN['y']}[a];"
            f"[a][fg]overlay=shortest=1,trim=duration={length}",
            *encode(), "-an", str(target),
        )
    else:
        length = INTRO + scene["hold"]
        ffmpeg(
            "-framerate", str(FPS), "-i", str(frames),
            "-filter_complex",
            f"color=c={BASE}:s={W}x{H}:r={FPS}[bg];"
            f"[0]tpad=stop_mode=clone:stop_duration={length}[fg];"
            f"[bg][fg]overlay=shortest=1,trim=duration={length}",
            *encode(), "-an", str(target),
        )
    return target, length


def main():
    parts = [scene_video(s) for s in SPEC["scenes"]]
    inputs, chain, offset = [], "", 0.0
    for i, (path, _) in enumerate(parts):
        inputs += ["-i", str(path)]
    last = "[0]"
    for i in range(1, len(parts)):
        offset += parts[i - 1][1] - FADE
        label = f"[x{i}]"
        chain += f"{last}[{i}]xfade=transition=fade:duration={FADE}:offset={offset:.3f}{label};"
        last = label
    chain += f"{last}fade=in:d=0.4,format=yuv420p[v]"
    video = OUT / "treehouse-announcement.mp4"
    ffmpeg(*inputs, "-filter_complex", chain, "-map", "[v]", *encode(), "-movflags", "+faststart", str(video))

    # A small looping GIF for the README, and the end card as a poster.
    gif = OUT / "treehouse-announcement.gif"
    ffmpeg(
        "-i", str(video), "-filter_complex",
        "fps=12,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128[p];[b][p]paletteuse=dither=bayer",
        str(gif),
    )
    poster = OUT / "treehouse-poster.png"
    ffmpeg("-sseof", "-1", "-i", str(video), "-frames:v", "1", str(poster))
    print(f"{video} ({duration(video):.1f}s)\n{gif}\n{poster}")


if __name__ == "__main__":
    main()
