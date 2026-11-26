"""The tech bed in two pieces joined at "Meet clove", so the drop lands
exactly there: A (calm build, ends on a riser) and B (starts on the drop).
Key from ~/.elevenlabs_music_key, never printed. Output: tts/music/tech.mp3
"""
import json, os, subprocess, urllib.request

KEY = open(os.path.expanduser("~/.elevenlabs_music_key")).read().strip()
DROP = 71.6
TOTAL = 175.73
NEG = ["vocals", "singing", "spoken word", "heavy distortion", "harsh", "dubstep wobble"]
STYLE = ["modern tech product music", "clean plucked synths", "electronic", "instrumental", "background music for a voice-over"]

A = {
    "positive_global_styles": STYLE + ["110 bpm", "no kick drum yet"],
    "negative_global_styles": NEG + ["four on the floor", "drop"],
    "sections": [
        {"section_name": "opening", "positive_local_styles": ["light", "playful", "sparse"], "negative_local_styles": [], "duration_ms": 11900, "lines": []},
        {"section_name": "build", "positive_local_styles": ["curious", "a little tense", "restrained", "slowly building anticipation"], "negative_local_styles": [], "duration_ms": 54700, "lines": []},
        {"section_name": "riser", "positive_local_styles": ["rising tension", "filter sweep up", "snare roll", "ends on the edge of a drop"], "negative_local_styles": [], "duration_ms": int((DROP - 66.6) * 1000), "lines": []},
    ],
}
B = {
    "positive_global_styles": STYLE + ["melodic techno", "122 bpm", "four on the floor"],
    "negative_global_styles": NEG,
    "sections": [
        {"section_name": "drop", "positive_local_styles": ["starts immediately on the drop", "vivid", "euphoric", "full energy from the first beat"], "negative_local_styles": ["intro", "build-up"], "duration_ms": 14000, "lines": []},
        {"section_name": "drive", "positive_local_styles": ["driving", "propulsive", "confident", "evolving synth lines"], "negative_local_styles": [], "duration_ms": int((TOTAL - DROP - 14 - 9) * 1000), "lines": []},
        {"section_name": "outro", "positive_local_styles": ["beat falls away", "warm", "resolving", "fade out"], "negative_local_styles": [], "duration_ms": 9000, "lines": []},
    ],
}


def gen(plan, out):
    req = urllib.request.Request(
        "https://api.elevenlabs.io/v1/music?output_format=mp3_44100_128",
        data=json.dumps({"composition_plan": plan}).encode(),
        headers={"xi-api-key": KEY, "content-type": "application/json"},
    )
    open(out, "wb").write(urllib.request.urlopen(req, timeout=600).read())


gen(A, "tts/music/tech-a.mp3")
gen(B, "tts/music/tech-b.mp3")
# A trimmed to exactly DROP seconds, B straight after, with a 40ms crossfade.
subprocess.run(
    ["ffmpeg", "-y", "-loglevel", "error", "-i", "tts/music/tech-a.mp3", "-i", "tts/music/tech-b.mp3", "-filter_complex",
     f"[0]atrim=0:{DROP + 0.04},asetpts=N/SR/TB[a];[1]asetpts=N/SR/TB[b];[a][b]acrossfade=d=0.04:c1=tri:c2=tri,atrim=0:{TOTAL}",
     "-b:a", "192k", "tts/music/tech.mp3"],
    check=True,
)
print("tech.mp3 written")
