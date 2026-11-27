// Parses a git patch off the main thread: a big branch is megabytes of text.
import { parsePatchFiles } from "@pierre/diffs";

self.onmessage = (e: MessageEvent<{ id: number; patch: string }>) => {
  const { id, patch } = e.data;
  try {
    const files = parsePatchFiles(patch).flatMap((p) => p.files);
    self.postMessage({ id, files });
  } catch (err) {
    self.postMessage({ id, error: String(err) });
  }
};
