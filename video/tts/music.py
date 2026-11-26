"""Generates the music bed with ElevenLabs Music, shaped to the video's arc.

    python3 tts/music.py            # all styles
    python3 tts/music.py warm       # one style

Section lengths follow the scene starts (see Root.tsx); the key is read
from ~/.elevenlabs_music_key and never printed. Output: tts/music/<style>.mp3
"""
import json, os, sys, urllib.request

KEY = open(os.path.expanduser("~/.elevenlabs_music_key")).read().strip()

# Seconds. Calm and restrained until "Meet clove" (71.6s), then the mood
# turns: vivid, driving techno through the demo, resolving at the outro.
ARC = [
    ("opening", 11.9, ["light", "playful", "sparse", "inviting"]),
    ("the problem", 56.6, ["curious", "a little tense", "restrained", "building slowly", "anticipation"]),
    ("the turn", 3.1, ["tension peaks", "riser", "short silence before the drop"]),
    ("the drop", 12.0, ["the beat drops", "vivid", "energetic", "euphoric lift", "four on the floor"]),
    ("demo", 83.3, ["driving melodic techno", "vivid", "propulsive", "confident", "evolving synth lines"]),
    ("outro", 8.83, ["resolving", "warm", "beat falls away", "gentle ending", "fade out"]),
]

STYLES = {
    "warm": ["warm ambient electronic", "soft synth pads", "gentle pulsing bass", "light arpeggio", "92 bpm"],
    "piano": ["minimal modern piano", "soft felt piano", "subtle synth pad", "airy", "88 bpm"],
    "tech": ["modern tech product music", "clean plucked synths", "electronic", "melodic techno after the drop", "110 bpm before the drop, 122 bpm after"],
}
NEGATIVE = ["vocals", "singing", "spoken word", "heavy distortion", "harsh", "dubstep wobble"]


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
