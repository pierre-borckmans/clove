"""Generates the music bed with ElevenLabs Music, shaped to the video's arc.

    python3 tts/music.py            # all styles
    python3 tts/music.py warm       # one style

Section lengths follow the scene starts (see Root.tsx); the key is read
from ~/.elevenlabs_music_key and never printed. Output: tts/music/<style>.mp3
"""
import json, os, sys, urllib.request

KEY = open(os.path.expanduser("~/.elevenlabs_music_key")).read().strip()

# Seconds: playful opening, the problem, the reveal at "Meet clove",
# the demo, the outro.
ARC = [
    ("opening", 12.0, ["light", "playful", "sparse", "inviting"]),
    ("the problem", 56.5, ["curious", "a little tense", "restrained", "building slowly"]),
    ("reveal", 4.5, ["lift", "bright swell", "release"]),
    ("demo", 92.5, ["steady", "confident", "flowing", "optimistic"]),
    ("outro", 8.9, ["resolving", "warm", "gentle ending", "fade out"]),
]

STYLES = {
    "warm": ["warm ambient electronic", "soft synth pads", "gentle pulsing bass", "light arpeggio", "92 bpm"],
    "piano": ["minimal modern piano", "soft felt piano", "subtle synth pad", "airy", "88 bpm"],
    "tech": ["modern tech product music", "clean plucked synths", "soft electronic beat", "subtle bass", "100 bpm"],
}
NEGATIVE = ["vocals", "singing", "spoken word", "loud drums", "heavy distortion", "busy melody"]


def plan(style):
    return {
        "positive_global_styles": STYLES[style] + ["instrumental", "background music for a voice-over"],
        "negative_global_styles": NEGATIVE,
        "sections": [
            {"section_name": name, "positive_local_styles": local, "negative_local_styles": [], "duration_ms": int(sec * 1000), "lines": []}
            for name, sec, local in ARC
        ],
    }


os.makedirs("tts/music", exist_ok=True)
for style in sys.argv[1:] or list(STYLES):
    req = urllib.request.Request(
        "https://api.elevenlabs.io/v1/music?output_format=mp3_44100_128",
        data=json.dumps({"composition_plan": plan(style)}).encode(),
        headers={"xi-api-key": KEY, "content-type": "application/json"},
    )
    try:
        data = urllib.request.urlopen(req, timeout=600).read()
    except urllib.error.HTTPError as e:
        print(style, e.code, e.read()[:400].decode(errors="replace"))
        continue
    open(f"tts/music/{style}.mp3", "wb").write(data)
    print(style, len(data), "bytes")
