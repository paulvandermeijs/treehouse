#!/usr/bin/env python3
"""Speaks each scene's "voice" line from scenes.json with ElevenLabs, and
makes the background music, into voice/<scene id>.mp3 and voice/music.mp3.

Usage: narrate.py [scene id | music ...]

Needs ELEVENLABS_API_KEY. Only lines and music whose text changed since the
last run are made again (voice/made.json remembers what each file says), so a
rerun costs nothing; name scenes, or "music", to remake them anyway.
"""

import json
import os
import sys
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
VOICE = HERE / "voice"
SPEC = json.loads((HERE / "scenes.json").read_text())
NARRATION = SPEC["narration"]
API = "https://api.elevenlabs.io/v1"


def post(path, body, target):
    request = urllib.request.Request(
        f"{API}{path}?output_format=mp3_44100_128",
        data=json.dumps(body).encode(),
        headers={"xi-api-key": os.environ["ELEVENLABS_API_KEY"], "content-type": "application/json"},
    )
    with urllib.request.urlopen(request) as response:
        target.write_bytes(response.read())


def main():
    if not os.environ.get("ELEVENLABS_API_KEY"):
        sys.exit("Set ELEVENLABS_API_KEY")
    VOICE.mkdir(exist_ok=True)
    made_file = VOICE / "made.json"
    made = json.loads(made_file.read_text()) if made_file.exists() else {}
    force = set(sys.argv[1:])

    jobs = {}
    for scene in SPEC["scenes"]:
        if scene.get("voice"):
            jobs[scene["id"]] = (
                f"/text-to-speech/{NARRATION['voice_id']}",
                {
                    "text": scene["voice"],
                    "model_id": NARRATION["model_id"],
                    "voice_settings": NARRATION["settings"],
                },
            )
    music = SPEC.get("music")
    if music:
        jobs["music"] = ("/music", {"prompt": music["prompt"], "music_length_ms": music["seconds"] * 1000})

    for name, (path, body) in jobs.items():
        target = VOICE / f"{name}.mp3"
        if name not in force and target.exists() and made.get(name) == body:
            continue
        print(f"{name}: {body.get('text') or body['prompt']}")
        post(path, body, target)
        made[name] = body
        made_file.write_text(json.dumps(made, indent=2) + "\n")


if __name__ == "__main__":
    main()
