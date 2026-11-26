"""Writes out/clove-intro.vtt: captions timed from the voice-over's words.

Scene starts follow Root.tsx (LEAD, XFADE, TAIL); each line is cut into
short cues at sentence ends, or every few words.
"""
import json, re

FPS, LEAD, XFADE = 30, 10, 20
MAX_WORDS = 8
script = json.load(open("src/script.json"))
durations = json.load(open("src/durations.json"))["durations"]
words = json.load(open("src/words.json"))
root = open("src/Root.tsx").read()
tail = {k: float(v) for k, v in re.findall(r"^\s+(\w+): ([\d.]+),$", root, re.M)}
assert f"const XFADE = {XFADE};" in root, "XFADE changed in Root.tsx"


def ts(sec):
    h, rem = divmod(sec, 3600)
    m, s = divmod(rem, 60)
    return f"{int(h):02d}:{int(m):02d}:{s:06.3f}"


cues, start = [], 0
for line in script:
    sid = line["id"]
    frames = LEAD + round((durations[sid] + tail.get(sid, 1.4)) * FPS) + XFADE
    t0 = (start + LEAD) / FPS
    end_of_line = t0 + durations[sid]
    # Whitespace tokens keep their punctuation; each holds one or more words.
    tokens, wi = [], 0
    for tok in line["say"].split():
        n = len(re.findall(r"[A-Za-z0-9'-]+", tok))
        if n == 0:
            continue
        tokens.append((tok, t0 + words[sid][wi]["t"]))
        wi += n
    group = []
    for i, (tok, t) in enumerate(tokens):
        group.append((tok, t))
        if tok[-1] in ".!?" or len(group) >= MAX_WORDS or i == len(tokens) - 1:
            # Spoken for the voice, written for the reader.
            text = " ".join(g[0] for g in group).replace("clove dot fast", "clove.fast")
            cues.append([group[0][1], None, text])
            group = []
    # Each cue lasts until the next starts, or the line ends.
    line_cues = [c for c in cues if c[1] is None]
    for a, b in zip(line_cues, line_cues[1:]):
        a[1] = b[0]
    if line_cues:
        line_cues[-1][1] = end_of_line + 0.3
    start += frames - XFADE

with open("out/clove-intro.vtt", "w") as f:
    f.write("WEBVTT\n\n")
    for i, (a, b, text) in enumerate(cues, 1):
        f.write(f"{i}\n{ts(a)} --> {ts(b)}\n{text}\n\n")
print(len(cues), "cues")
