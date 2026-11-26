"""Voices src/script.json with Chatterbox into public/vo/<id>.wav and writes
src/durations.json, which sets each scene's length.

Each sentence is generated on its own, in several takes; the take whose pitch
falls at the end is kept (endings drift upward on long generations), and the
sentences are joined with short pauses. tts/takes/<id>/ keeps every take;
tts/picks.json can pin one by ear: {"cost.2": 3} keeps take 3 of cost's
third sentence.

    python tts/chatterbox_vo.py                  # Chatterbox's own voice
    python tts/chatterbox_vo.py --ref af_heart   # clone a reference in tts/refs/
    python tts/chatterbox_vo.py --only golden    # redo one line

Reference clips in tts/refs/ are Kokoro preset voices (Apache 2.0), not people.
"""
import argparse, json, os, re, subprocess
import librosa, numpy as np, perth, soundfile as sf, torch

if perth.PerthImplicitWatermarker is None:  # fails to load on some installs; inaudible anyway
    perth.PerthImplicitWatermarker = perth.DummyWatermarker
from chatterbox.tts import ChatterboxTTS

ap = argparse.ArgumentParser()
ap.add_argument("--ref", help="name of a clip in tts/refs/ to clone")
ap.add_argument("--exaggeration", type=float, default=0.35)
ap.add_argument("--cfg", type=float, default=0.35)
ap.add_argument("--takes", type=int, default=5)
ap.add_argument("--only", nargs="*", help="ids to regenerate")
a = ap.parse_args()

device = "mps" if torch.backends.mps.is_available() else "cpu"
model = ChatterboxTTS.from_pretrained(device=device)
sr = model.sr
ref = f"tts/refs/{a.ref}.wav" if a.ref else None
picks = json.load(open("tts/picks.json")) if os.path.exists("tts/picks.json") else {}


def trim(x):
    y, _ = librosa.effects.trim(x, top_db=40)
    return y


def ending_slope(x):
    """Pitch change over the last voiced half-second, in semitones; negative falls."""
    f0, voiced, _ = librosa.pyin(x, fmin=70, fmax=400, sr=sr, frame_length=1024)
    f0 = f0[voiced]
    if len(f0) < 12:
        return 0.0
    tail = f0[-max(6, len(f0) // 5):]
    head = f0[-len(f0) // 2 : -len(tail)] if len(f0) > 2 * len(tail) else f0[: len(f0) // 2]
    return float(12 * np.log2(np.median(tail) / np.median(head)))


lines = json.load(open("src/script.json"))
os.makedirs("public/vo", exist_ok=True)
dur_path = "src/durations.json"
durs = json.load(open(dur_path)) if os.path.exists(dur_path) else {"durations": {}}
for line in lines:
    lid = line["id"]
    if a.only and lid not in a.only:
        continue
    sentences = [s for s in re.split(r"(?<=[.!?])\s+", line["say"].strip()) if s]
    os.makedirs(f"tts/takes/{lid}", exist_ok=True)
    parts, report = [], []
    for si, sentence in enumerate(sentences):
        takes = []
        for t in range(a.takes):
            torch.manual_seed(1000 * si + t)
            wav = model.generate(sentence, audio_prompt_path=ref, exaggeration=a.exaggeration, cfg_weight=a.cfg)
            x = trim(wav.squeeze().cpu().numpy())
            sf.write(f"tts/takes/{lid}/{si}-{t}.wav", x, sr)
            takes.append((ending_slope(x), len(x), x))
        # Drop outliers in length (garbled or rushed takes), then the most falling ending.
        med = np.median([n for _, n, _ in takes])
        ok = [i for i, (_, n, _) in enumerate(takes) if 0.75 * med <= n <= 1.35 * med] or list(range(len(takes)))
        best = picks.get(f"{lid}.{si}", min(ok, key=lambda i: takes[i][0]))
        parts.append(takes[best][2])
        report.append(f"{si}:take {best} ({takes[best][0]:+.1f} st)")
    gap = np.zeros(int(0.28 * sr), dtype=np.float32)
    joined = np.concatenate([p for x in parts for p in (x, gap)][:-1])
    raw = f"/tmp/clove-vo-{lid}.wav"
    sf.write(raw, joined, sr)
    out = f"public/vo/{lid}.wav"
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", raw, "-ar", "48000", "-ac", "1", out], check=True)
    secs = len(joined) / sr
    durs["durations"][lid] = secs
    print(f"{lid:9} {secs:5.2f}s  " + "  ".join(report), flush=True)
    json.dump(durs | {"voice": f"chatterbox:{a.ref or 'default'}"}, open(dur_path, "w"), indent=2)
