"""Voices src/script.json with ElevenLabs as ONE continuous read, so
intonation follows the whole narrative, then cuts it into scenes at the
line boundaries using the character timestamps ElevenLabs returns.

    python3 tts/elevenlabs_vo.py --voice Sarah --audition   # full read only, to listen
    python3 tts/elevenlabs_vo.py --voice Sarah              # full read, cut into public/vo/<id>.wav

The API key is read from ~/.elevenlabs_key and never printed.
Full reads are kept in tts/eleven/<voice>-<model>.mp3 (and their timestamps).
"""
import argparse, base64, json, os, subprocess, urllib.request

ap = argparse.ArgumentParser()
ap.add_argument("--voice", default="Sarah", help="voice name (or id) from the account's library")
ap.add_argument("--model", default="eleven_multilingual_v2", help="eleven_multilingual_v2 or eleven_v3")
ap.add_argument("--stability", type=float, default=0.5)
ap.add_argument("--style", type=float, default=0.15)
ap.add_argument("--speed", type=float, default=0.98)
ap.add_argument("--audition", action="store_true", help="only make the full read")
ap.add_argument("--reuse", action="store_true", help="cut the full read already in tts/eleven/ instead of a new take")
a = ap.parse_args()

KEY = open(os.path.expanduser("~/.elevenlabs_key")).read().strip()
API = "https://api.elevenlabs.io/v1"


def call(path, body=None):
    req = urllib.request.Request(
        API + path,
        data=json.dumps(body).encode() if body is not None else None,
        headers={"xi-api-key": KEY, "content-type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=300) as r:
        return json.load(r)


voices = call("/voices")["voices"]
voice = next((v for v in voices if v["voice_id"] == a.voice or v["name"].split(" ")[0].lower() == a.voice.lower()), None)
if voice is None:
    raise SystemExit(f"no voice {a.voice!r}; have: {', '.join(v['name'].split(' ')[0] for v in voices)}")

lines = json.load(open("src/script.json"))
# Blank lines between scenes give natural pauses to cut in.
text = "\n\n".join(l["say"] for l in lines)
starts, pos = [], 0
for l in lines:
    i = text.index(l["say"], pos)
    starts.append((i, i + len(l["say"])))
    pos = i + len(l["say"])

name = voice["name"].split(" ")[0]
os.makedirs("tts/eleven", exist_ok=True)
base = f"tts/eleven/{name}-{a.model}"
if a.reuse:
    al = json.load(open(base + ".json"))
else:
    settings = {"stability": a.stability, "similarity_boost": 0.8, "style": a.style, "use_speaker_boost": True, "speed": a.speed}
    res = call(
        f"/text-to-speech/{voice['voice_id']}/with-timestamps?output_format=mp3_44100_128",
        {"text": text, "model_id": a.model, "voice_settings": settings},
    )
    open(base + ".mp3", "wb").write(base64.b64decode(res["audio_base64"]))
    al = res.get("alignment") or res.get("normalized_alignment")
    json.dump(al, open(base + ".json", "w"))
total = al["character_end_times_seconds"][-1]
print(f"full read: {base}.mp3 ({total:.1f}s, {len(text)} characters)")
if a.audition:
    raise SystemExit(0)

st, en = al["character_start_times_seconds"], al["character_end_times_seconds"]
os.makedirs("public/vo", exist_ok=True)
durs = {"durations": {}}
words = {}
for idx, (l, (s, e)) in enumerate(zip(lines, starts)):
    t0 = max(0.0, st[s] - 0.06)
    t1 = en[e - 1] + 0.12
    # Never into the next line's first sound.
    if idx + 1 < len(lines):
        t1 = min(t1, st[starts[idx + 1][0]] - 0.02)
    out = f"public/vo/{l['id']}.wav"
    subprocess.run(
        ["ffmpeg", "-y", "-loglevel", "error", "-i", base + ".mp3", "-ss", f"{t0:.3f}", "-to", f"{t1:.3f}",
         "-af", "afade=t=in:d=0.02,areverse,afade=t=in:d=0.06,areverse", "-ar", "48000", "-ac", "1", out],
        check=True,
    )
    durs["durations"][l["id"]] = round(t1 - t0, 3)
    # When each word starts, relative to the scene's clip: visuals cue on words.
    ws, w, w0 = [], "", None
    for k in range(s, e + 1):
        ch = text[k] if k < e else " "
        if ch.isalnum() or ch in "'-":
            if w0 is None:
                w0 = st[k]
            w += ch
        elif w:
            ws.append({"w": w.lower(), "t": round(w0 - t0, 3)})
            w, w0 = "", None
    words[l["id"]] = ws
    print(f"{l['id']:9} {t1 - t0:5.2f}s")
durs["voice"] = f"elevenlabs:{name}:{a.model}"
json.dump(durs, open("src/durations.json", "w"), indent=2)
json.dump(words, open("src/words.json", "w"), indent=1)
