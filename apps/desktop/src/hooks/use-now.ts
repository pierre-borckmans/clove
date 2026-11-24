import { useEffect, useState } from "react";

/** Current time in ms, ticking while `active`. */
export function useNow(active: boolean, every = 100): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const t = setInterval(() => setNow(Date.now()), every);
    return () => clearInterval(t);
  }, [active, every]);
  return active ? now : Date.now();
}
