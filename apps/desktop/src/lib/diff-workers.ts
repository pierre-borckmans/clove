import type { FileDiffMetadata } from "@pierre/diffs";
import HighlightWorkerUrl from "@pierre/diffs/worker/worker.js?worker&url";
import { useEffect, useState } from "react";

/** Shiki runs in these, so highlighting never blocks the UI. */
export function highlightWorker(): Worker {
  return new Worker(HighlightWorkerUrl, { type: "module" });
}

let parser: Worker | undefined;
let seq = 0;
const pending = new Map<number, { resolve: (f: FileDiffMetadata[]) => void; reject: (e: Error) => void }>();

function parsePatch(patch: string): Promise<FileDiffMetadata[]> {
  if (!parser) {
    parser = new Worker(new URL("./parse-patch.worker.ts", import.meta.url), { type: "module" });
    parser.onmessage = (e: MessageEvent<{ id: number; files?: FileDiffMetadata[]; error?: string }>) => {
      const p = pending.get(e.data.id);
      pending.delete(e.data.id);
      if (e.data.files) p?.resolve(e.data.files);
      else p?.reject(new Error(e.data.error ?? "could not parse the patch"));
    };
  }
  const id = ++seq;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    parser?.postMessage({ id, patch });
  });
}

/**
 * The patch's files, parsed in a worker. `files` keeps the previous result
 * while a new patch parses, so a refresh never blanks the view.
 */
export function useParsedPatch(patch: string | undefined) {
  const [state, setState] = useState<{
    patch?: string;
    files?: FileDiffMetadata[];
    error?: string;
    /** Bumped for every new parse; item versions derive from it. */
    generation: number;
  }>({ generation: 0 });
  useEffect(() => {
    if (patch === undefined || patch === state.patch) return;
    if (patch === "") {
      setState((s) => ({ patch, files: [], generation: s.generation + 1 }));
      return;
    }
    let live = true;
    parsePatch(patch).then(
      (files) => live && setState((s) => ({ patch, files, generation: s.generation + 1 })),
      (e: Error) => live && setState((s) => ({ ...s, patch, error: e.message })),
    );
    return () => {
      live = false;
    };
  }, [patch, state.patch]);
  return { files: state.files, error: state.error, generation: state.generation, parsing: patch !== state.patch };
}
