"""Voices src/script.json with ElevenLabs as ONE continuous read, so
intonation follows the whole narrative, then cuts it into scenes at the
line boundaries using the character timestamps ElevenLabs returns.

    python3 tts/elevenlabs_vo.py --voice Sarah --audition   # full read only, to listen
    python3 tts/elevenlabs_vo.py --voice Sarah              # full read, cut into public/vo/<id>.wav
    python3 tts/elevenlabs_vo.py --voice Eric --only logo   # re-voice one line, the others untouched

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
ap.add_argument("--splice", help="take a full read, but cut only these line ids from it, comma-separated (the others keep their audio)")
ap.add_argument("--only", help="re-voice just this line id, with its neighbours as context")
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
settings = {"stability": a.stability, "similarity_boost": 0.8, "style": a.style, "use_speaker_boost": True, "speed": a.speed}


def cut(src, t0, t1, out):
    subprocess.run(
        ["ffmpeg", "-y", "-loglevel", "error", "-i", src, "-ss", f"{t0:.3f}", "-to", f"{t1:.3f}",
         "-af", "afade=t=in:d=0.02,areverse,afade=t=in:d=0.06,areverse", "-ar", "48000", "-ac", "1", out],
        check=True,
    )


def trim_tail(out):
    """Drops a stray sound (a hum, a breath) after the line has gone quiet near
    its end. Returns the new length, or None if nothing was cut."""
    dur = float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", out],
                               capture_output=True, text=True, check=True).stdout)
    log = subprocess.run(["ffmpeg", "-i", out, "-af", "silencedetect=n=-45dB:d=0.12", "-f", "null", "-"],
                         capture_output=True, text=True).stderr
    starts = [float(l.split("silence_start: ")[1]) for l in log.splitlines() if "silence_start" in l]
    tail = [t for t in starts if dur - 0.8 < t < dur - 0.15]
    if not tail:
        return None
    end = tail[0] + 0.08
    tmp = out + ".tmp.wav"
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", out, "-t", f"{end:.3f}",
                    "-af", "areverse,afade=t=in:d=0.06,areverse", tmp], check=True)
    os.replace(tmp, out)
    return round(end, 3)


def word_times(text, st, s, e, t0):
    """When each word of text[s:e] starts, relative to t0: visuals cue on words."""
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
    return ws


if a.only:
    ids = [l["id"] for l in lines]
    i = ids.index(a.only)
    say = lines[i]["say"]
    body = {"text": say, "model_id": a.model, "voice_settings": settings,
            "previous_text": "\n\n".join(l["say"] for l in lines[max(0, i - 2):i]),
            "next_text": lines[i + 1]["say"] if i + 1 < len(lines) else ""}
    res = call(f"/text-to-speech/{voice['voice_id']}/with-timestamps?output_format=mp3_44100_128", body)
    os.makedirs("tts/eleven/lines", exist_ok=True)
    src = f"tts/eleven/lines/{name}-{a.only}.mp3"
    open(src, "wb").write(base64.b64decode(res["audio_base64"]))
    al = res.get("alignment") or res.get("normalized_alignment")
    st, en = al["character_start_times_seconds"], al["character_end_times_seconds"]
    t0, t1 = max(0.0, st[0] - 0.06), en[len(say) - 1] + 0.12
    cut(src, t0, t1, f"public/vo/{a.only}.wav")
    durs = json.load(open("src/durations.json"))
    durs["durations"][a.only] = trim_tail(f"public/vo/{a.only}.wav") or round(t1 - t0, 3)
    words = json.load(open("src/words.json"))
    words[a.only] = word_times(say, st, 0, len(say), t0)
    json.dump(durs, open("src/durations.json", "w"), indent=2)
    json.dump(words, open("src/words.json", "w"), indent=1)
    print(f"{a.only:9} {t1 - t0:5.2f}s (one line, {len(say)} characters)")
    raise SystemExit(0)

os.makedirs("tts/eleven", exist_ok=True)
splice = set(a.splice.split(",")) if a.splice else set()
base = f"tts/eleven/{name}-{a.model}" + (f"-splice-{a.splice.replace(',', '+')}" if a.splice else "")
if a.reuse:
    al = json.load(open(base + ".json"))
else:
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
if a.splice:
    durs = json.load(open("src/durations.json"))
    words = json.load(open("src/words.json"))
for idx, (l, (s, e)) in enumerate(zip(lines, starts)):
    if splice and l["id"] not in splice:
        continue
    t0 = max(0.0, st[s] - 0.06)
    t1 = en[e - 1] + 0.12
    # Never into the next line's first sound.
    if idx + 1 < len(lines):
        t1 = min(t1, st[starts[idx + 1][0]] - 0.02)
    out = f"public/vo/{l['id']}.wav"
    cut(base + ".mp3", t0, t1, out)
    durs["durations"][l["id"]] = trim_tail(out) or round(t1 - t0, 3)
    words[l["id"]] = word_times(text, st, s, e, t0)
    print(f"{l['id']:9} {t1 - t0:5.2f}s")
if not a.splice:
    durs["voice"] = f"elevenlabs:{name}:{a.model}"
json.dump(durs, open("src/durations.json", "w"), indent=2)
json.dump(words, open("src/words.json", "w"), indent=1)
