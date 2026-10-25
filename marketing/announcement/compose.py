#!/usr/bin/env python3
"""Puts the announcement together from render.mjs's overlay frames and
record.py's app clips, then writes the video, a GIF and a poster image.

Usage: compose.py <out folder>

Each chapter lasts as long as its clip in <out>/clips/<id>.mp4; the title and
end card last their "hold" from scenes.json. A scene whose narration line
(voice/<id>.mp3, made by narrate.py) runs longer is stretched by holding its
last frame. Scenes cross-fade into each other; the voice and quiet background
music are mixed under them.

There's one video per theme in scenes.json: Mocha's is
treehouse-announcement.mp4, the others add their name, such as
treehouse-announcement-latte.mp4.
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
VOICE = HERE / "voice"
NARRATION = SPEC["narration"]


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


def voice(scene):
    path = VOICE / f"{scene['id']}.mp3"
    return path if scene.get("voice") and path.exists() else None


def needed(scene):
    """How long the scene must last for its line: the line starts "lead" in
    and leaves "tail" after it, which also covers the cross-fade out."""
    line = voice(scene)
    return NARRATION["lead"] + duration(line) + NARRATION["tail"] if line else 0.0


def scene_video(scene, theme, base):
    frames = OUT / "frames" / theme / scene["id"] / "%03d.png"
    target = OUT / "scenes" / theme / f"{scene['id']}.mp4"
    target.parent.mkdir(parents=True, exist_ok=True)
    if scene["kind"] == "chapter":
        clip = OUT / "clips" / f"{scene['id']}.mp4"
        recorded = duration(clip)
        length = max(recorded, needed(scene))
        ffmpeg(
            "-f", "lavfi", "-i", f"color=c={base}:s={W}x{H}:r={FPS}:d={length}",
            "-i", str(clip),
            "-framerate", str(FPS), "-i", str(frames),
            "-filter_complex",
            # The overlay's intro frames, then its last frame held to the end.
            f"[2]tpad=stop_mode=clone:stop_duration={length}[fg];"
            # The app clip's last frame is held while the line finishes.
            f"[1]tpad=stop_mode=clone:stop_duration={length - recorded:.3f},"
            f"scale={WIN['w']}:{WIN['h']}:flags=lanczos,setsar=1,fps={FPS},fade=in:d=0.5[app];"
            f"[0][app]overlay={WIN['x']}:{WIN['y']}[a];"
            f"[a][fg]overlay=shortest=1,trim=duration={length}",
            *encode(), "-an", str(target),
        )
    else:
        length = max(INTRO + scene["hold"], needed(scene))
        ffmpeg(
            "-framerate", str(FPS), "-i", str(frames),
            "-filter_complex",
            f"color=c={base}:s={W}x{H}:r={FPS}[bg];"
            f"[0]tpad=stop_mode=clone:stop_duration={length}[fg];"
            f"[bg][fg]overlay=shortest=1,trim=duration={length}",
            *encode(), "-an", str(target),
        )
    return target, length


def make(theme):
    base = SPEC["themes"][theme]["base"].replace("#", "0x")
    parts = [scene_video(s, theme, base) for s in SPEC["scenes"]]
    inputs, chain, offset, starts = [], "", 0.0, [0.0]
    for i, (path, _) in enumerate(parts):
        inputs += ["-i", str(path)]
    last = "[0]"
    for i in range(1, len(parts)):
        offset += parts[i - 1][1] - FADE
        starts.append(offset)
        label = f"[x{i}]"
        chain += f"{last}[{i}]xfade=transition=fade:duration={FADE}:offset={offset:.3f}{label};"
        last = label
    chain += f"{last}fade=in:d=0.4:c={base},format=yuv420p[v];"
    total = offset + parts[-1][1]

    # Each line starts "lead" into its scene; the music runs under all of it
    # and fades out with the end card.
    voices = []
    for scene, start in zip(SPEC["scenes"], starts):
        line = voice(scene)
        if line:
            n = len(parts) + len(voices)
            inputs += ["-i", str(line)]
            delay = round((start + NARRATION["lead"]) * 1000)
            chain += f"[{n}]aresample=48000,adelay={delay}:all=1[v{n}];"
            voices.append(f"[v{n}]")
    music = SPEC.get("music")
    if music and (VOICE / "music.mp3").exists():
        n = len(parts) + len(voices)
        inputs += ["-i", str(VOICE / "music.mp3")]
        chain += (
            f"[{n}]aresample=48000,atrim=duration={total:.3f},volume={music['volume']},"
            f"afade=in:d=1,afade=out:st={total - 2.5:.3f}:d=2.5[v{n}];"
        )
        voices.append(f"[v{n}]")
    if voices:
        chain += (
            f"{''.join(voices)}amix=inputs={len(voices)}:duration=longest:normalize=0,"
            # Lifts the mix towards the -16 LUFS that web video players expect.
            f"loudnorm=I=-16:TP=-1.5:LRA=11,aresample=48000,apad,atrim=duration={total:.3f}[a]"
        )
        audio = ["-map", "[a]", "-c:a", "aac", "-b:a", "192k"]
    else:
        chain = chain.rstrip(";")
        audio = []
    suffix = "" if theme == "mocha" else f"-{theme}"
    video = OUT / f"treehouse-announcement{suffix}.mp4"
    ffmpeg(*inputs, "-filter_complex", chain, "-map", "[v]", *audio, *encode(), "-movflags", "+faststart", str(video))

    # A small looping GIF for the README, and the end card as a poster.
    gif = OUT / f"treehouse-announcement{suffix}.gif"
    ffmpeg(
        "-i", str(video), "-filter_complex",
        "fps=12,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128[p];[b][p]paletteuse=dither=bayer",
        str(gif),
    )
    poster = OUT / f"treehouse-poster{suffix}.png"
    ffmpeg("-sseof", "-1", "-i", str(video), "-frames:v", "1", str(poster))
    print(f"{video} ({duration(video):.1f}s)\n{gif}\n{poster}")


if __name__ == "__main__":
    for theme in SPEC["themes"]:
        make(theme)
